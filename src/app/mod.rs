pub mod keybindings;

#[cfg(not(test))]
pub mod components;
#[cfg(not(test))]
mod ui;

#[cfg(not(test))]
use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(not(test))]
use crossterm::ExecutableCommand;
#[cfg(not(test))]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
#[cfg(not(test))]
use ratatui::backend::CrosstermBackend;
#[cfg(not(test))]
use ratatui::Terminal;

use rusqlite::Connection;

#[cfg(not(test))]
use crate::app::keybindings::{Action, dispatch};
use crate::db::models::{Article, Feed};

const MAX_CONTENT_SIZE: usize = 64 * 1024; // 64 KB

static PANICKED: AtomicBool = AtomicBool::new(false);

/// Guard that restores terminal state on drop, even during a panic unwind.
#[cfg(not(test))]
struct TerminalGuard;

#[cfg(not(test))]
impl TerminalGuard {
    fn install() -> Self {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !PANICKED.swap(true, Ordering::SeqCst) {
                let _ = disable_raw_mode();
                let _ = stdout().execute(LeaveAlternateScreen);
            }
            previous(info);
        }));
        TerminalGuard
    }
}

#[cfg(not(test))]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !PANICKED.load(Ordering::SeqCst) {
            let _ = disable_raw_mode();
            let _ = stdout().execute(LeaveAlternateScreen);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    FeedList,
    HeadlineList,
    ArticleView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    AddingFeed(String),
    ConfirmDelete(usize),
    Searching(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    All,
    UnreadOnly,
    BookmarkedOnly,
}

impl FilterMode {
    fn as_str(&self) -> &'static str {
        match self {
            FilterMode::All => "",
            FilterMode::UnreadOnly => "unread",
            FilterMode::BookmarkedOnly => "bookmarked",
        }
    }

    fn next(&self) -> Self {
        match self {
            FilterMode::All => FilterMode::UnreadOnly,
            FilterMode::UnreadOnly => FilterMode::BookmarkedOnly,
            FilterMode::BookmarkedOnly => FilterMode::All,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            FilterMode::All => "ALL",
            FilterMode::UnreadOnly => "UNREAD",
            FilterMode::BookmarkedOnly => "BOOKMARKED",
        }
    }
}

pub struct App {
    conn: Connection,
    pub feeds: Vec<(Feed, i64)>,
    pub selected_feed: usize,
    pub articles: Vec<Article>,
    pub visible_articles: Vec<Article>,
    pub selected_article: usize,
    pub article_scroll: usize,
    pub focus: FocusPane,
    pub error: Option<String>,
    pub stripped_content: Option<String>,
    pub input_mode: InputMode,
    pub filter_mode: FilterMode,
    pub search_query: String,
    refresh_running: Arc<AtomicBool>,
    refresh_done: Arc<AtomicBool>,
    pub daemon_running: Arc<AtomicBool>,
    daemon_stop: Arc<AtomicBool>,
}

impl App {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            feeds: Vec::new(),
            selected_feed: 0,
            articles: Vec::new(),
            visible_articles: Vec::new(),
            selected_article: 0,
            article_scroll: 0,
            focus: FocusPane::FeedList,
            error: None,
            stripped_content: None,
            input_mode: InputMode::Normal,
            filter_mode: FilterMode::All,
            search_query: String::new(),
            refresh_running: Arc::new(AtomicBool::new(false)),
            refresh_done: Arc::new(AtomicBool::new(false)),
            daemon_running: Arc::new(AtomicBool::new(false)),
            daemon_stop: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(not(test))]
    pub fn run_tui(mut self) {
        let _guard = TerminalGuard::install();
        enable_raw_mode().expect("failed to enable raw mode");
        stdout()
            .execute(EnterAlternateScreen)
            .expect("failed to enter alternate screen");

        let backend = CrosstermBackend::new(stdout());
        let mut terminal =
            Terminal::new(backend).expect("failed to create ratatui terminal");

        self.load_feeds();
        let mut render_errors: u8 = 0;

        loop {
            // Check if background refresh completed
            if self.refresh_done.load(Ordering::Acquire) {
                self.refresh_done.store(false, Ordering::Release);
                self.refresh_running.store(false, Ordering::Release);
                self.load_feeds();
                self.reload_articles();
            }

            let draw_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = terminal.draw(|frame| {
                    ui::draw(frame, &self);
                });
            }));
            if draw_result.is_err() {
                self.error = Some("Render error — check logs".into());
                tracing::error!("TUI render panic");
                render_errors += 1;
                if render_errors >= 3 {
                    tracing::error!("too many render errors, aborting TUI");
                    break;
                }
            } else {
                render_errors = 0;
            }

            // Handle input mode
            match &self.input_mode {
                InputMode::AddingFeed(_) | InputMode::Searching(_) => {
                    match crossterm::event::read() {
                        Ok(crossterm::event::Event::Key(key)) => self.handle_input_key(key),
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!("event read error: {e}");
                        }
                    }
                    continue;
                }
                InputMode::ConfirmDelete(_) => {
                    match crossterm::event::read() {
                        Ok(crossterm::event::Event::Key(key)) => self.handle_confirm_key(key),
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!("event read error: {e}");
                        }
                    }
                    continue;
                }
                InputMode::Normal => {}
            }

            let event = match crossterm::event::read() {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("event read error: {e}");
                    continue;
                }
            };

            match dispatch(&event) {
                Action::Quit => break,
                Action::CyclePane => self.cycle_focus(),
                Action::Up => self.navigate_up(),
                Action::Down => self.navigate_down(),
                Action::Select => self.select_current(),
                Action::ScrollUp => {
                    if self.focus == FocusPane::ArticleView {
                        self.article_scroll = self.article_scroll.saturating_sub(5);
                    }
                }
                Action::ScrollDown => {
                    if self.focus == FocusPane::ArticleView {
                        self.article_scroll = self.article_scroll.saturating_add(5);
                    }
                }
                Action::ToggleBookmark => self.toggle_bookmark(),
                Action::ToggleRead => self.toggle_read(),
                Action::AddFeed => {
                    self.input_mode = InputMode::AddingFeed(String::new());
                }
                Action::DeleteFeed => {
                    if self.focus == FocusPane::FeedList && !self.feeds.is_empty() {
                        self.input_mode = InputMode::ConfirmDelete(self.selected_feed);
                    }
                }
                Action::OpenInBrowser => self.open_in_browser(),
                Action::Search => {
                    self.input_mode = InputMode::Searching(String::new());
                }
                Action::Escape => {
                    self.input_mode = InputMode::Normal;
                }
                Action::Refresh => self.start_refresh(),
                Action::CycleFilter => {
                    self.filter_mode = self.filter_mode.next();
                    self.reload_articles();
                }
                Action::ToggleDaemon => self.toggle_daemon(),
                Action::None => {}
            }
        }
    }

    fn handle_input_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        if key.code == crossterm::event::KeyCode::Char('c') && key.modifiers == crossterm::event::KeyModifiers::CONTROL {
            self.input_mode = InputMode::Normal;
            return;
        }
        let is_search = matches!(&self.input_mode, InputMode::Searching(_));
        match (&mut self.input_mode, key.code) {
            (InputMode::AddingFeed(buf), KeyCode::Char(c)) => buf.push(c),
            (InputMode::Searching(buf), KeyCode::Char(c)) => buf.push(c),
            (InputMode::AddingFeed(buf), KeyCode::Enter) => {
                let url = std::mem::take(buf);
                self.input_mode = InputMode::Normal;
                self.add_feed(&url);
                return;
            }
            (InputMode::Searching(buf), KeyCode::Enter) => {
                self.search_query = std::mem::take(buf);
                self.input_mode = InputMode::Normal;
                return;
            }
            (InputMode::AddingFeed(buf), KeyCode::Backspace) => {
                buf.pop();
            }
            (InputMode::Searching(buf), KeyCode::Backspace) => {
                buf.pop();
            }
            (_, KeyCode::Esc) => {
                self.input_mode = InputMode::Normal;
                return;
            }
            _ => {}
        }
        if is_search
            && let InputMode::Searching(ref q) = self.input_mode {
                self.search_query = q.clone();
                self.apply_visible_filter();
            }
    }

    fn handle_confirm_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        let idx = match &self.input_mode {
            InputMode::ConfirmDelete(i) => *i,
            _ => return,
        };
        if key.code == KeyCode::Char('c') && key.modifiers == crossterm::event::KeyModifiers::CONTROL {
            self.input_mode = InputMode::Normal;
            return;
        }
        match key.code {
            KeyCode::Char('d') => {
                self.input_mode = InputMode::Normal;
                let feed_id = self.feeds.get(idx).map(|(f, _)| f.id);
                if let Some(id) = feed_id {
                    match Feed::delete(&self.conn, id) {
                        Ok(()) => {
                            self.load_feeds();
                            self.articles.clear();
                            self.visible_articles.clear();
                            self.selected_article = 0;
                            self.error = None;
                        }
                        Err(e) => {
                            self.error = Some("Failed to delete feed".into());
                            tracing::error!("failed to delete feed: {e}");
                        }
                    }
                }
            }
            KeyCode::Char('c') | KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    fn add_feed(&mut self, url: &str) {
        // Validate URL
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => {
                self.error = Some("Invalid URL".into());
                return;
            }
        };
        if parsed.scheme() != "https" {
            self.error = Some("Feed URL must use HTTPS".into());
            return;
        }

        // Use the URL as a placeholder title; refresh will update it
        match Feed::insert(&self.conn, url, url) {
            Ok(_) => {
                self.load_feeds();
                self.error = None;
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("UNIQUE") {
                    self.error = Some("Feed URL already exists".into());
                } else {
                    self.error = Some("Failed to add feed".into());
                    tracing::error!("failed to add feed: {e}");
                }
            }
        }
    }

    fn open_in_browser(&self) {
        let url = match self.focus {
            FocusPane::ArticleView | FocusPane::HeadlineList => {
                self.selected_article_ref().and_then(|a| a.url.as_deref())
            }
            FocusPane::FeedList => self.feeds.get(self.selected_feed).map(|(f, _)| f.url.as_str()),
        };

        let Some(url) = url else { return };

        // Validate URL before opening
        if url.starts_with('-') {
            return;
        }
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return;
        }

        let result = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(url).spawn()
        } else if cfg!(target_os = "linux") {
            std::process::Command::new("xdg-open").arg(url).spawn()
        } else if cfg!(target_os = "windows") {
            // Use ShellExecuteW via rundll32 to avoid cmd.exe argument injection
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", url])
                .spawn()
        } else {
            return;
        };

        if let Err(e) = result {
            tracing::error!("failed to open URL: {e}");
        }
    }

    fn start_refresh(&mut self) {
        if self.refresh_running.load(Ordering::Acquire) {
            return; // Already refreshing
        }

        let feed_id = self.feeds.get(self.selected_feed).map(|(f, _)| f.id);
        let db_path = crate::db::get_db_path().to_string_lossy().to_string();
        let running = self.refresh_running.clone();
        let done = self.refresh_done.clone();

        if !running.swap(true, Ordering::AcqRel) {
            std::thread::spawn(move || {
                let db_path_str = db_path.to_string();
                match crate::db::init_db(&db_path_str) {
                    Ok(conn) => {
                        crate::feed::refresh_feeds(&conn, feed_id);
                        let _ = conn.close();
                    }
                    Err(e) => {
                        tracing::error!("failed to open DB for refresh: {e}");
                    }
                }
                running.store(false, Ordering::Release);
                done.store(true, Ordering::Release);
            });
            self.error = Some("Refreshing...".into());
        }
    }

    fn toggle_daemon(&mut self) {
        // Use compare_exchange to prevent races: only start if not running
        let was_running = self.daemon_running.swap(true, Ordering::AcqRel);
        if was_running {
            self.daemon_running.store(false, Ordering::Release);
            self.daemon_stop.store(true, Ordering::Release);
            self.error = Some("Daemon stopped".into());
        } else {
            self.daemon_stop.store(false, Ordering::Release);
            let running = self.daemon_running.clone();
            let stop = self.daemon_stop.clone();
            let db_path = crate::db::get_db_path().to_string_lossy().to_string();

            std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let interval = std::time::Duration::from_secs(15 * 60);

                    while !stop.load(Ordering::Acquire) {
                        match crate::db::init_db(&db_path) {
                            Ok(conn) => {
                                crate::feed::refresh_feeds(&conn, None);
                                let _ = conn.close();
                            }
                            Err(e) => {
                                tracing::error!("daemon: failed to open DB: {e}");
                            }
                        }
                        for _ in 0..interval.as_secs() {
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }));
                if result.is_err() {
                    tracing::error!("daemon thread panicked");
                }
                running.store(false, Ordering::Release);
            });
            self.error = Some("Daemon started".into());
        }
    }

    fn load_feeds(&mut self) {
        match Feed::list_with_unread_count(&self.conn) {
            Ok(feeds) => {
                self.feeds = feeds;
                self.selected_feed = self.selected_feed.min(self.feeds.len().saturating_sub(1));
                self.error = None;
            }
            Err(e) => {
                self.error = Some("Failed to load feeds".into());
                tracing::error!("failed to load feeds: {e}");
            }
        }
    }

    fn reload_articles(&mut self) {
        let feed_id = self.feeds.get(self.selected_feed).map(|(f, _)| f.id);
        self.articles = match Article::list_filtered(&self.conn, feed_id, self.filter_mode.as_str(), None, None) {
            Ok(a) => a,
            Err(e) => {
                self.error = Some("Failed to load articles".into());
                tracing::error!("failed to load articles: {e}");
                return;
            }
        };
        self.selected_article = 0;
        self.article_scroll = 0;
        self.stripped_content = None;
        self.apply_visible_filter();
    }

    fn apply_visible_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        self.visible_articles = if q.is_empty() {
            self.articles.clone()
        } else {
            self.articles.iter()
                .filter(|a| {
                    a.title.to_lowercase().contains(&q)
                        || a.author.as_deref().is_some_and(|au| au.to_lowercase().contains(&q))
                })
                .cloned()
                .collect()
        };
        self.selected_article = self.selected_article.min(self.visible_articles.len().saturating_sub(1));
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            FocusPane::FeedList => FocusPane::HeadlineList,
            FocusPane::HeadlineList => FocusPane::ArticleView,
            FocusPane::ArticleView => FocusPane::FeedList,
        };
    }

    fn navigate_up(&mut self) {
        match self.focus {
            FocusPane::FeedList => {
                self.selected_feed = self.selected_feed.saturating_sub(1);
            }
            FocusPane::HeadlineList => {
                self.selected_article = self.selected_article.saturating_sub(1);
                self.article_scroll = 0;
                self.cache_article_content();
            }
            FocusPane::ArticleView => {
                self.article_scroll = self.article_scroll.saturating_sub(1);
            }
        }
    }

    fn navigate_down(&mut self) {
        match self.focus {
            FocusPane::FeedList => {
                let max = self.feeds.len().saturating_sub(1);
                self.selected_feed = self.selected_feed.saturating_add(1).min(max);
            }
            FocusPane::HeadlineList => {
                let max = self.visible_articles.len().saturating_sub(1);
                self.selected_article = self.selected_article.saturating_add(1).min(max);
                self.article_scroll = 0;
                self.cache_article_content();
            }
            FocusPane::ArticleView => {
                self.article_scroll = self.article_scroll.saturating_add(1);
            }
        }
    }

    fn select_current(&mut self) {
        match self.focus {
            FocusPane::FeedList => {
                self.reload_articles();
                self.focus = FocusPane::HeadlineList;
            }
            FocusPane::HeadlineList => {
                if let Some(article) = self.selected_article_ref() {
                    let _ = Article::mark_read(&self.conn, article.id);
                    self.cache_article_content();
                }
                self.focus = FocusPane::ArticleView;
            }
            FocusPane::ArticleView => {}
        }
    }

    fn toggle_bookmark(&mut self) {
        if let Some(article) = self.selected_article_ref().cloned() {
            match Article::toggle_bookmark(&self.conn, article.id) {
                Ok(()) => self.reload_articles(),
                Err(e) => {
                    self.error = Some("Failed to toggle bookmark".into());
                    tracing::error!("failed to toggle bookmark: {e}");
                }
            }
        }
    }

    fn toggle_read(&mut self) {
        if let Some(article) = self.selected_article_ref().cloned() {
            match Article::toggle_read(&self.conn, article.id) {
                Ok(()) => self.reload_articles(),
                Err(e) => {
                    self.error = Some("Failed to toggle read".into());
                    tracing::error!("failed to toggle read: {e}");
                }
            }
        }
    }

    fn cache_article_content(&mut self) {
        let content = self.selected_article_ref().and_then(|a| {
            let has_extracted = a.extract_attempts > 0
                || a.content.as_deref().is_some_and(|c| !c.is_empty());
            if has_extracted {
                a.content.as_deref().map(|s| s.to_string())
            } else {
                a.summary.as_deref().map(strip_html)
            }
        });
        self.stripped_content = content;
    }

    pub fn selected_article_ref(&self) -> Option<&Article> {
        self.visible_articles.get(
            self.selected_article.min(self.visible_articles.len().saturating_sub(1)),
        )
    }
}

/// Strip HTML tags from content, collapse whitespace, and limit to MAX_CONTENT_SIZE.
pub fn strip_html(content: &str) -> String {
    use scraper::Html;
    let doc = Html::parse_fragment(content);
    let text: String = doc.root_element().text().collect::<Vec<_>>().join(" ");
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() > MAX_CONTENT_SIZE {
        let mut end = MAX_CONTENT_SIZE;
        while !collapsed.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…\n\n[Content truncated at {} KB]", &collapsed[..end], MAX_CONTENT_SIZE / 1024)
    } else {
        collapsed
    }
}

fn truncate_stripped(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_mode_cycle() {
        assert_eq!(FilterMode::All.next(), FilterMode::UnreadOnly);
        assert_eq!(FilterMode::UnreadOnly.next(), FilterMode::BookmarkedOnly);
        assert_eq!(FilterMode::BookmarkedOnly.next(), FilterMode::All);
    }

    #[test]
    fn test_filter_mode_labels() {
        assert_eq!(FilterMode::All.label(), "ALL");
        assert_eq!(FilterMode::UnreadOnly.label(), "UNREAD");
        assert_eq!(FilterMode::BookmarkedOnly.label(), "BOOKMARKED");
    }

    #[test]
    fn test_strip_html_removes_tags() {
        assert_eq!(strip_html("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn test_strip_html_empty() {
        assert_eq!(strip_html(""), "");
    }

    #[test]
    fn test_strip_html_plain_text() {
        assert_eq!(strip_html("plain text"), "plain text");
    }

    #[test]
    fn test_truncate_stripped_short() {
        assert_eq!(truncate_stripped("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_stripped_long() {
        let result = truncate_stripped("hello world", 5);
        assert!(result.starts_with("hello"));
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_stripped_char_boundary() {
        let s = "café";
        let result = truncate_stripped(s, 4);
        assert_eq!(result, "caf…");
    }
}
