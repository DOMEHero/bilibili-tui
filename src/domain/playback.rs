//! Application-level playback queue state.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistItem {
    pub bvid: String,
    pub aid: i64,
    pub cid: Option<i64>,
    pub title: String,
    pub uploader_mid: Option<i64>,
    pub duration: Option<i64>,
    pub page: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlayOrder {
    #[default]
    Forward,
    Reverse,
    Shuffle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistSource {
    Manual,
    Uploader { mid: i64, name: String },
    Favorites { media_id: i64, title: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaybackStatus {
    #[default]
    Idle,
    Starting,
    Playing,
    Paused,
    Failed,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackEvent {
    Finished { bvid: String },
}

#[derive(Debug, Default)]
pub struct PlaybackState {
    pub queue: Vec<PlaylistItem>,
    pub current_index: Option<usize>,
    pub order: PlayOrder,
    pub source: Option<PlaylistSource>,
    pub status: PlaybackStatus,
    pub last_error: Option<String>,
}

impl PlaybackState {
    pub fn replace_queue(&mut self, source: PlaylistSource, items: Vec<PlaylistItem>) {
        self.queue = items;
        self.source = Some(source);
        self.current_index = (!self.queue.is_empty()).then_some(0);
        self.status = PlaybackStatus::Idle;
        self.last_error = None;
    }

    pub fn play_from(&mut self, index: usize) -> bool {
        if index >= self.queue.len() {
            return false;
        }
        self.current_index = Some(index);
        self.status = PlaybackStatus::Starting;
        self.last_error = None;
        true
    }

    pub fn advance(&mut self) -> bool {
        let Some(current) = self.current_index else {
            return false;
        };
        let next = match self.order {
            PlayOrder::Forward => current
                .checked_add(1)
                .filter(|next| *next < self.queue.len()),
            PlayOrder::Reverse => current.checked_sub(1),
            PlayOrder::Shuffle => None,
        };
        if let Some(next) = next {
            self.current_index = Some(next);
            self.status = PlaybackStatus::Starting;
            true
        } else {
            self.status = PlaybackStatus::Finished;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64) -> PlaylistItem {
        PlaylistItem {
            bvid: format!("BV{id}"),
            aid: id,
            cid: Some(id),
            title: format!("video {id}"),
            uploader_mid: Some(1),
            duration: Some(60),
            page: None,
        }
    }

    #[test]
    fn replacing_queue_resets_position_and_error() {
        let mut state = PlaybackState {
            last_error: Some("old error".into()),
            ..PlaybackState::default()
        };
        state.replace_queue(PlaylistSource::Manual, vec![item(1), item(2)]);
        assert_eq!(state.current_index, Some(0));
        assert_eq!(state.last_error, None);
    }

    #[test]
    fn forward_queue_finishes_after_last_item() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1), item(2)]);
        assert!(state.advance());
        assert_eq!(state.current_index, Some(1));
        assert!(!state.advance());
        assert_eq!(state.status, PlaybackStatus::Finished);
    }

    #[test]
    fn play_from_rejects_out_of_bounds_index() {
        let mut state = PlaybackState::default();
        state.replace_queue(PlaylistSource::Manual, vec![item(1)]);
        assert!(!state.play_from(1));
        assert_eq!(state.current_index, Some(0));
    }
}
