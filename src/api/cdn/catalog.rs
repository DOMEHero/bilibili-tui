use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const REFRESH_SECS: i64 = 24 * 60 * 60;
const SOURCES: [&str; 2] = [
    "https://gist.githubusercontent.com/maguowei/20a39ed03b43b87704c47c517064e3d7/raw/bilibili.snd.sgmodule",
    "https://raw.githubusercontent.com/BiliUniverse/Redirect/main/src/function/database.mjs",
];
const DOMESTIC: [&str; 10] = [
    "upos-sz-mirrorali.bilivideo.com",
    "upos-sz-mirrorali02.bilivideo.com",
    "upos-sz-mirrorbos.bilivideo.com",
    "upos-sz-mirrorcos.bilivideo.com",
    "upos-sz-mirrorhw.bilivideo.com",
    "upos-sz-mirrorks3.bilivideo.com",
    "upos-sz-mirrorkodo.bilivideo.com",
    "upos-sz-mirrorwcs.bilivideo.com",
    "upos-sz-mirrorxycdn.bilivideo.com",
    "upos-sz-upcdntx.bilivideo.com",
];
const OVERSEAS: [&str; 3] = [
    "upos-hz-mirrorakam.akamaized.net",
    "upos-sz-mirrorasiaov.bilibilivideo.com",
    "upos-sz-mirrorcosov.bilivideo.com",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Region {
    MainlandChina,
    Overseas,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct CatalogCache {
    updated_at: i64,
    region_updated_at: i64,
    region: Option<Region>,
    hosts: Vec<String>,
}

fn cache_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("bilibili-tui");
    path.push("cdn-catalog.json");
    Some(path)
}

fn load_cache() -> CatalogCache {
    cache_path()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_cache(cache: &CatalogCache) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(cache) {
        let temporary = path.with_extension("json.tmp");
        if fs::write(&temporary, bytes).is_ok() {
            let _ = fs::rename(temporary, path);
        }
    }
}

fn extract_hosts(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| {
        !(character.is_ascii_alphanumeric() || character == '.' || character == '-')
    })
    .filter(|token| {
        (token.starts_with("upos-") || token.starts_with("proxy-"))
            && (token.ends_with("bilivideo.com")
                || token.ends_with("bilibilivideo.com")
                || token.ends_with("akamaized.net"))
    })
    .map(str::to_ascii_lowercase)
}

pub(super) fn is_overseas_host(host: &str) -> bool {
    host.contains("akamaized") || host.contains("asiaov") || host.contains("cosov")
}

async fn detect_region(client: &Client) -> Option<Region> {
    let value: serde_json::Value = client
        .get("https://api.bilibili.com/x/web-interface/zone?jsonp=jsonp")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let country_code = value.pointer("/data/country_code")?.as_i64()?;
    Some(if country_code == 86 {
        Region::MainlandChina
    } else {
        Region::Overseas
    })
}

pub(super) async fn regional_hosts(client: &Client) -> (Region, Vec<String>) {
    let now = chrono::Utc::now().timestamp();
    let mut cache = load_cache();
    if now - cache.updated_at >= REFRESH_SECS || cache.hosts.is_empty() {
        let mut hosts = BTreeSet::new();
        hosts.extend(DOMESTIC.into_iter().map(str::to_string));
        hosts.extend(OVERSEAS.into_iter().map(str::to_string));
        for source in SOURCES {
            if let Ok(Ok(response)) =
                tokio::time::timeout(Duration::from_secs(5), client.get(source).send()).await
                && let Ok(response) = response.error_for_status()
                && let Ok(text) = response.text().await
            {
                hosts.extend(extract_hosts(&text));
            }
        }
        cache.hosts = hosts.into_iter().collect();
        cache.updated_at = now;
    }
    if (now - cache.region_updated_at >= REFRESH_SECS || cache.region.is_none())
        && let Some(region) = detect_region(client).await
    {
        cache.region = Some(region);
        cache.region_updated_at = now;
    }
    let region = cache.region.unwrap_or(Region::Overseas);
    save_cache(&cache);
    let hosts = cache
        .hosts
        .into_iter()
        .filter(|host| match region {
            Region::MainlandChina => !is_overseas_host(host),
            Region::Overseas => is_overseas_host(host),
        })
        .collect();
    (region, hosts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_classifies_cdn_hosts() {
        let text =
            "Host.A=upos-sz-mirrorali.bilivideo.com, https://upos-hz-mirrorakam.akamaized.net/path";
        let hosts = extract_hosts(text).collect::<Vec<_>>();
        assert_eq!(hosts.len(), 2);
        assert!(!is_overseas_host(&hosts[0]));
        assert!(is_overseas_host(&hosts[1]));
    }
}
