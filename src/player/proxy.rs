use crate::api::cdn::{CdnCandidate, RankedStreams, record_cdn_result};
use anyhow::{Result, anyhow};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE, REFERER, USER_AGENT};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock, oneshot};
use tokio_util::sync::CancellationToken;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36";

struct ProxyState {
    client: reqwest::Client,
    video: Vec<CdnCandidate>,
    audio: Vec<CdnCandidate>,
    video_index: AtomicUsize,
    audio_index: AtomicUsize,
    prefixes: Mutex<HashMap<usize, CachedPrefix>>,
    audio_cache: RwLock<Option<CachedPrefix>>,
    cancellation: CancellationToken,
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
                .read_timeout(std::time::Duration::from_secs(15))
                .build()?,
            video: streams.video,
            audio: streams.audio,
            video_index: AtomicUsize::new(0),
            audio_index: AtomicUsize::new(0),
            prefixes: Mutex::new(HashMap::new()),
            audio_cache: RwLock::new(None),
            cancellation: CancellationToken::new(),
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
                        tokio::spawn(async move {
                            tokio::select! {
                                _ = state.cancellation.cancelled() => {}
                                result = serve(socket, state.clone()) => { let _ = result; }
                            }
                        });
                    }
                }
            }
        });
        prefetch_backup(state.clone());
        prefetch_audio(state.clone());
        Ok(Self {
            state,
            video_url: format!("http://{address}/video?generation=0"),
            audio_url: format!("http://{address}/audio"),
            shutdown: Some(shutdown_tx),
        })
    }

    pub fn next_video_cdn(&self) -> Option<(usize, String)> {
        let next = self.state.video_index.load(Ordering::Acquire) + 1;
        self.video_cdn(next)
    }

    pub fn video_cdn(&self, index: usize) -> Option<(usize, String)> {
        (index < self.state.video.len()).then(|| {
            let base = self.video_url.split('?').next().unwrap_or(&self.video_url);
            (index, format!("{base}?generation={index}"))
        })
    }

    pub fn commit_video_cdn(&mut self, next: usize) -> bool {
        let current = self.state.video_index.load(Ordering::Acquire);
        if next <= current || next >= self.state.video.len() {
            return false;
        }
        self.state.video_index.store(next, Ordering::Release);
        self.video_url = format!(
            "{}?generation={next}",
            self.video_url.split('?').next().unwrap()
        );
        prefetch_backup(self.state.clone());
        true
    }

    pub fn record_current_corruption(&self) {
        let index = self.state.video_index.load(Ordering::Acquire);
        if let Some(candidate) = self.state.video.get(index) {
            record_cdn_result(&candidate.host, true);
        }
    }

    #[cfg(test)]
    pub fn switch_video_cdn_for_test(&mut self) -> Option<String> {
        let (next, url) = self.next_video_cdn()?;
        self.state.video_index.store(next, Ordering::Release);
        self.video_url = url.clone();
        Some(url)
    }

    pub fn record_success(&self) {
        let index = self.state.video_index.load(Ordering::Relaxed);
        if let Some(candidate) = self.state.video.get(index) {
            record_cdn_result(&candidate.host, false);
        }
    }
}

fn prefetch_audio(state: Arc<ProxyState>) {
    tokio::spawn(async move {
        let download = async {
            let start = state.audio_index.load(Ordering::Acquire);
            for index in start..state.audio.len() {
                let candidate = &state.audio[index];
                let Ok(mut response) = state
                    .client
                    .get(&candidate.url)
                    .header(RANGE, "bytes=0-")
                    .header(REFERER, "https://www.bilibili.com/")
                    .header(USER_AGENT, UA)
                    .send()
                    .await
                else {
                    continue;
                };
                let Some(total) = cacheable_audio_length(&response) else {
                    continue;
                };
                let mut bytes = Vec::with_capacity(total as usize);
                while let Ok(Some(chunk)) = response.chunk().await {
                    if bytes.len().saturating_add(chunk.len()) > total as usize {
                        bytes.clear();
                        break;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                if bytes.len() as u64 == total {
                    state.audio_index.store(index, Ordering::Release);
                    return Some(CachedPrefix { bytes, total });
                }
            }
            None
        };
        tokio::select! {
            _ = state.cancellation.cancelled() => {}
            cached = download => {
                if let Some(cached) = cached {
                    *state.audio_cache.write().await = Some(cached);
                }
            }
        }
    });
}

fn cacheable_audio_length(response: &reqwest::Response) -> Option<u64> {
    cacheable_audio_length_parts(
        response.status(),
        response.headers(),
        response.content_length(),
    )
}

fn cacheable_audio_length_parts(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    content_length: Option<u64>,
) -> Option<u64> {
    const MAX_AUDIO_CACHE: u64 = 256 * 1024 * 1024;
    let total = match status {
        reqwest::StatusCode::PARTIAL_CONTENT => headers
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range)
            .map(|(_, _, total)| total),
        reqwest::StatusCode::OK => content_length,
        _ => None,
    }?;
    (total > 0 && total <= MAX_AUDIO_CACHE).then_some(total)
}

impl Drop for MediaProxy {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.state.cancellation.cancel();
    }
}

fn prefetch_backup(state: Arc<ProxyState>) {
    let next = state.video_index.load(Ordering::Relaxed) + 1;
    let Some(candidate) = state.video.get(next).cloned() else {
        return;
    };
    tokio::spawn(async move {
        let prefetch = async {
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
            if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return;
            }
            let Some((range_start, range_end, total)) = response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_range)
            else {
                return;
            };
            if range_start != 0 || range_end < range_start || total <= range_end {
                return;
            }
            let mut bytes = Vec::with_capacity(1024 * 1024);
            while let Ok(Some(chunk)) = response.chunk().await {
                let remaining = 1024 * 1024usize - bytes.len();
                bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
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
        };
        tokio::select! {
            _ = state.cancellation.cancelled() => {}
            _ = prefetch => {}
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

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?, total.parse().ok()?))
}

fn generation(path: &str) -> Option<usize> {
    path.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "generation").then(|| value.parse().ok()).flatten()
    })
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
    if !request.windows(4).any(|value| value == b"\r\n\r\n") {
        socket
            .write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    }
    let request = String::from_utf8_lossy(&request);
    let mut lines = request.lines();
    let first = lines.next().ok_or_else(|| anyhow!("empty proxy request"))?;
    let mut request_parts = first.split_whitespace();
    let method = request_parts.next().unwrap_or("");
    let path = request_parts.next().unwrap_or("/");
    let route = path.split('?').next().unwrap_or(path);
    if !matches!(method, "GET" | "HEAD") {
        socket
            .write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    }
    let range = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("range").then(|| value.trim())
    });
    let video_index = generation(path).unwrap_or_else(|| state.video_index.load(Ordering::Acquire));
    let candidate = if route == "/video" {
        let index = video_index;
        state.video.get(index).cloned()
    } else if route == "/audio" {
        state
            .audio
            .get(state.audio_index.load(Ordering::Acquire))
            .cloned()
    } else {
        None
    };
    let Some(candidate) = candidate else {
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    if route == "/audio"
        && let Some((start, requested_end)) = range.and_then(parse_range)
    {
        let cache = state.audio_cache.read().await;
        if let Some(cache) = cache.as_ref() {
            let end = requested_end.unwrap_or_else(|| cache.bytes.len().saturating_sub(1));
            if end < cache.bytes.len() && start <= end {
                let body = &cache.bytes[start..=end];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nConnection: close\r\nContent-Type: audio/mp4\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{end}/{}\r\nAccept-Ranges: bytes\r\n\r\n",
                    body.len(),
                    cache.total
                );
                socket.write_all(head.as_bytes()).await?;
                if method == "GET" {
                    socket.write_all(body).await?;
                }
                return Ok(());
            }
        }
    }
    if route == "/video"
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
                if method == "GET" {
                    socket.write_all(body).await?;
                }
                return Ok(());
            }
        }
    }
    let send = |candidate: &CdnCandidate| {
        let mut request = state
            .client
            .get(&candidate.url)
            .header(REFERER, "https://www.bilibili.com/")
            .header(USER_AGENT, UA);
        if let Some(range) = range {
            request = request.header(RANGE, range);
        }
        request.send()
    };
    let mut response = match send(&candidate).await {
        Ok(response) if is_media_response(response.status()) => response,
        first => {
            let mut recovered = None;
            if route == "/audio" {
                let start = state.audio_index.load(Ordering::Acquire) + 1;
                for index in start..state.audio.len() {
                    if let Ok(response) = send(&state.audio[index]).await
                        && is_media_response(response.status())
                    {
                        state.audio_index.store(index, Ordering::Release);
                        prefetch_audio(state.clone());
                        recovered = Some(response);
                        break;
                    }
                }
            }
            recovered.ok_or_else(|| match first {
                Err(error) => anyhow!(error).context("CDN proxy upstream request"),
                Ok(response) => anyhow!("CDN proxy upstream HTTP {}", response.status()),
            })?
        }
    };
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
    if method == "GET" {
        while let Some(chunk) = response.chunk().await? {
            socket.write_all(&chunk).await?;
        }
    }
    Ok(())
}

fn is_media_response(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::OK | reqwest::StatusCode::PARTIAL_CONTENT
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_valid_byte_ranges() {
        assert_eq!(parse_range("bytes=10-20"), Some((10, Some(20))));
        assert_eq!(parse_range("bytes=10-"), Some((10, None)));
        assert_eq!(parse_range("items=10-20"), None);
        assert_eq!(parse_range("bytes=-20"), None);
    }

    #[test]
    fn validates_content_range_and_generation() {
        assert_eq!(parse_content_range("bytes 0-99/1000"), Some((0, 99, 1000)));
        assert_eq!(parse_content_range("bytes */1000"), None);
        assert_eq!(generation("/video?generation=3"), Some(3));
        assert_eq!(generation("/video?x=1&generation=4"), Some(4));
    }

    #[test]
    fn accepts_bounded_audio_when_origin_ignores_range() {
        let headers = reqwest::header::HeaderMap::new();
        assert_eq!(
            cacheable_audio_length_parts(reqwest::StatusCode::OK, &headers, Some(1024)),
            Some(1024)
        );
        assert_eq!(
            cacheable_audio_length_parts(
                reqwest::StatusCode::OK,
                &headers,
                Some(257 * 1024 * 1024)
            ),
            None
        );
        assert!(!is_media_response(reqwest::StatusCode::FORBIDDEN));
    }
}
