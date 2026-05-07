use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use crate::app::components::{ArticlePane, FeedListPane, HeadlinePane};
use crate::app::{FocusPane, InputMode};

use super::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let status = render_status_bar(app);
    frame.render_widget(status, layout[0]);

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(35),
            Constraint::Percentage(45),
        ])
        .split(layout[1]);

    FeedListPane::render(
        frame,
        panes[0],
        &app.feeds,
        app.selected_feed,
        app.focus == FocusPane::FeedList,
    );

    HeadlinePane::render(
        frame,
        panes[1],
        &app.visible_articles,
        app.selected_article,
        app.focus == FocusPane::HeadlineList,
        app.feeds.get(app.selected_feed).map(|(f, _)| f.title.as_str()),
    );

    ArticlePane::render(
        frame,
        panes[2],
        app.selected_article_ref(),
        app.stripped_content.as_deref(),
        app.article_scroll,
        app.focus == FocusPane::ArticleView,
    );

    // Bottom bar: error, prompt, hints
    let bottom = match &app.input_mode {
        InputMode::Normal => {
            if let Some(ref err) = app.error {
                Paragraph::new(Line::styled(
                    format!(" {err} "),
                    Style::default().fg(Color::White).bg(Color::Red),
                ))
            } else {
                let hints = match app.focus {
                    FocusPane::FeedList =>
                        " j/k nav · Tab cycle · Enter select · a add · D delete · m daemon · R refresh · q quit ",
                    FocusPane::HeadlineList =>
                        " j/k nav · Tab cycle · Enter view · r read · b bookmark · o open · / search · f filter ",
                    FocusPane::ArticleView =>
                        " u/d scroll · Tab cycle · r read · b bookmark · o open · q quit ",
                };
                Paragraph::new(Line::styled(
                    hints,
                    Style::default().fg(Color::White).bg(Color::Blue),
                ))
            }
        }
        InputMode::AddingFeed(buf) => {
            let prompt = format!(" Add feed URL (Esc to cancel): {buf}");
            Paragraph::new(Line::styled(
                prompt,
                Style::default().fg(Color::White).bg(Color::Green),
            ))
        }
        InputMode::ConfirmDelete(_) => {
            Paragraph::new(Line::styled(
                " Delete feed? Press 'd' to confirm, 'c' or Esc to cancel ",
                Style::default().fg(Color::White).bg(Color::Red),
            ))
        }
        InputMode::Searching(buf) => {
            let prompt = format!(" Search (Esc to cancel): {buf}");
            Paragraph::new(Line::styled(
                prompt,
                Style::default().fg(Color::White).bg(Color::Green),
            ))
        }
    };
    frame.render_widget(bottom, layout[2]);
}

fn render_status_bar(app: &App) -> Paragraph<'static> {
    let mode = match app.focus {
        FocusPane::FeedList => "FEED LIST",
        FocusPane::HeadlineList => "HEADLINES",
        FocusPane::ArticleView => "ARTICLE",
    };

    let feed_count = app.feeds.len();
    let article_count = app.visible_articles.len();
    let total = app.articles.len();
    let filter = app.filter_mode.label();
    let search = if app.search_query.is_empty() {
        String::new()
    } else {
        format!(" / \"{}\"", app.search_query)
    };

    let daemon_status = if app.daemon_running.load(std::sync::atomic::Ordering::Relaxed) {
        " [DAEMON]"
    } else {
        ""
    };

    let bar = format!(
        " NEWS{daemon_status}  |  {mode}  |  Feeds: {feed_count}  |  {article_count}/{total} ({filter}{search})  "
    );
    Paragraph::new(Line::styled(bar, Style::default().fg(Color::White).bg(Color::Blue)))
}
