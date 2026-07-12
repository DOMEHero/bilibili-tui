//! Dynamic feed API types and functions

use serde::{Deserialize, Deserializer};

fn deserialize_optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("integer is out of range"))
            .map(Some),
        Some(serde_json::Value::String(value)) => value
            .parse::<i64>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected integer or numeric string, got {other}"
        ))),
    }
}

fn deserialize_optional_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_optional_i64(deserializer)?
        .map(|value| i32::try_from(value).map_err(serde::de::Error::custom))
        .transpose()
}

fn deserialize_optional_f32<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(number)) => number
            .as_f64()
            .map(|value| Some(value as f32))
            .ok_or_else(|| serde::de::Error::custom("invalid floating-point number")),
        Some(serde_json::Value::String(value)) => value
            .parse::<f32>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected number or numeric string, got {other}"
        ))),
    }
}

/// Dynamic feed response
#[derive(Debug, Deserialize)]
pub struct DynamicFeedData {
    pub items: Option<Vec<DynamicItem>>,
    pub offset: Option<String>,
    pub has_more: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub update_num: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::DynamicFeedData;
    use serde::Deserialize;

    #[test]
    fn feed_accepts_string_update_num() {
        let data: DynamicFeedData =
            serde_json::from_str(r#"{"items":[],"offset":"1","has_more":true,"update_num":"0"}"#)
                .expect("string update_num should deserialize");

        assert_eq!(data.update_num, Some(0));
    }

    #[test]
    fn saved_real_feed_response_deserializes_when_fixture_is_available() {
        let Ok(path) = std::env::var("BILIBILI_DYNAMIC_FIXTURE") else {
            return;
        };
        #[derive(Deserialize)]
        struct Response {
            data: DynamicFeedData,
        }

        let body = std::fs::read(path).expect("read dynamic fixture");
        let response: Response =
            serde_json::from_slice(&body).expect("deserialize dynamic fixture");
        assert!(response.data.items.is_some());
    }
}

/// Portal data response (frequently watched UPs)
#[derive(Debug, Deserialize)]
pub struct PortalData {
    pub up_list: Option<Vec<UpListItem>>,
    // my_info and live_users are available but not needed for now
}

/// UP master in portal up_list
#[derive(Debug, Clone, Deserialize)]
pub struct UpListItem {
    pub mid: i64,
    pub uname: String,
    pub face: String,
    pub has_update: bool,
    #[serde(default)]
    pub is_reserve_recall: bool,
}

/// Individual dynamic item
#[derive(Debug, Clone, Deserialize)]
pub struct DynamicItem {
    pub id_str: Option<String>,
    #[serde(rename = "type")]
    pub dynamic_type: Option<String>,
    pub modules: Option<DynamicModules>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DynamicModules {
    pub module_author: Option<ModuleAuthor>,
    pub module_dynamic: Option<ModuleDynamic>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleAuthor {
    pub name: Option<String>,
    pub face: Option<String>,
    pub mid: Option<i64>,
    pub pub_time: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub pub_ts: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleDynamic {
    pub major: Option<DynamicMajor>,
    pub desc: Option<DynamicDesc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DynamicMajor {
    #[serde(rename = "type")]
    pub major_type: Option<String>,
    pub archive: Option<ArchiveInfo>,
    pub draw: Option<DrawInfo>,
    pub opus: Option<OpusInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchiveInfo {
    pub bvid: Option<String>,
    pub title: Option<String>,
    pub cover: Option<String>,
    pub desc: Option<String>,
    pub duration_text: Option<String>,
    pub stat: Option<ArchiveStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchiveStat {
    pub play: Option<String>,
    pub danmaku: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DynamicDesc {
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DrawInfo {
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    pub id: Option<i64>,
    pub items: Option<Vec<DrawItem>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DrawItem {
    pub src: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub height: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub width: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_f32")]
    pub size: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpusInfo {
    pub title: Option<String>,
    pub summary: Option<OpusSummary>,
    pub pics: Option<Vec<OpusPic>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpusSummary {
    pub text: Option<String>,
    pub rich_text_nodes: Option<Vec<RichTextNode>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpusPic {
    pub url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub height: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_i32")]
    pub width: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_optional_f32")]
    pub size: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RichTextNode {
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub node_type: Option<String>,
}

impl DynamicItem {
    pub fn is_video(&self) -> bool {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.major_type.as_ref())
            .map(|t| t == "MAJOR_TYPE_ARCHIVE")
            .unwrap_or(false)
    }

    pub fn author_name(&self) -> &str {
        self.modules
            .as_ref()
            .and_then(|m| m.module_author.as_ref())
            .and_then(|a| a.name.as_deref())
            .unwrap_or("未知")
    }

    pub fn author_face(&self) -> Option<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_author.as_ref())
            .and_then(|a| a.face.as_deref())
    }

    pub fn author_mid(&self) -> Option<i64> {
        self.modules
            .as_ref()
            .and_then(|modules| modules.module_author.as_ref())
            .and_then(|author| author.mid)
    }

    pub fn pub_time(&self) -> &str {
        self.modules
            .as_ref()
            .and_then(|m| m.module_author.as_ref())
            .and_then(|a| a.pub_time.as_deref())
            .unwrap_or("")
    }

    pub fn video_title(&self) -> Option<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.archive.as_ref())
            .and_then(|a| a.title.as_deref())
    }

    pub fn video_cover(&self) -> Option<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.archive.as_ref())
            .and_then(|a| a.cover.as_deref())
    }

    pub fn video_bvid(&self) -> Option<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.archive.as_ref())
            .and_then(|a| a.bvid.as_deref())
    }

    pub fn video_play(&self) -> &str {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.archive.as_ref())
            .and_then(|a| a.stat.as_ref())
            .and_then(|s| s.play.as_deref())
            .unwrap_or("-")
    }

    pub fn video_danmaku(&self) -> &str {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.archive.as_ref())
            .and_then(|a| a.stat.as_ref())
            .and_then(|s| s.danmaku.as_deref())
            .unwrap_or("-")
    }

    pub fn video_duration(&self) -> &str {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.archive.as_ref())
            .and_then(|a| a.duration_text.as_deref())
            .unwrap_or("")
    }

    pub fn is_draw(&self) -> bool {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.major_type.as_ref())
            .map(|t| t == "MAJOR_TYPE_DRAW")
            .unwrap_or(false)
    }

    pub fn is_opus(&self) -> bool {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.major_type.as_ref())
            .map(|t| t == "MAJOR_TYPE_OPUS")
            .unwrap_or(false)
    }

    pub fn draw_images(&self) -> Vec<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.draw.as_ref())
            .and_then(|draw| draw.items.as_ref())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.src.as_deref())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn opus_text(&self) -> Option<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.opus.as_ref())
            .and_then(|opus| opus.summary.as_ref())
            .and_then(|summary| summary.text.as_deref())
    }

    pub fn opus_images(&self) -> Vec<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.opus.as_ref())
            .and_then(|opus| opus.pics.as_ref())
            .map(|pics| pics.iter().filter_map(|pic| pic.url.as_deref()).collect())
            .unwrap_or_default()
    }

    pub fn desc_text(&self) -> Option<&str> {
        self.modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.desc.as_ref())
            .and_then(|desc| desc.text.as_deref())
    }

    /// Get the correct comment type code for this dynamic
    /// Type 11: 相簿（图片动态） - image/photo albums (MAJOR_TYPE_DRAW, MAJOR_TYPE_OPUS)
    /// Type 17: 动态（纯文字动态&分享） - text dynamics and shares
    /// Type 1: 视频 - videos (MAJOR_TYPE_ARCHIVE)
    pub fn comment_type(&self) -> i32 {
        if let Some(major_type) = self
            .modules
            .as_ref()
            .and_then(|m| m.module_dynamic.as_ref())
            .and_then(|d| d.major.as_ref())
            .and_then(|m| m.major_type.as_ref())
        {
            match major_type.as_str() {
                "MAJOR_TYPE_DRAW" | "MAJOR_TYPE_OPUS" => 11, // 相簿（图片动态）
                "MAJOR_TYPE_ARCHIVE" => 1,                   // 视频
                _ => 17,                                     // 其他类型的动态（纯文字、转发等）
            }
        } else {
            17 // 默认使用类型17
        }
    }

    /// Get the correct oid for comments
    /// For image dynamics (MAJOR_TYPE_DRAW), use the draw.id
    /// For other types, use the dynamic id itself
    pub fn comment_oid(&self, dynamic_id: &str) -> Option<i64> {
        if self.is_draw() {
            // For draw type, use the draw.id as oid
            self.modules
                .as_ref()
                .and_then(|m| m.module_dynamic.as_ref())
                .and_then(|d| d.major.as_ref())
                .and_then(|m| m.draw.as_ref())
                .and_then(|draw| draw.id)
        } else {
            // For other types, use the dynamic_id
            dynamic_id.parse::<i64>().ok()
        }
    }
}

/// Following users response
#[derive(Debug, Deserialize)]
pub struct FollowingsData {
    pub list: Option<Vec<FollowingUser>>,
    pub total: Option<i32>,
}

/// Following user info
#[derive(Debug, Clone, Deserialize)]
pub struct FollowingUser {
    pub mid: Option<i64>,
    pub uname: Option<String>,
    pub face: Option<String>,
    pub sign: Option<String>,
}
