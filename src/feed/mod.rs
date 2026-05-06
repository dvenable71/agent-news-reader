pub mod extract;

use anyhow::Result;
use rusqlite::Connection;
use tracing::info;

use crate::db::models::Article;

pub fn refresh_feeds(_conn: &Connection) {
    tracing::info!("refresh_feeds stub");
}

/// Extract content for a single article by ID.
/// Skips if content already exists (cache hit).
pub fn extract_article_content(conn: &Connection, article_id: i64) -> Result<()> {
    let article = Article::get(conn, article_id)?
        .ok_or_else(|| anyhow::anyhow!("article not found: {article_id}"))?;

    // Check cache
    if article.content.as_deref().is_some_and(|c| !c.is_empty()) {
        info!(article_id, "content already extracted, skipping");
        return Ok(());
    }

    let url = article
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("article has no URL"))?;

    info!(article_id, url, "extracting content");

    match extract::extract_content(url) {
        Ok(text) => {
            let mut updated = article.clone();
            updated.content = Some(text);
            Article::update(conn, &updated)?;
            Article::reset_extract_attempts(conn, article_id)?;
            info!(article_id, "content extracted successfully");
            Ok(())
        }
        Err(e) => {
            let err_str = e.to_string();
            tracing::warn!(article_id, error = %err_str, "content extraction failed");
            let mut updated = article.clone();
            updated.content = Some("[Could not extract content]".into());
            Article::update(conn, &updated)?;
            Article::bump_extract_attempts(conn, article_id)?;
            Ok(())
        }
    }
}

/// Extract content for all articles without content, up to `limit`.
pub fn extract_all(conn: &Connection, feed_id: Option<i64>, limit: i64) -> Result<usize> {
    let articles = if let Some(fid) = feed_id {
        Article::list(conn, Some(fid))?
            .into_iter()
            .filter(|a| a.content.as_deref().map_or(true, |c| c.is_empty()))
            .filter(|a| a.extract_attempts < 3)
            .take(limit as usize)
            .collect::<Vec<_>>()
    } else {
        Article::list_without_content(conn, limit)?
    };

    let count = articles.len();
    info!(count, "extracting content for articles");

    for article in &articles {
        let _ = extract_article_content(conn, article.id);
    }

    Ok(count)
}
