mod article_detail;
mod bangumi;
mod bangumi_detail;
mod dynamic;
mod dynamic_detail;
mod favorites;
mod history;
mod home;
mod live;
mod live_detail;
mod login;
mod search;
mod settings;
mod sidebar;
pub mod theme;
mod up;
mod video_card;
mod video_detail;

pub use article_detail::ArticleDetailPage;
pub use bangumi::BangumiPage;
pub use bangumi_detail::BangumiDetailPage;
pub use dynamic::{DynamicPage, DynamicTab};
pub use dynamic_detail::DynamicDetailPage;
pub use favorites::FavoritesPage;
pub use history::HistoryPage;
pub use home::HomePage;
pub use live::LivePage;
pub use live_detail::LiveDetailPage;
pub use login::LoginPage;
pub use search::SearchPage;
pub use settings::SettingsPage;
pub use sidebar::{NavItem, Sidebar};
pub use theme::{DEFAULT_THEME_ID, Theme, ThemeChoice};
pub use up::UpPage;
pub use video_card::{VideoCard, VideoCardGrid};
pub use video_detail::VideoDetailPage;

use crate::application::AppAction;
use crate::storage::Keybindings;
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyModifiers, MouseEvent},
    prelude::{Color, Line, Modifier, Rect, Span, Style},
};

/// Build the centered, bracketed shortcut footer used across list pages.
/// Each tuple is `(shortcut, label, color)`; shortcut text is emphasized while
/// labels and brackets use the secondary foreground.
pub fn shortcut_footer(
    theme: &Theme,
    items: impl IntoIterator<Item = (String, String, Color)>,
) -> Line<'static> {
    let muted = Style::default().fg(theme.fg_secondary);
    let mut spans = Vec::new();
    for (index, (shortcut, label, color)) in items.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", muted));
        }
        spans.push(Span::styled("[", muted));
        spans.push(Span::styled(
            shortcut,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled("] ", muted));
        spans.push(Span::styled(label, muted));
    }
    Line::from(spans)
}

/// UI Component trait
pub trait Component {
    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, keys: &Keybindings);
    fn handle_input(&mut self, key: KeyCode, keys: &Keybindings) -> Option<AppAction> {
        let _ = (key, keys);
        None
    }
    fn handle_input_with_modifiers(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        keys: &Keybindings,
    ) -> Option<AppAction> {
        let _ = modifiers;
        self.handle_input(key, keys)
    }
    fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<AppAction> {
        let _ = (event, area);
        None
    }
}

/// Application pages
pub enum Page {
    Login(LoginPage),
    Home(HomePage),
    Search(SearchPage),
    Dynamic(DynamicPage),
    DynamicDetail(Box<DynamicDetailPage>),
    ArticleDetail(Box<ArticleDetailPage>),
    VideoDetail(Box<VideoDetailPage>),
    Up(Box<UpPage>),
    History(HistoryPage),
    Favorites(FavoritesPage),
    Live(LivePage),
    LiveDetail(Box<LiveDetailPage>),
    Settings(Box<SettingsPage>),
    Bangumi(Box<BangumiPage>),
    BangumiDetail(Box<BangumiDetailPage>),
}
