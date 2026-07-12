//! Bilibili Live Streaming API types and utilities

use serde::Deserialize;

/// Live room recommendation item from getMoreRecList API
#[derive(Debug, Clone, Deserialize)]
pub struct LiveRoom {
    pub roomid: i64,
    pub uid: i64,
    pub title: String,
    pub uname: String,
    pub face: String,
    pub cover: String,
    #[serde(default)]
    pub keyframe: String,
    pub online: i64,
    #[serde(default)]
    pub area_v2_name: String,
    #[serde(default)]
    pub area_v2_parent_name: String,
    #[serde(default)]
    pub watched_show: Option<WatchedShow>,
}

/// Watched count display info
#[derive(Debug, Clone, Deserialize)]
pub struct WatchedShow {
    pub num: i64,
    #[serde(default)]
    pub text_small: String,
}

/// Live recommendations response data
#[derive(Debug, Deserialize)]
pub struct LiveRecommendData {
    #[serde(default)]
    pub recommend_room_list: Vec<LiveRoom>,
}

#[derive(Debug, Deserialize)]
pub struct LiveHomeData {
    #[serde(default)]
    pub room_list: Vec<LiveHomeModule>,
}

#[derive(Debug, Deserialize)]
pub struct LiveHomeModule {
    pub module_info: LiveModuleInfo,
    #[serde(default)]
    pub list: Vec<LiveRoom>,
}

#[derive(Debug, Deserialize)]
pub struct LiveModuleInfo {
    pub title: String,
    #[serde(rename = "type")]
    pub module_type: i32,
}

impl LiveHomeData {
    pub fn followed_then_recommended(self) -> Vec<LiveRoom> {
        let mut followed = Vec::new();
        let mut recommended = Vec::new();
        for module in self.room_list {
            if module.module_info.module_type == 8 || module.module_info.title == "我的关注" {
                followed.extend(module.list);
            } else if module.module_info.module_type == 6 || module.module_info.title == "推荐直播"
            {
                recommended.extend(module.list);
            }
        }
        let mut seen = std::collections::HashSet::new();
        followed.extend(recommended);
        followed.retain(|room| seen.insert(room.roomid));
        followed
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LivePlayInfoData {
    pub live_status: i32,
    pub playurl_info: Option<LivePlayUrlInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LivePlayUrlInfo {
    pub playurl: LivePlayUrl,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LivePlayUrl {
    #[serde(default)]
    pub stream: Vec<LiveStream>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveStream {
    pub protocol_name: String,
    #[serde(default)]
    pub format: Vec<LiveFormat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveFormat {
    pub format_name: String,
    #[serde(default)]
    pub codec: Vec<LiveCodec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveCodec {
    pub codec_name: String,
    pub current_qn: i64,
    #[serde(default)]
    pub accept_qn: Vec<i64>,
    pub base_url: String,
    #[serde(default)]
    pub url_info: Vec<LiveUrlInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveUrlInfo {
    pub host: String,
    pub extra: String,
}

impl LivePlayInfoData {
    pub fn highest_available_quality(&self) -> Option<i64> {
        self.playurl_info
            .as_ref()?
            .playurl
            .stream
            .iter()
            .flat_map(|stream| &stream.format)
            .flat_map(|format| &format.codec)
            .flat_map(|codec| codec.accept_qn.iter().copied())
            .max()
    }

    pub fn stream_urls(&self) -> Vec<String> {
        let Some(info) = &self.playurl_info else {
            return Vec::new();
        };
        let highest = self.highest_available_quality().unwrap_or_default();
        let mut choices = info
            .playurl
            .stream
            .iter()
            .flat_map(|stream| {
                stream.format.iter().flat_map(move |format| {
                    format.codec.iter().filter_map(move |codec| {
                        (codec.current_qn == highest).then_some((
                            protocol_rank(&stream.protocol_name, &format.format_name),
                            codec_rank(&codec.codec_name),
                            codec,
                        ))
                    })
                })
            })
            .collect::<Vec<_>>();
        choices.sort_by_key(|(protocol, codec, _)| (*protocol, *codec));
        choices
            .into_iter()
            .flat_map(|(_, _, codec)| {
                codec
                    .url_info
                    .iter()
                    .map(|url| format!("{}{}{}", url.host, codec.base_url, url.extra))
            })
            .collect()
    }
}

fn protocol_rank(protocol: &str, format: &str) -> u8 {
    match (protocol, format) {
        ("http_hls", "fmp4") => 0,
        ("http_hls", "ts") => 1,
        ("http_stream", "flv") => 2,
        _ => 3,
    }
}

fn codec_rank(codec: &str) -> u8 {
    match codec {
        "avc" => 0,
        "hevc" => 1,
        "av1" => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(roomid: i64) -> LiveRoom {
        LiveRoom {
            roomid,
            uid: roomid,
            title: roomid.to_string(),
            uname: String::new(),
            face: String::new(),
            cover: String::new(),
            keyframe: String::new(),
            online: 0,
            area_v2_name: String::new(),
            area_v2_parent_name: String::new(),
            watched_show: None,
        }
    }

    #[test]
    fn followed_rooms_are_first_and_duplicates_are_removed() {
        let data = LiveHomeData {
            room_list: vec![
                LiveHomeModule {
                    module_info: LiveModuleInfo {
                        title: "推荐直播".into(),
                        module_type: 6,
                    },
                    list: vec![room(2), room(1)],
                },
                LiveHomeModule {
                    module_info: LiveModuleInfo {
                        title: "我的关注".into(),
                        module_type: 8,
                    },
                    list: vec![room(1)],
                },
            ],
        };
        assert_eq!(
            data.followed_then_recommended()
                .into_iter()
                .map(|room| room.roomid)
                .collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn live_play_info_uses_highest_quality_and_prefers_fmp4_avc() {
        let value = serde_json::json!({
            "live_status": 1,
            "playurl_info": {"playurl": {"stream": [{
                "protocol_name": "http_hls",
                "format": [{"format_name": "fmp4", "codec": [{
                    "codec_name": "avc", "current_qn": 20000,
                    "accept_qn": [10000, 20000], "base_url": "/best?",
                    "url_info": [{"host": "https://cdn.example", "extra": "token=1"}]
                }]}]
            }]}}
        });
        let data: LivePlayInfoData = serde_json::from_value(value).unwrap();
        assert_eq!(data.highest_available_quality(), Some(20000));
        assert_eq!(data.stream_urls(), ["https://cdn.example/best?token=1"]);
    }
}

/// Live room detailed info from get_info API
#[derive(Debug, Clone, Deserialize)]
pub struct LiveRoomInfo {
    pub uid: i64,
    pub room_id: i64,
    #[serde(default)]
    pub short_id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// 0=未开播, 1=直播中, 2=轮播中
    pub live_status: i32,
    #[serde(default)]
    pub area_id: i64,
    #[serde(default)]
    pub area_name: String,
    #[serde(default)]
    pub parent_area_name: String,
    /// 关注数
    #[serde(default)]
    pub attention: i64,
    /// 在线人数
    #[serde(default)]
    pub online: i64,
    #[serde(default)]
    pub user_cover: String,
    #[serde(default)]
    pub keyframe: String,
    #[serde(default)]
    pub live_time: String,
    #[serde(default)]
    pub tags: String,
}

impl LiveRoomInfo {
    /// Get display cover URL
    pub fn cover_url(&self) -> &str {
        if !self.user_cover.is_empty() {
            &self.user_cover
        } else if !self.keyframe.is_empty() {
            &self.keyframe
        } else {
            ""
        }
    }

    /// Get live status text
    pub fn status_text(&self) -> &'static str {
        match self.live_status {
            0 => "未开播",
            1 => "直播中",
            2 => "轮播中",
            _ => "未知",
        }
    }
}
