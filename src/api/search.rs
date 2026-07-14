//! Search API types and functions

use serde::Deserialize;

/// Search result for video type
#[derive(Debug, Deserialize)]
pub struct SearchData {
    pub result: Option<Vec<SearchVideoItem>>,
    #[serde(rename = "numResults")]
    pub num_results: Option<i32>,
    pub page: Option<i32>,
    pub pagesize: Option<i32>,
}

/// Individual video search result
#[derive(Debug, Clone, Deserialize)]
pub struct SearchVideoItem {
    pub aid: Option<i64>,
    pub bvid: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub pic: Option<String>,
    pub play: Option<i64>,
    pub duration: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "danmaku")]
    pub danmaku: Option<i64>,
    pub mid: Option<i64>,
}

impl SearchVideoItem {
    pub fn display_title(&self) -> String {
        // Remove HTML tags like <em class="keyword">
        self.title
            .as_deref()
            .unwrap_or("无标题")
            .replace("<em class=\"keyword\">", "")
            .replace("</em>", "")
    }

    pub fn author_name(&self) -> &str {
        self.author.as_deref().unwrap_or("未知")
    }

    pub fn format_play(&self) -> String {
        match self.play {
            Some(n) if n >= 10000 => format!("{:.1}万", n as f64 / 10000.0),
            Some(n) => format!("{}", n),
            None => "-".to_string(),
        }
    }

    pub fn cover_url(&self) -> Option<String> {
        self.pic.as_ref().map(|url| {
            if url.starts_with("//") {
                format!("https:{}", url)
            } else {
                url.clone()
            }
        })
    }
}

/// Hot search item from web endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct HotwordItem {
    pub keyword: Option<String>,
    pub show_name: Option<String>,
    pub icon: Option<String>,
    pub pos: Option<i32>,
    pub word_type: Option<i32>,
}

impl HotwordItem {
    /// Display name prefers show_name over keyword
    pub fn display_text(&self) -> String {
        self.show_name
            .as_ref()
            .or(self.keyword.as_ref())
            .cloned()
            .unwrap_or_else(|| "-".to_string())
    }

    /// Keyword to trigger search
    pub fn keyword_text(&self) -> Option<String> {
        self.keyword
            .clone()
            .or_else(|| self.show_name.clone())
            .filter(|s| !s.is_empty())
    }

    /// Optional badge based on word_type
    pub fn badge(&self) -> Option<&'static str> {
        match self.word_type.unwrap_or_default() {
            4 => Some("新"),
            5 => Some("热"),
            7 => Some("直播"),
            9 => Some("梗"),
            11 => Some("话题"),
            12 => Some("独家"),
            _ => None,
        }
    }
}

/// Response for hot search list (web)
#[derive(Debug, Deserialize)]
pub struct HotwordResponse {
    pub code: Option<i32>,
    pub message: Option<String>,
    pub list: Option<Vec<HotwordItem>>, // Top 10 hot words
}
