use anyhow::{Result, anyhow};
use futures_util::future::join_all;
use reqwest::header::{RANGE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const PROBE_BYTES: u64 = 512 * 1024 - 1;
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36";
const SCORE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Deserialize)]
pub struct PlayUrlData {
    pub dash: Dash,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dash {
    pub video: Vec<DashStream>,
    #[serde(default)]
    pub audio: Vec<DashStream>,
    pub dolby: Option<Dolby>,
    pub flac: Option<Flac>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dolby {
    #[serde(default)]
    pub audio: Option<Vec<DashStream>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Flac {
    pub audio: Option<DashStream>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashStream {
    pub id: i64,
    #[serde(default)]
    pub bandwidth: i64,
    #[serde(default)]
    pub codecid: i64,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default, rename = "baseUrl")]
    pub base_url_camel: Option<String>,
    #[serde(default)]
    pub backup_url: Option<Vec<String>>,
    #[serde(default, rename = "backupUrl")]
    pub backup_url_camel: Option<Vec<String>>,
}

impl DashStream {
    fn primary_url(&self) -> Option<&str> {
        self.base_url.as_deref().or(self.base_url_camel.as_deref())
    }

    fn backup_urls(&self) -> impl Iterator<Item = &String> {
        self.backup_url
            .iter()
            .flatten()
            .chain(self.backup_url_camel.iter().flatten())
    }
}

#[derive(Debug, Clone)]
pub struct CdnCandidate {
    pub url: String,
    pub host: String,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct RankedStreams {
    pub video: Vec<CdnCandidate>,
    pub audio: Vec<CdnCandidate>,
}

#[derive(Clone, Copy)]
struct CachedScore {
    measured_at: Instant,
    probe: ProbeScore,
}

#[derive(Debug, Clone, Copy)]
struct ProbeScore {
    latency: Duration,
    throughput_bps: f64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct CdnHistory {
    attempts: u64,
    corruptions: u64,
}

static CDN_SCORES: OnceLock<Mutex<HashMap<String, CachedScore>>> = OnceLock::new();
static CDN_HISTORY: OnceLock<Mutex<HashMap<String, CdnHistory>>> = OnceLock::new();

fn scores() -> &'static Mutex<HashMap<String, CachedScore>> {
    CDN_SCORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn host(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()?
        .host_str()
        .map(ToOwned::to_owned)
}

fn history_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("bilibili-tui");
    path.push("cdn-history.json");
    Some(path)
}

fn history() -> &'static Mutex<HashMap<String, CdnHistory>> {
    CDN_HISTORY.get_or_init(|| {
        let values = history_path()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Mutex::new(values)
    })
}

fn save_history(values: &HashMap<String, CdnHistory>) {
    let Some(path) = history_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(values) {
        let _ = fs::write(path, bytes);
    }
}

pub fn record_cdn_result(host: &str, corrupted: bool) {
    if let Ok(mut values) = history().lock() {
        let entry = values.entry(host.to_string()).or_default();
        entry.attempts = entry.attempts.saturating_add(1);
        if corrupted {
            entry.corruptions = entry.corruptions.saturating_add(1);
        }
        save_history(&values);
    }
}

fn reliability(host: &str) -> f64 {
    let value = history()
        .lock()
        .ok()
        .and_then(|values| values.get(host).cloned())
        .unwrap_or_default();
    1.0 - (value.corruptions as f64 + 1.0) / (value.attempts as f64 + 10.0)
}

fn cached_score(url: &str) -> Option<ProbeScore> {
    let host = host(url)?;
    let cached = *scores().lock().ok()?.get(&host)?;
    (cached.measured_at.elapsed() < SCORE_TTL).then_some(cached.probe)
}

async fn probe(client: &reqwest::Client, url: String) -> (String, Option<ProbeScore>) {
    if let Some(score) = cached_score(&url) {
        return (url, Some(score));
    }

    let started = Instant::now();
    let result = tokio::time::timeout(PROBE_TIMEOUT, async {
        let mut response = client
            .get(&url)
            .header(RANGE, format!("bytes=0-{PROBE_BYTES}"))
            .header(REFERER, "https://www.bilibili.com/")
            .header(USER_AGENT, USER_AGENT_VALUE)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow!("HTTP {}", response.status()));
        }
        let mut received = 0usize;
        let mut first_byte = None;
        while let Some(chunk) = response.chunk().await? {
            first_byte.get_or_insert_with(|| started.elapsed());
            received += chunk.len();
            if received > PROBE_BYTES as usize {
                break;
            }
        }
        if received == 0 {
            return Err(anyhow!("empty CDN response"));
        }
        let elapsed = started.elapsed();
        Ok::<_, anyhow::Error>(ProbeScore {
            latency: first_byte.unwrap_or(elapsed),
            throughput_bps: received as f64 * 8.0 / elapsed.as_secs_f64(),
        })
    })
    .await;

    let score = result.ok().and_then(Result::ok);
    if let (Some(host), Some(score)) = (host(&url), score)
        && let Ok(mut cache) = scores().lock()
    {
        cache.insert(
            host,
            CachedScore {
                measured_at: Instant::now(),
                probe: score,
            },
        );
    }
    (url, score)
}

async fn rank_urls(stream: &DashStream) -> Result<Vec<CdnCandidate>> {
    let mut urls = Vec::with_capacity(
        1 + stream.backup_url.as_ref().map_or(0, Vec::len)
            + stream.backup_url_camel.as_ref().map_or(0, Vec::len),
    );
    urls.push(
        stream
            .primary_url()
            .ok_or_else(|| anyhow!("CDN 流缺少主地址"))?
            .to_string(),
    );
    urls.extend(stream.backup_urls().cloned());
    urls.sort_by_key(|url| host(url));
    urls.dedup_by(|a, b| host(a) == host(b));

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(800))
        .build()?;
    let results = join_all(urls.into_iter().map(|url| probe(&client, url))).await;
    let mut ranked = results
        .into_iter()
        .filter_map(|(url, probe)| {
            let probe = probe?;
            let host = host(&url)?;
            let latency = latency_score(probe.latency);
            let ratio = if probe.throughput_bps == 0.0 || stream.bandwidth <= 0 {
                1.0
            } else {
                probe.throughput_bps / stream.bandwidth as f64
            };
            let speed = speed_score(ratio);
            let score = reliability(&host) * 0.55 + latency * 0.35 + speed.min(1.0) * 0.10;
            Some(CdnCandidate { url, host, score })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    (!ranked.is_empty())
        .then_some(ranked)
        .ok_or_else(|| anyhow!("没有可用的 CDN 节点"))
}

fn latency_score(latency: Duration) -> f64 {
    1.0 / (1.0 + latency.as_secs_f64() / 0.2)
}

fn speed_score(ratio: f64) -> f64 {
    if ratio < 1.0 {
        ratio.max(0.0).powi(2) * 0.6
    } else if ratio <= 1.5 {
        0.6 + (ratio - 1.0) * 0.7
    } else {
        0.95 + (1.0 - (-(ratio - 1.5)).exp()) * 0.05
    }
}

pub async fn rank_streams(data: &PlayUrlData) -> Result<RankedStreams> {
    let video = data
        .dash
        .video
        .iter()
        .max_by_key(|stream| (stream.id, stream.bandwidth))
        .ok_or_else(|| anyhow!("播放地址没有视频流"))?;
    let mut audio = data.dash.audio.iter().collect::<Vec<_>>();
    if let Some(dolby) = &data.dash.dolby {
        audio.extend(dolby.audio.iter().flatten());
    }
    if let Some(flac) = &data.dash.flac
        && let Some(stream) = &flac.audio
    {
        audio.push(stream);
    }
    let audio = audio
        .into_iter()
        .max_by_key(|stream| stream.bandwidth)
        .ok_or_else(|| anyhow!("播放地址没有音频流"))?;
    let (video, audio) = tokio::join!(rank_urls(video), rank_urls(audio));
    Ok(RankedStreams {
        video: video?,
        audio: audio?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playurl_accepts_camel_and_snake_case_urls() {
        let value = serde_json::json!({
            "dash": {
                "video": [{"id": 80, "bandwidth": 1, "baseUrl": "https://a/v", "backupUrl": ["https://b/v"]}],
                "audio": [{"id": 30280, "bandwidth": 2, "base_url": "https://a/a", "backup_url": ["https://b/a"]}],
                "dolby": null,
                "flac": null
            }
        });
        let data: PlayUrlData = serde_json::from_value(value).unwrap();
        assert_eq!(
            data.dash.video[0].backup_url_camel.as_ref().map(Vec::len),
            Some(1)
        );
        assert_eq!(data.dash.audio[0].base_url.as_deref(), Some("https://a/a"));
    }

    #[test]
    fn speed_penalizes_below_bitrate_and_flattens_above_one_point_five() {
        assert!(speed_score(0.5) < speed_score(0.9));
        assert!(speed_score(0.9) < speed_score(1.0));
        assert!(speed_score(1.5) - speed_score(1.0) > speed_score(3.0) - speed_score(1.5));
        assert!(speed_score(10.0) <= 1.0);
    }

    #[test]
    fn latency_rewards_low_time_to_first_byte() {
        assert!(
            latency_score(Duration::from_millis(20)) > latency_score(Duration::from_millis(200))
        );
    }
}
