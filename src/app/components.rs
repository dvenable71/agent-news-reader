use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::truncate_stripped;
use crate::db::models::{Article, Feed};

pub struct FeedListPane;

impl FeedListPane {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        feeds: &[(Feed, i64)],
        selected: usize,
        focused: bool,
    ) {
        let border_style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Feeds ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if feeds.is_empty() {
            frame.render_widget(
                Paragraph::new("No feeds — run add-feed")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let idx = selected.min(feeds.len().saturating_sub(1));
        let items: Vec<ListItem> = feeds.iter().map(|(feed, unread)| {
            let content = if *unread > 0 {
                Line::from(vec![
                    Span::styled("●", Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::raw(&feed.title),
                    Span::styled(format!(" ({unread})"), Style::default().fg(Color::Yellow)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("○", Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::raw(&feed.title),
                ])
            };
            ListItem::new(content)
        }).collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        let mut state = ListState::default();
        state.select(Some(idx));
        frame.render_stateful_widget(list, inner, &mut state);
    }
}

pub struct HeadlinePane;

impl HeadlinePane {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        articles: &[Article],
        selected: usize,
        focused: bool,
        feed_title: Option<&str>,
    ) {
        let title = match feed_title {
            Some(name) => format!(" Headlines — {name} "),
            None => " Headlines — All ".to_string(),
        };

        let border_style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if articles.is_empty() {
            frame.render_widget(
                Paragraph::new("No articles")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let idx = selected.min(articles.len().saturating_sub(1));
        let items: Vec<ListItem> = articles.iter().map(|article| {
            let read_indicator = if article.is_read {
                Span::styled("○", Style::default().fg(Color::DarkGray))
            } else {
                Span::styled("●", Style::default().fg(Color::Green))
            };

            let bookmark = if article.is_bookmarked {
                Span::styled(" ★", Style::default().fg(Color::Yellow))
            } else {
                Span::raw("")
            };

            let title_text = truncate_stripped(&article.title, 60);

            let time = article.published_at.as_deref().unwrap_or("").to_string();

            ListItem::new(Line::from(vec![
                read_indicator,
                Span::raw(" "),
                Span::raw(title_text),
                bookmark,
                Span::raw("  "),
                Span::styled(time, Style::default().fg(Color::DarkGray)),
            ]))
        }).collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        let mut state = ListState::default();
        state.select(Some(idx));
        frame.render_stateful_widget(list, inner, &mut state);
    }
}

pub struct ArticlePane;

impl ArticlePane {
    pub fn render(
        frame: &mut Frame,
        area: Rect,
        article: Option<&Article>,
        content: Option<&str>,
        scroll: usize,
        focused: bool,
    ) {
        let border_style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" Article ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let Some(article) = article else {
            frame.render_widget(
                Paragraph::new("Select an article to view")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center),
                inner,
            );
            return;
        };

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::styled(&article.title, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
        lines.push(Line::from(""));

        let mut meta = Vec::new();
        if let Some(author) = &article.author {
            meta.push(format!("By {author}"));
        }
        if let Some(date) = &article.published_at {
            meta.push(date.clone());
        }
        if !meta.is_empty() {
            lines.push(Line::styled(meta.join(" · "), Style::default().fg(Color::DarkGray)));
            lines.push(Line::from(""));
        }

        let body = content.unwrap_or("");
        for line in body.lines() {
            lines.push(Line::raw(line));
        }

        let text_height = lines.len() as u16;
        let max_scroll = text_height.saturating_sub(inner.height);
        let scroll_offset = (scroll as u16).min(max_scroll);

        let paragraph = Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0));

        frame.render_widget(paragraph, inner);
    }
}

