use crate::app::App;
use crate::presentation::tui::{Component, Page};
use crossterm::event::MouseEventKind;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent},
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::io;

impl App {
    /// Main run loop
    pub async fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        // Initialize the first page
        self.init_current_page().await;

        // Store the last content area for mouse handling
        let mut last_content_area = Rect::default();

        // Scroll accumulator for high-resolution mouse wheel throttling
        // Many modern mice generate multiple scroll events per physical "click"
        const SCROLL_THRESHOLD: i32 = 15; // Accumulate 15 events before scrolling
        let mut scroll_accumulator: i32 = 0;

        while !self.should_quit {
            terminal.draw(|frame| {
                last_content_area = self.get_content_area(frame.area());
                self.draw(frame);
            })?;

            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle_input(key.code, key.modifiers).await;
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            scroll_accumulator += 1;
                            if scroll_accumulator >= SCROLL_THRESHOLD {
                                scroll_accumulator = 0;
                                self.handle_mouse(mouse, last_content_area).await;
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            scroll_accumulator -= 1;
                            if scroll_accumulator <= -SCROLL_THRESHOLD {
                                scroll_accumulator = 0;
                                self.handle_mouse(mouse, last_content_area).await;
                            }
                        }
                        _ => {
                            // Other mouse events (clicks) are handled immediately
                            self.handle_mouse(mouse, last_content_area).await;
                        }
                    },
                    _ => {}
                }
            }

            // Handle background tasks (like QR code polling)
            self.tick().await;
        }
        Ok(())
    }

    /// Get the content area excluding sidebar
    fn get_content_area(&self, area: Rect) -> Rect {
        // Login page, VideoDetail, DynamicDetail, and BangumiDetail use full area
        if matches!(
            self.current_page,
            Page::Login(_)
                | Page::VideoDetail(_)
                | Page::DynamicDetail(_)
                | Page::BangumiDetail(_)
                | Page::Up(_)
        ) {
            return area;
        }

        // Main layout with sidebar
        if self.show_sidebar {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(16), // Sidebar
                    Constraint::Min(40),    // Content
                ])
                .split(area)[1]
        } else {
            area
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Login page, VideoDetail, DynamicDetail, and BangumiDetail don't show sidebar
        if matches!(
            self.current_page,
            Page::Login(_)
                | Page::VideoDetail(_)
                | Page::DynamicDetail(_)
                | Page::BangumiDetail(_)
                | Page::Up(_)
        ) {
            match &mut self.current_page {
                Page::Login(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::VideoDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::DynamicDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::BangumiDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                Page::Up(page) => page.draw(frame, area, &self.theme, &self.keybindings),
                _ => {}
            }
            self.draw_playback_error(frame, area);
            return;
        }

        // Main layout with sidebar
        let chunks = if self.show_sidebar {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(16), // Sidebar
                    Constraint::Min(40),    // Content
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(40)])
                .split(area)
        };

        if self.show_sidebar && chunks.len() > 1 {
            self.sidebar.draw(frame, chunks[0], &self.theme);
            self.draw_page(frame, chunks[1]);
        } else {
            self.draw_page(frame, chunks[0]);
        }
        self.draw_playback_error(frame, area);
    }

    fn draw_playback_error(&self, frame: &mut Frame, area: Rect) {
        let Some(error) = self.playback.last_error.as_deref() else {
            return;
        };
        let popup = Rect {
            x: area.x,
            y: area.bottom().saturating_sub(3),
            width: area.width,
            height: 3.min(area.height),
        };
        let message = Paragraph::new(error)
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title(" 播放错误 "));
        frame.render_widget(message, popup);
    }

    fn draw_page(&mut self, frame: &mut Frame, area: Rect) {
        match &mut self.current_page {
            Page::Login(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Home(page) => {
                if let Some(notice) = self.pending_home_notice.take() {
                    page.set_footer_notice(notice);
                }
                page.draw(frame, area, &self.theme, &self.keybindings);
            }
            Page::Search(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Dynamic(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::DynamicDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::VideoDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::History(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Favorites(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Live(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::LiveDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Settings(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Bangumi(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::BangumiDetail(page) => page.draw(frame, area, &self.theme, &self.keybindings),
            Page::Up(page) => page.draw(frame, area, &self.theme, &self.keybindings),
        }
    }

    async fn handle_input(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // Playback errors remain visible until the user acknowledges them with
        // the next key press; the key still performs its normal action.
        self.playback.last_error = None;
        let keys = &self.keybindings;
        let action = match &mut self.current_page {
            Page::Login(page) => page.handle_input(key, keys),
            Page::Home(page) => page.handle_input(key, keys),
            Page::Search(page) => page.handle_input(key, keys),
            Page::Dynamic(page) => page.handle_input_with_modifiers(key, modifiers, keys),
            Page::DynamicDetail(page) => page.handle_input(key, keys),
            Page::VideoDetail(page) => page.handle_input(key, keys),
            Page::History(page) => page.handle_input(key, keys),
            Page::Favorites(page) => page.handle_input(key, keys),
            Page::Live(page) => page.handle_input(key, keys),
            Page::LiveDetail(page) => page.handle_input(key, keys),
            Page::Settings(page) => page.handle_input(key, keys),
            Page::Bangumi(page) => page.handle_input(key, keys),
            Page::BangumiDetail(page) => page.handle_input(key, keys),
            Page::Up(page) => page.handle_input(key, keys),
        };

        if let Some(action) = action {
            self.handle_action(action).await;
        }
    }

    async fn handle_mouse(&mut self, event: MouseEvent, area: Rect) {
        let action = match &mut self.current_page {
            Page::Login(page) => page.handle_mouse(event, area),
            Page::Home(page) => page.handle_mouse(event, area),
            Page::Search(page) => page.handle_mouse(event, area),
            Page::Dynamic(page) => page.handle_mouse(event, area),
            Page::DynamicDetail(page) => page.handle_mouse(event, area),
            Page::VideoDetail(page) => page.handle_mouse(event, area),
            Page::History(page) => page.handle_mouse(event, area),
            Page::Favorites(page) => page.handle_mouse(event, area),
            Page::Live(page) => page.handle_mouse(event, area),
            Page::LiveDetail(page) => page.handle_mouse(event, area),
            Page::Settings(page) => page.handle_mouse(event, area),
            Page::Bangumi(page) => page.handle_mouse(event, area),
            Page::BangumiDetail(page) => page.handle_mouse(event, area),
            Page::Up(page) => page.handle_mouse(event, area),
        };

        if let Some(action) = action {
            self.handle_action(action).await;
        }
    }

    pub(super) async fn tick(&mut self) {
        self.drain_network_events();
        if let Some((items, source, start_index, order)) = self.pending_playlist.take() {
            self.start_playlist(items, source, start_index, order).await;
        }
        while let Ok(event) = self.playback_event_rx.try_recv() {
            let accepted = self.playback.apply_event(&event);
            match event {
                crate::domain::playback::PlaybackEvent::Finished {
                    session_id,
                    bvid: Some(bvid),
                } => {
                    if accepted
                        && self.auto_return_after_playback.as_ref()
                            == Some(&(session_id, bvid.clone()))
                        && matches!(&self.current_page, Page::VideoDetail(page) if page.bvid == bvid)
                    {
                        self.auto_return_after_playback = None;
                        self.handle_action(crate::application::AppAction::BackToList)
                            .await;
                    } else if self
                        .auto_return_after_playback
                        .as_ref()
                        .is_some_and(|(id, _)| *id == session_id)
                    {
                        self.auto_return_after_playback = None;
                    }
                }
                crate::domain::playback::PlaybackEvent::Failed { session_id, .. }
                    if accepted
                        && self
                            .auto_return_after_playback
                            .as_ref()
                            .is_some_and(|(id, _)| *id == session_id) =>
                {
                    self.auto_return_after_playback = None;
                }
                _ => {}
            }
        }

        let auto_play = if let Page::VideoDetail(page) = &mut self.current_page {
            if page.auto_play_pending && !page.loading && page.video_info.is_some() {
                page.auto_play_pending = false;
                Some((page.bvid.clone(), page.play_action()))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((bvid, action)) = auto_play {
            self.handle_action(action).await;
            if let Some(session_id) = self.playback.session_id {
                self.auto_return_after_playback = Some((session_id, bvid));
            }
        }
        match &mut self.current_page {
            Page::Login(page) => {
                let client = &self.api_client;
                if let Some(action) = page.tick(client).await {
                    self.handle_action(action).await;
                }
            }
            Page::Home(page) => {
                // Non-blocking: poll completed downloads and start new ones
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::Search(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::Dynamic(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::VideoDetail(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::History(page) => {
                page.poll_cover_results();
                page.start_cover_downloads();
            }
            Page::Favorites(page) => {
                page.videos.poll_cover_results();
                page.videos.start_cover_downloads();
            }
            Page::Bangumi(page) => {
                page.index_grid.poll_cover_results();
                page.index_grid.start_cover_downloads();
            }
            Page::Up(page) => {
                page.videos.poll_cover_results();
                page.videos.start_cover_downloads();
                page.favorite_videos.poll_cover_results();
                page.favorite_videos.start_cover_downloads();
            }
            _ => {}
        }
    }
}
