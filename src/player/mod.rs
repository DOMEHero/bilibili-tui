use crate::api::client::ApiClient;
use crate::domain::playback::{PlayOrder, PlaybackEvent, PlaylistItem};
use crate::storage::Credentials;
use anyhow::Result;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::time::{Instant, interval};

mod proxy;

/// Play a video using mpv with yt-dlp and report watch progress
/// This function spawns mpv in a background task to avoid blocking the TUI
#[allow(clippy::too_many_arguments)]
pub async fn play_video(
    api_client: Arc<ApiClient>,
    bvid: &str,
    aid: i64,
    cid: i64,
    duration: i64,
    page_num: Option<i32>,
    credentials: Option<&Credentials>,
    playback_event_tx: Sender<PlaybackEvent>,
) -> Result<()> {
    let webpage_url = match page_num {
        Some(p) if p > 1 => format!("https://www.bilibili.com/video/{}?p={}", bvid, p),
        _ => format!("https://www.bilibili.com/video/{}", bvid),
    };

    // Report watch start
    let _ = crate::api::heartbeat::report_watch_start(&api_client, aid, cid, bvid, duration).await;

    let start_ts = chrono::Utc::now().timestamp();

    let mut media_proxy = match api_client.get_play_url(bvid, cid).await {
        Ok(play_url) => match crate::api::cdn::rank_streams(&play_url).await {
            Ok(streams) => proxy::MediaProxy::start(streams).await.ok(),
            Err(_) => None,
        },
        Err(_) => None,
    };

    let mut cmd = Command::new("mpv");

    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let cookie_path_to_clean = if let Some(creds) = credentials {
        let cookie_path = crate::storage::export_cookies_for_ytdlp(creds)?;
        cmd.arg(format!(
            "--ytdl-raw-options=cookies={}",
            cookie_path.display()
        ));
        Some(cookie_path)
    } else {
        None
    };

    cmd.arg("--force-window=immediate");
    // Direct CDN URLs do not contain the BVID/CID. Keep the original page as
    // the referrer for SponsorBlock and pass CID explicitly to the danmaku
    // script so both integrations still work when yt-dlp is bypassed.
    cmd.arg(format!("--referrer={webpage_url}"));
    cmd.arg(format!("--http-header-fields=Referer: {webpage_url}"));
    cmd.arg(format!("--script-opts-append=cid={cid}"));
    let ipc_path = std::env::temp_dir().join(format!(
        "bilibili-tui-mpv-{}-{}.sock",
        std::process::id(),
        cid
    ));
    let _ = std::fs::remove_file(&ipc_path);
    cmd.arg(format!("--input-ipc-server={}", ipc_path.display()));
    cmd.arg("--msg-level=ffmpeg=error,vd=warn");
    if let Some(proxy) = &media_proxy {
        cmd.arg(format!("--audio-file={}", proxy.audio_url));
        cmd.arg(&proxy.video_url);
    } else {
        cmd.arg("--ytdl-format=bestvideo+bestaudio/best");
        cmd.arg(&webpage_url);
    }

    let mut child = cmd.spawn()?;
    let stderr = child
        .stderr
        .take()
        .map(BufReader::new)
        .map(|reader| reader.lines());

    // Clone bvid for the background task (needs 'static lifetime)
    let bvid = bvid.to_string();

    // Spawn a background task to handle heartbeat and cleanup
    // This prevents blocking the TUI
    tokio::spawn(async move {
        let start_time = Instant::now();
        let mut played_time: i64 = 0;
        let mut heartbeat_interval = interval(Duration::from_secs(15));
        let mut stderr = stderr;
        let mut decode_errors = 0usize;
        let mut last_switch = Instant::now() - Duration::from_secs(10);

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    played_time += 15;
                    let real_played_time = start_time.elapsed().as_secs() as i64;

                    let _ = crate::api::heartbeat::report_heartbeat(
                        &api_client,
                        aid,
                        cid,
                        &bvid,
                        played_time,
                        real_played_time,
                        real_played_time,
                        start_ts,
                        0, // play_type: 0 = playing
                    ).await;
                }
                result = child.wait() => {
                    let real_played_time = start_time.elapsed().as_secs() as i64;

                    let _ = crate::api::heartbeat::report_heartbeat(
                        &api_client,
                        aid,
                        cid,
                        &bvid,
                        played_time,
                        real_played_time,
                        real_played_time,
                        start_ts,
                        4, // play_type: 4 = end
                    ).await;

                    if result.as_ref().is_ok_and(|status| status.success())
                        && let Some(proxy) = &media_proxy
                    {
                        proxy.record_success();
                    }
                    break;
                }
                line = async {
                    match &mut stderr {
                        Some(lines) => lines.next_line().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let Ok(Some(line)) = line else { stderr = None; continue };
                    if is_corrupt_video_log(&line) {
                        decode_errors += 1;
                    }
                    if decode_errors >= 3 && last_switch.elapsed() > Duration::from_secs(5) {
                        decode_errors = 0;
                        if let Some(proxy) = &mut media_proxy
                            && let Some(video_url) = proxy.switch_video_cdn()
                        {
                            let position = mpv_time_pos(&ipc_path).await.unwrap_or(0.0);
                            let _ = replace_mpv_stream(
                                &ipc_path,
                                &video_url,
                                &proxy.audio_url,
                                position,
                            ).await;
                            last_switch = Instant::now();
                        }
                    }
                }
            }
        }

        // Cleanup cookie file
        if let Some(path) = cookie_path_to_clean {
            let _ = tokio::fs::remove_file(path).await;
        }
        let _ = tokio::fs::remove_file(ipc_path).await;
        let _ = playback_event_tx.send(PlaybackEvent::Finished { bvid });
    });

    Ok(())
}

fn is_corrupt_video_log(line: &str) -> bool {
    [
        "Invalid NAL unit size",
        "Error splitting the input into NAL units",
        "hardware accelerator failed to decode picture",
        "Error while decoding frame",
    ]
    .iter()
    .any(|needle| line.contains(needle))
}

async fn mpv_ipc(path: &std::path::Path, command: serde_json::Value) -> Result<serde_json::Value> {
    let mut stream = UnixStream::connect(path).await?;
    let mut bytes = serde_json::to_vec(&serde_json::json!({ "command": command }))?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await?;
    Ok(serde_json::from_str(&line)?)
}

async fn mpv_time_pos(path: &std::path::Path) -> Option<f64> {
    mpv_ipc(path, serde_json::json!(["get_property", "time-pos"]))
        .await
        .ok()?
        .get("data")?
        .as_f64()
}

async fn replace_mpv_stream(
    path: &std::path::Path,
    video_url: &str,
    audio_url: &str,
    position: f64,
) -> Result<()> {
    mpv_ipc(path, serde_json::json!(["loadfile", video_url, "replace"])).await?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    mpv_ipc(path, serde_json::json!(["audio-add", audio_url, "select"])).await?;
    mpv_ipc(
        path,
        serde_json::json!(["seek", position, "absolute+exact"]),
    )
    .await?;
    Ok(())
}

/// Start one mpv process with multiple Bilibili URLs. mpv owns automatic
/// advancement, so window/fullscreen/volume state is preserved between items.
pub async fn play_playlist(
    items: Vec<PlaylistItem>,
    order: PlayOrder,
    start_index: usize,
    credentials: Option<&Credentials>,
) -> Result<()> {
    let (items, start_index) = ordered_playlist(items, order, start_index)?;

    let mut cmd = Command::new("mpv");
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.arg("--ytdl-format=bestvideo+bestaudio/best");
    cmd.arg("--force-window=immediate");
    cmd.arg(format!("--playlist-start={start_index}"));

    let cookie_path_to_clean = if let Some(creds) = credentials {
        let cookie_path = crate::storage::export_cookies_for_ytdlp(creds)?;
        cmd.arg(format!(
            "--ytdl-raw-options=cookies={}",
            cookie_path.display()
        ));
        Some(cookie_path)
    } else {
        None
    };

    for item in items {
        let url = match item.page {
            Some(page) if page > 1 => {
                format!("https://www.bilibili.com/video/{}?p={page}", item.bvid)
            }
            _ => format!("https://www.bilibili.com/video/{}", item.bvid),
        };
        cmd.arg(url);
    }

    let mut child = cmd.spawn()?;
    tokio::spawn(async move {
        let _ = child.wait().await;
        if let Some(path) = cookie_path_to_clean {
            let _ = tokio::fs::remove_file(path).await;
        }
    });
    Ok(())
}

fn ordered_playlist(
    mut items: Vec<PlaylistItem>,
    order: PlayOrder,
    mut start_index: usize,
) -> Result<(Vec<PlaylistItem>, usize)> {
    if items.is_empty() {
        anyhow::bail!("播放列表为空");
    }
    if start_index >= items.len() {
        anyhow::bail!("播放起点超出列表范围");
    }
    if order == PlayOrder::Reverse {
        start_index = items.len() - 1 - start_index;
        items.reverse();
    }
    Ok((items, start_index))
}

/// Play a bangumi episode using mpv with yt-dlp
/// This function spawns mpv in a background task to avoid blocking the TUI
pub async fn play_bangumi_episode(ep_id: i64, credentials: Option<&Credentials>) -> Result<()> {
    let video_url = format!("https://www.bilibili.com/bangumi/play/ep{}", ep_id);

    let mut cmd = Command::new("mpv");
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    let cookie_path_to_clean = if let Some(creds) = credentials {
        let cookie_path = crate::storage::export_cookies_for_ytdlp(creds)?;
        cmd.arg(format!(
            "--ytdl-raw-options=cookies={}",
            cookie_path.display()
        ));
        Some(cookie_path)
    } else {
        None
    };

    cmd.arg("--ytdl-format=bestvideo+bestaudio/best");
    cmd.arg("--force-window=immediate");
    cmd.arg(&video_url);

    let mut child = cmd.spawn()?;

    tokio::spawn(async move {
        let _ = child.wait().await;

        // Cleanup cookie file
        if let Some(path) = cookie_path_to_clean {
            let _ = tokio::fs::remove_file(path).await;
        }
    });

    Ok(())
}

/// Play a live stream using mpv
/// This function spawns mpv in a background task to avoid blocking the TUI
pub async fn play_live(api_client: Arc<ApiClient>, room_id: i64) -> Result<()> {
    let urls = api_client.get_best_live_stream_urls(room_id).await?;
    let first_url = urls
        .first()
        .ok_or_else(|| anyhow::anyhow!("直播播放地址为空"))?;
    let mut child = spawn_live_mpv(first_url)?;

    tokio::spawn(async move {
        let mut urls = urls;
        let mut next_url = 1usize;
        let mut retries = 0usize;
        loop {
            let status = child.wait().await;
            if status.as_ref().is_ok_and(|status| status.success()) || retries >= 3 {
                break;
            }
            retries += 1;
            if next_url >= urls.len() {
                match api_client.get_best_live_stream_urls(room_id).await {
                    Ok(refreshed) => {
                        urls = refreshed;
                        next_url = 0;
                    }
                    Err(_) => break,
                }
            }
            let Some(url) = urls.get(next_url) else {
                break;
            };
            next_url += 1;
            match spawn_live_mpv(url) {
                Ok(restarted) => child = restarted,
                Err(_) => break,
            }
        }
    });

    Ok(())
}

fn spawn_live_mpv(url: &str) -> Result<tokio::process::Child> {
    let mut cmd = Command::new("mpv");
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    configure_live_mpv(&mut cmd, url);
    Ok(cmd.spawn()?)
}

fn configure_live_mpv(cmd: &mut Command, url: &str) {
    cmd.arg("--force-window=immediate");
    cmd.arg("--referrer=https://live.bilibili.com/");
    cmd.arg("--cache=yes");
    cmd.arg("--cache-secs=20");
    cmd.arg("--demuxer-readahead-secs=20");
    cmd.arg("--network-timeout=10");
    cmd.arg("--stream-lavf-o=reconnect=1,reconnect_streamed=1,reconnect_delay_max=5");
    cmd.arg("--hwdec=auto-safe");
    cmd.arg(url);
}

#[cfg(test)]
mod playlist_tests {
    use super::*;

    fn item(id: i64) -> PlaylistItem {
        PlaylistItem {
            bvid: format!("BV{id}"),
            aid: id,
            cid: None,
            title: id.to_string(),
            uploader_mid: None,
            duration: None,
            page: None,
        }
    }

    #[test]
    fn reverse_play_all_starts_at_first_reversed_item() {
        let items = vec![item(1), item(2), item(3)];
        let (items, start) = ordered_playlist(items, PlayOrder::Reverse, 2).unwrap();
        assert_eq!(
            items.iter().map(|item| item.aid).collect::<Vec<_>>(),
            [3, 2, 1]
        );
        assert_eq!(start, 0);
    }

    #[test]
    fn forward_play_all_preserves_web_order() {
        let items = vec![item(1), item(2), item(3)];
        let (items, start) = ordered_playlist(items, PlayOrder::Forward, 0).unwrap();
        assert_eq!(
            items.iter().map(|item| item.aid).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(start, 0);
    }

    #[test]
    fn recognizes_corrupt_h264_diagnostics() {
        assert!(is_corrupt_video_log(
            "h264: Invalid NAL unit size (123 > 10)"
        ));
        assert!(is_corrupt_video_log(
            "h264: Error splitting the input into NAL units."
        ));
        assert!(!is_corrupt_video_log("AO: [coreaudio] 48000Hz stereo"));
    }

    #[tokio::test]
    #[ignore = "requires login, network access, and mpv"]
    async fn best_live_stream_decodes_in_mpv() {
        let credentials = crate::storage::load_credentials().expect("load credentials");
        let client = Arc::new(ApiClient::with_cookies(&credentials));
        let room = client
            .get_live_home_rooms()
            .await
            .expect("live rooms")
            .into_iter()
            .next()
            .expect("an active live room");
        let url = client
            .get_best_live_stream_urls(room.roomid)
            .await
            .expect("best stream")
            .into_iter()
            .next()
            .expect("stream URL");
        let mut cmd = Command::new("mpv");
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.arg("--no-config");
        cmd.arg("--vo=null");
        cmd.arg("--ao=null");
        cmd.arg("--frames=30");
        configure_live_mpv(&mut cmd, &url);
        let mut child = cmd.spawn().expect("spawn mpv");
        let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
            .await
            .expect("mpv decode timeout")
            .expect("wait for mpv");
        assert!(status.success());
    }

    #[tokio::test]
    #[ignore = "requires network access and mpv"]
    async fn mpv_switches_proxy_cdn_without_restarting() {
        let client = ApiClient::new();
        let bvid = "BV1cP7j64E37";
        let info = client.get_video_info(bvid).await.expect("video info");
        let play_url = client.get_play_url(bvid, info.cid).await.expect("playurl");
        let streams = crate::api::cdn::rank_streams(&play_url)
            .await
            .expect("rank CDN streams");
        assert!(streams.video.len() > 1, "test needs a backup video CDN");
        let mut proxy = proxy::MediaProxy::start(streams)
            .await
            .expect("start proxy");
        let ipc = std::env::temp_dir().join(format!(
            "bilibili-tui-switch-test-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&ipc);
        let mut command = Command::new("mpv");
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .arg("--no-config")
            .arg("--vo=null")
            .arg("--ao=null")
            .arg(format!("--input-ipc-server={}", ipc.display()))
            .arg(format!("--audio-file={}", proxy.audio_url))
            .arg(&proxy.video_url);
        let mut child = command.spawn().expect("spawn mpv");
        let pid = child.id();
        for _ in 0..50 {
            if ipc.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let position = mpv_time_pos(&ipc).await.expect("read playback position");
        let backup = proxy.switch_video_cdn_for_test().expect("backup CDN");
        replace_mpv_stream(&ipc, &backup, &proxy.audio_url, position)
            .await
            .expect("replace stream over IPC");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(child.id(), pid);
        assert!(child.try_wait().expect("query mpv").is_none());
        assert!(mpv_time_pos(&ipc).await.expect("position after switch") >= position);
        child.kill().await.expect("stop test mpv");
        let _ = std::fs::remove_file(ipc);
    }
}
