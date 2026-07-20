//! Watch history API types
//!
//! API endpoint: GET https://api.bilibili.com/x/web-interface/history/cursor
//! Authentication: Cookie (SESSDATA)

use serde::Deserialize;

/// Stable identity used by Bilibili's history deletion endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HistoryKey {
    pub business: String,
    pub kid: i64,
}

impl HistoryKey {
    pub fn api_value(&self) -> String {
        format!("{}_{}", self.business, self.kid)
    }
}

/// Response data for history cursor API
#[derive(Debug, Deserialize)]
pub struct HistoryData {
    pub cursor: HistoryCursor,
    pub tab: Option<Vec<HistoryTab>>,
    pub list: Vec<HistoryItem>,
}

/// Cursor for pagination
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryCursor {
    /// Max value for next page
    pub max: i64,
    /// View timestamp for next page
    pub view_at: i64,
    /// Business type
    pub business: String,
    /// Page size
    pub ps: i32,
}

/// Tab types in history
#[derive(Debug, Deserialize)]
pub struct HistoryTab {
    #[serde(rename = "type")]
    pub tab_type: String,
    pub name: String,
}

/// Individual history item
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryItem {
    /// Title of the content
    #[serde(default)]
    pub title: String,
    /// Long title (for episodes)
    #[serde(default)]
    pub long_title: Option<String>,
    /// Cover image URL
    #[serde(default)]
    pub cover: Option<String>,
    /// Alternative covers
    #[serde(default)]
    pub covers: Option<Vec<String>>,
    /// URI for navigation
    #[serde(default)]
    pub uri: Option<String>,
    /// History metadata
    pub history: HistoryMeta,
    /// Number of videos in the content
    #[serde(default)]
    pub videos: i32,
    /// Author name
    #[serde(default)]
    pub author_name: String,
    /// Author avatar
    #[serde(default)]
    pub author_face: Option<String>,
    /// Author mid
    #[serde(default)]
    pub author_mid: i64,
    /// Last view timestamp
    #[serde(default)]
    pub view_at: i64,
    /// Watch progress in seconds
    #[serde(default)]
    pub progress: i64,
    /// Badge text (e.g., "专栏", "直播中", "国创")
    #[serde(default)]
    pub badge: Option<String>,
    /// Show title for episodes
    #[serde(default)]
    pub show_title: Option<String>,
    /// Duration in seconds
    #[serde(default)]
    pub duration: i64,
    /// Current episode info
    #[serde(default)]
    pub current: Option<String>,
    /// Total episodes
    #[serde(default)]
    pub total: i32,
    /// New episode description
    #[serde(default)]
    pub new_desc: Option<String>,
    /// Whether the series is finished
    #[serde(default)]
    pub is_finish: i32,
    /// Whether favorited
    #[serde(default)]
    pub is_fav: i32,
    /// Kid for certain types
    #[serde(default)]
    pub kid: i64,
    /// Tag name
    #[serde(default)]
    pub tag_name: Option<String>,
    /// Live status (0: not live, 1: live)
    #[serde(default)]
    pub live_status: i32,
}

/// History metadata containing IDs
#[derive(Debug, Clone, Deserialize)]
pub struct HistoryMeta {
    /// Object ID
    #[serde(default)]
    pub oid: i64,
    /// Episode ID (for pgc)
    #[serde(default)]
    pub epid: i64,
    /// BV ID (for archive)
    #[serde(default)]
    pub bvid: Option<String>,
    /// Page number
    #[serde(default)]
    pub page: i32,
    /// CID
    #[serde(default)]
    pub cid: i64,
    /// Part name
    #[serde(default)]
    pub part: Option<String>,
    /// Business type: archive, pgc, live, article, article-list
    #[serde(default)]
    pub business: String,
    /// Device type
    #[serde(default)]
    pub dt: i32,
}

impl HistoryItem {
    /// Get the best cover URL
    pub fn get_cover(&self) -> Option<&str> {
        if let Some(ref cover) = self.cover
            && !cover.is_empty()
        {
            return Some(cover.as_str());
        }
        if let Some(ref covers) = self.covers
            && let Some(first) = covers.first()
        {
            return Some(first.as_str());
        }
        None
    }

    /// Format duration as mm:ss
    pub fn format_duration(&self) -> String {
        if self.duration > 0 {
            let minutes = self.duration / 60;
            let seconds = self.duration % 60;
            format!("{:02}:{:02}", minutes, seconds)
        } else {
            "--:--".to_string()
        }
    }

    /// Format progress as mm:ss
    pub fn format_progress(&self) -> String {
        if self.progress > 0 {
            let minutes = self.progress / 60;
            let seconds = self.progress % 60;
            format!("{:02}:{:02}", minutes, seconds)
        } else {
            "00:00".to_string()
        }
    }

    /// Calculate progress percentage
    pub fn progress_percent(&self) -> f64 {
        if self.duration > 0 {
            (self.progress as f64 / self.duration as f64 * 100.0).min(100.0)
        } else {
            0.0
        }
    }

    /// Format view_at timestamp as relative time
    pub fn format_view_time(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let diff = now - self.view_at;

        if diff < 60 {
            "刚刚".to_string()
        } else if diff < 3600 {
            format!("{}分钟前", diff / 60)
        } else if diff < 86400 {
            format!("{}小时前", diff / 3600)
        } else if diff < 604800 {
            format!("{}天前", diff / 86400)
        } else {
            // Format as date
            let secs = self.view_at;
            let days_since_epoch = secs / 86400;
            let year = 1970 + (days_since_epoch / 365);
            format!("{}年", year)
        }
    }

    /// Check if this is a video (archive or pgc)
    pub fn is_video(&self) -> bool {
        matches!(self.history.business.as_str(), "archive" | "pgc")
    }

    /// Check if this is a live room history entry
    pub fn is_live(&self) -> bool {
        self.history.business == "live"
    }

    pub fn is_article(&self) -> bool {
        matches!(self.history.business.as_str(), "article" | "article-list")
    }

    pub fn is_selectable(&self) -> bool {
        self.is_video() || self.is_article()
    }

    pub fn history_key(&self) -> Option<HistoryKey> {
        (self.kid > 0 && self.is_selectable()).then(|| HistoryKey {
            business: self.history.business.clone(),
            kid: self.kid,
        })
    }

    pub fn article_id(&self) -> Option<i64> {
        match self.history.business.as_str() {
            "article" => (self.history.oid > 0).then_some(self.history.oid),
            "article-list" => (self.history.cid > 0).then_some(self.history.cid),
            _ => None,
        }
    }

    /// Get bvid if available
    pub fn get_bvid(&self) -> Option<&str> {
        self.history.bvid.as_deref().filter(|s| !s.is_empty())
    }

    /// Get live room id for live history entries
    pub fn get_live_room_id(&self) -> Option<i64> {
        if self.history.oid > 0 {
            return Some(self.history.oid);
        }

        self.uri
            .as_deref()
            .and_then(Self::parse_live_room_id_from_uri)
    }

    fn parse_live_room_id_from_uri(uri: &str) -> Option<i64> {
        let trimmed = uri.trim();
        if trimmed.is_empty() {
            return None;
        }

        let no_scheme = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .unwrap_or(trimmed);
        let host_path = no_scheme.strip_prefix("//").unwrap_or(no_scheme);

        let mut parts = host_path.splitn(2, '/');
        let host = parts.next()?.split(':').next()?.to_ascii_lowercase();
        if host != "live.bilibili.com" && host != "m.live.bilibili.com" {
            return None;
        }

        let path = parts.next().unwrap_or_default();
        let first_segment = path
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .split('/')
            .find(|seg| !seg.is_empty())?;

        first_segment.parse::<i64>().ok().filter(|id| *id > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_history_records_deserialize_and_route_ids() {
        let items: Vec<HistoryItem> = serde_json::from_value(serde_json::json!([
            {
                "title": "video",
                "kid": 11,
                "history": {"oid": 11, "bvid": "BV1test", "business": "archive"}
            },
            {
                "title": "vip episode",
                "kid": 22,
                "history": {"epid": 2201, "bvid": "", "business": "pgc"}
            },
            {
                "title": "article",
                "kid": 33,
                "covers": ["https://i.test/article.jpg"],
                "history": {"oid": 3301, "business": "article"}
            },
            {
                "title": "article list",
                "kid": 44,
                "history": {"oid": 4400, "cid": 4401, "business": "article-list"}
            }
        ]))
        .expect("mixed history fixture");

        assert_eq!(items[0].get_bvid(), Some("BV1test"));
        assert_eq!(items[1].get_bvid(), None);
        assert_eq!(items[1].history.epid, 2201);
        assert_eq!(items[2].get_cover(), Some("https://i.test/article.jpg"));
        assert_eq!(items[2].article_id(), Some(3301));
        assert_eq!(items[3].article_id(), Some(4401));
        assert_eq!(
            items[3].history_key().unwrap().api_value(),
            "article-list_44"
        );
    }
}
