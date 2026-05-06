pub mod keybindings;

#[cfg(not(test))]
pub mod components;
#[cfg(not(test))]
mod ui;

#[cfg(not(test))]
use std::io::stdout;
use std::sync::atomic::AtomicBool;

#[cfg(not(test))]
use crossterm::ExecutableCommand;
#[cfg(not(test))]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
#[cfg(not(test))]
use ratatui::backend::CrosstermBackend;
#[cfg(not(test))]
use ratatui::Terminal;

use rusqlite::Connection;

use crate::db::models::{Article, Feed};

#[cfg(not(test))]
use crate::app::keybindings::{Action, dispatch};

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
            if !PANICKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
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
        if !PANICKED.load(std::sync::atomic::Ordering::SeqCst) {
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

pub struct App {
    conn: Connection,
    pub feeds: Vec<(Feed, i64)>,
    pub selected_feed: usize,
    pub articles: Vec<Article>,
    pub selected_article: usize,
    pub article_scroll: usize,
    pub focus: FocusPane,
    pub error: Option<String>,
    pub stripped_content: Option<String>,
}

impl App {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            feeds: Vec::new(),
            selected_feed: 0,
            articles: Vec::new(),
            selected_article: 0,
            article_scroll: 0,
            focus: FocusPane::FeedList,
            error: None,
            stripped_content: None,
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
                    self.article_scroll = self.article_scroll.saturating_sub(5);
                }
                Action::ScrollDown => {
                    self.article_scroll = self.article_scroll.saturating_add(5);
                }
                Action::ToggleBookmark => self.toggle_bookmark(),
                Action::None => {}
            }
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

    fn load_articles(&mut self, feed_id: Option<i64>) {
        match Article::list(&self.conn, feed_id) {
            Ok(articles) => {
                self.articles = articles;
                self.selected_article = 0;
                self.article_scroll = 0;
                self.stripped_content = None;
                self.error = None;
            }
            Err(e) => {
                self.error = Some("Failed to load articles".into());
                tracing::error!("failed to load articles: {e}");
            }
        }
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
                let max = self.articles.len().saturating_sub(1);
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
                let feed = self.feeds.get(self.selected_feed).map(|(f, _)| f.id);
                self.load_articles(feed);
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
        if let Some(article) = self.selected_article_ref().map(|a| a.id) {
            match Article::toggle_bookmark(&self.conn, article) {
                Ok(()) => {
                    let feed_id = self.feeds.get(self.selected_feed).map(|(f, _)| f.id);
                    self.load_articles(feed_id);
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some("Failed to toggle bookmark".into());
                    tracing::error!("failed to toggle bookmark: {e}");
                }
            }
        }
    }

    fn cache_article_content(&mut self) {
        let content = self.selected_article_ref().and_then(|a| {
            // If content was extracted (has extract_attempts > 0 or non-empty content),
            // use it directly — it's already formatted plain text.
            // Otherwise, strip HTML from the RSS summary.
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
        self.articles.get(
            self.selected_article.min(self.articles.len().saturating_sub(1)),
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
        format!(
            "{}…\n\n[Content truncated at {} KB]",
            &collapsed[..end],
            MAX_CONTENT_SIZE / 1024
        )
    } else {
        collapsed
    }
}

/// Truncate a string to a display width, appending an ellipsis if truncated.
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
