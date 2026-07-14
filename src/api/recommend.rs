//! Video recommendation API types

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeFeed {
    /// The personalized feed returned by Bilibili's homepage recommendation API.
    Recommended,
    /// Bilibili's public popular feed.
    Popular,
    Weekly,
    Ranking,
    MustWatch,
}

impl HomeFeed {
    pub const ALL: [Self; 5] = [
        Self::Recommended,
        Self::Popular,
        Self::Weekly,
        Self::Ranking,
        Self::MustWatch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Recommended => "首页推荐",
            Self::Popular => "综合热门",
            Self::Weekly => "每周必看",
            Self::Ranking => "排行榜",
            Self::MustWatch => "入站必刷",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RecommendData {
    pub item: Vec<VideoItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoItem {
    #[serde(alias = "aid")]
    pub id: i64,
    #[serde(default)]
    pub bvid: Option<String>,
    #[serde(default)]
    pub cid: Option<i64>,
    #[serde(default)]
    pub goto: String,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub pic: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(default)]
    pub pubdate: Option<i64>,
    #[serde(default)]
    pub owner: Option<VideoOwner>,
    #[serde(default)]
    pub stat: Option<VideoStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoOwner {
    pub mid: i64,
    pub name: String,
    pub face: Option<String>,
    #[serde(default)]
    pub follower: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoStat {
    pub view: Option<i64>,
    pub like: Option<i64>,
    pub danmaku: Option<i64>,
    #[serde(default)]
    pub reply: Option<i64>,
}

impl VideoItem {
    /// Format duration as mm:ss
    pub fn format_duration(&self) -> String {
        if let Some(duration) = self.duration {
            let minutes = duration / 60;
            let seconds = duration % 60;
            format!("{:02}:{:02}", minutes, seconds)
        } else {
            "--:--".to_string()
        }
    }

    /// Format view count (e.g., 1.2万)
    pub fn format_views(&self) -> String {
        if let Some(stat) = &self.stat {
            if let Some(view) = stat.view {
                if view >= 10000 {
                    format!("{:.1}万", view as f64 / 10000.0)
                } else {
                    view.to_string()
                }
            } else {
                "-".to_string()
            }
        } else {
            "-".to_string()
        }
    }

    /// Get author name
    pub fn author_name(&self) -> &str {
        self.owner.as_ref().map(|o| o.name.as_str()).unwrap_or("-")
    }
}
