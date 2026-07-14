//! UP space profile and submission API models.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceInfo {
    pub mid: i64,
    pub name: String,
    pub face: Option<String>,
    pub sign: Option<String>,
    pub level: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelationStat {
    pub mid: i64,
    pub following: Option<i64>,
    pub follower: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceVideoOrder {
    Latest,
    Popular,
}

impl SpaceVideoOrder {
    pub fn api_value(self) -> &'static str {
        match self {
            Self::Latest => "pubdate",
            Self::Popular => "click",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoData {
    pub list: SpaceVideoList,
    pub page: SpaceVideoPage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoList {
    #[serde(default)]
    pub vlist: Vec<SpaceVideoItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoPage {
    pub count: i64,
    pub pn: Option<i32>,
    pub ps: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpaceVideoItem {
    pub aid: i64,
    pub bvid: String,
    pub title: String,
    pub pic: Option<String>,
    pub duration: Option<i64>,
    pub play: Option<i64>,
    pub video_review: Option<i64>,
    pub created: Option<i64>,
    pub mid: Option<i64>,
    pub author: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::SpaceVideoOrder;

    #[test]
    fn space_sort_matches_web_query_values() {
        assert_eq!(SpaceVideoOrder::Latest.api_value(), "pubdate");
        assert_eq!(SpaceVideoOrder::Popular.api_value(), "click");
    }
}
