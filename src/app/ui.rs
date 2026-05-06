use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
    Frame,
};

use crate::app::components::{ArticlePane, FeedListPane, HeadlinePane};
use crate::app::FocusPane;

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
        &app.articles,
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

    // Status bar (bottom) or error bar
    let bottom = if let Some(ref err) = app.error {
        Paragraph::new(Line::styled(
            format!(" ERROR: {err} "),
            Style::default().fg(Color::White).bg(Color::Red),
        ))
    } else {
        let hints = " j/k nav · Tab cycle · Enter select · b bookmark · u/d scroll · q quit ";
        Paragraph::new(Line::styled(
            hints,
            Style::default().fg(Color::White).bg(Color::Blue),
        ))
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
    let article_count = app.articles.len();
    let unread = app
        .articles
        .iter()
        .filter(|a| !a.is_read)
        .count();

    let bar = format!(
        " NEWS  |  Mode: {mode}  |  Feeds: {feed_count}  |  Articles: {article_count} ({unread} new)  "
    );
    Paragraph::new(Line::styled(bar, Style::default().fg(Color::White).bg(Color::Blue)))
}
