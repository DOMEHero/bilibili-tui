use crate::api::cdn::{CdnCandidate, RankedStreams, record_cdn_result};
use anyhow::{Context, Result, anyhow};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE, REFERER, USER_AGENT};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot};

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36";

struct ProxyState {
    client: reqwest::Client,
    video: Vec<CdnCandidate>,
    audio: Vec<CdnCandidate>,
    video_index: AtomicUsize,
    prefixes: Mutex<HashMap<usize, CachedPrefix>>,
}

struct CachedPrefix {
    bytes: Vec<u8>,
    total: u64,
}

pub struct MediaProxy {
    state: Arc<ProxyState>,
    pub video_url: String,
    pub audio_url: String,
    shutdown: Option<oneshot::Sender<()>>,
}

impl MediaProxy {
    pub async fn start(streams: RankedStreams) -> Result<Self> {
        if streams.video.is_empty() || streams.audio.is_empty() {
            return Err(anyhow!("CDN 候选地址为空"));
        }
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let state = Arc::new(ProxyState {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(3))
                .build()?,
            video: streams.video,
            audio: streams.audio,
            video_index: AtomicUsize::new(0),
            prefixes: Mutex::new(HashMap::new()),
        });
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let server_state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { break };
                        let state = server_state.clone();
                        tokio::spawn(async move { let _ = serve(socket, state).await; });
                    }
                }
            }
        });
        prefetch_backup(state.clone());
        Ok(Self {
            state,
            video_url: format!("http://{address}/video?generation=0"),
            audio_url: format!("http://{address}/audio"),
            shutdown: Some(shutdown_tx),
        })
    }

    pub fn switch_video_cdn(&mut self) -> Option<String> {
        self.advance_video_cdn(true)
    }

    #[cfg(test)]
    pub fn switch_video_cdn_for_test(&mut self) -> Option<String> {
        self.advance_video_cdn(false)
    }

    fn advance_video_cdn(&mut self, record_corruption: bool) -> Option<String> {
        let current = self.state.video_index.load(Ordering::Relaxed);
        let next = current + 1;
        if next >= self.state.video.len() {
            return None;
        }
        if record_corruption {
            record_cdn_result(&self.state.video[current].host, true);
        }
        self.state.video_index.store(next, Ordering::Relaxed);
        prefetch_backup(self.state.clone());
        let base = self.video_url.split('?').next()?;
        self.video_url = format!("{base}?generation={next}");
        Some(self.video_url.clone())
    }

    pub fn record_success(&self) {
        let index = self.state.video_index.load(Ordering::Relaxed);
        if let Some(candidate) = self.state.video.get(index) {
            record_cdn_result(&candidate.host, false);
        }
    }
}

impl Drop for MediaProxy {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

fn prefetch_backup(state: Arc<ProxyState>) {
    let next = state.video_index.load(Ordering::Relaxed) + 1;
    let Some(candidate) = state.video.get(next).cloned() else {
        return;
    };
    tokio::spawn(async move {
        let Ok(mut response) = state
            .client
            .get(candidate.url)
            .header(RANGE, "bytes=0-1048575")
            .header(REFERER, "https://www.bilibili.com/")
            .header(USER_AGENT, UA)
            .send()
            .await
            .and_then(|response| response.error_for_status())
        else {
            return;
        };
        let total = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.rsplit('/').next())
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut bytes = Vec::with_capacity(1024 * 1024);
        while let Ok(Some(chunk)) = response.chunk().await {
            bytes.extend_from_slice(&chunk);
            if bytes.len() >= 1024 * 1024 {
                break;
            }
        }
        if !bytes.is_empty() {
            state
                .prefixes
                .lock()
                .await
                .insert(next, CachedPrefix { bytes, total });
        }
    });
}

fn parse_range(value: &str) -> Option<(usize, Option<usize>)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    Some((
        start.parse().ok()?,
        (!end.is_empty()).then(|| end.parse().ok()).flatten(),
    ))
}

async fn serve(mut socket: TcpStream, state: Arc<ProxyState>) -> Result<()> {
    let mut request = Vec::with_capacity(4096);
    let mut buffer = [0u8; 1024];
    while !request.windows(4).any(|value| value == b"\r\n\r\n") && request.len() < 32 * 1024 {
        let read = socket.read(&mut buffer).await?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let request = String::from_utf8_lossy(&request);
    let mut lines = request.lines();
    let first = lines.next().ok_or_else(|| anyhow!("empty proxy request"))?;
    let path = first.split_whitespace().nth(1).unwrap_or("/");
    let range = lines.find_map(|line| {
        line.strip_prefix("Range:")
            .or_else(|| line.strip_prefix("range:"))
            .map(str::trim)
    });
    let video_index = state.video_index.load(Ordering::Relaxed);
    let candidate = if path.starts_with("/video") {
        let index = video_index;
        state.video.get(index)
    } else if path.starts_with("/audio") {
        state.audio.first()
    } else {
        None
    };
    let Some(candidate) = candidate else {
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    if path.starts_with("/video")
        && let Some((start, requested_end)) = range.and_then(parse_range)
    {
        let prefixes = state.prefixes.lock().await;
        if let Some(prefix) = prefixes.get(&video_index) {
            let end = requested_end.unwrap_or_else(|| prefix.bytes.len().saturating_sub(1));
            if end < prefix.bytes.len() && start <= end {
                let body = &prefix.bytes[start..=end];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nConnection: close\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len(),
                    prefix.total
                );
                socket.write_all(head.as_bytes()).await?;
                socket.write_all(body).await?;
                return Ok(());
            }
        }
    }
    let mut upstream = state
        .client
        .get(&candidate.url)
        .header(REFERER, "https://www.bilibili.com/")
        .header(USER_AGENT, UA);
    if let Some(range) = range {
        upstream = upstream.header(RANGE, range)
    }
    let mut response = upstream
        .send()
        .await
        .context("CDN proxy upstream request")?;
    let status = response.status();
    let reason = status.canonical_reason().unwrap_or("OK");
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nConnection: close\r\n",
        status.as_u16(),
        reason
    );
    for name in [CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE] {
        if let Some(value) = response
            .headers()
            .get(&name)
            .and_then(|value| value.to_str().ok())
        {
            head.push_str(name.as_str());
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
    }
    head.push_str("Accept-Ranges: bytes\r\n\r\n");
    socket.write_all(head.as_bytes()).await?;
    while let Some(chunk) = response.chunk().await? {
        socket.write_all(&chunk).await?;
    }
    Ok(())
}
