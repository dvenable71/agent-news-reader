use anyhow::Result;
use rusqlite::{Connection, params};

#[derive(Debug, Clone)]
pub struct Feed {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Feed {
    pub fn list(conn: &Connection) -> Result<Vec<Feed>> {
        let mut stmt = conn.prepare(
            "SELECT id, title, url, site_url, description, created_at, updated_at
             FROM feeds ORDER BY title",
        )?;
        let feeds = stmt
            .query_map([], |row| {
                Ok(Feed {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    url: row.get(2)?,
                    site_url: row.get(3)?,
                    description: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(feeds)
    }

    pub fn get(conn: &Connection, id: i64) -> Result<Option<Feed>> {
        let mut stmt = conn.prepare(
            "SELECT id, title, url, site_url, description, created_at, updated_at
             FROM feeds WHERE id = ?",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Feed {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                site_url: row.get(3)?,
                description: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        match rows.next() {
            Some(Ok(feed)) => Ok(Some(feed)),
            _ => Ok(None),
        }
    }

    pub fn insert(conn: &Connection, title: &str, url: &str) -> Result<Feed> {
        conn.execute(
            "INSERT INTO feeds (title, url) VALUES (?, ?)",
            params![title, url],
        )?;
        let id = conn.last_insert_rowid();
        Self::get(conn, id)?.ok_or_else(|| anyhow::anyhow!("failed to retrieve inserted feed"))
    }

    pub fn update(conn: &Connection, feed: &Feed) -> Result<()> {
        conn.execute(
            "UPDATE feeds SET title = ?, url = ?, site_url = ?, description = ?,
             updated_at = datetime('now') WHERE id = ?",
            params![
                feed.title,
                feed.url,
                feed.site_url,
                feed.description,
                feed.id
            ],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, id: i64) -> Result<()> {
        conn.execute("DELETE FROM feeds WHERE id = ?", params![id])?;
        Ok(())
    }

    /// List all feeds with their unread article counts.
    /// Uses a LEFT JOIN so feeds with zero articles are included.
    pub fn list_with_unread_count(conn: &Connection) -> Result<Vec<(Feed, i64)>> {
        let mut stmt = conn.prepare(
            "SELECT f.id, f.title, f.url, f.site_url, f.description,
                    f.created_at, f.updated_at,
                    COUNT(CASE WHEN a.is_read = 0 THEN 1 END) AS unread
             FROM feeds f
             LEFT JOIN articles a ON a.feed_id = f.id
             GROUP BY f.id
             ORDER BY f.title",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    Feed {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        url: row.get(2)?,
                        site_url: row.get(3)?,
                        description: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    },
                    row.get::<_, i64>(7)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[derive(Debug, Clone)]
pub struct Article {
    pub id: i64,
    pub feed_id: i64,
    pub guid: String,
    pub title: String,
    pub url: Option<String>,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub is_read: bool,
    pub is_bookmarked: bool,
    pub created_at: String,
}

impl Article {
    pub fn list(conn: &Connection, feed_id: Option<i64>) -> Result<Vec<Article>> {
        match feed_id {
            Some(fid) => {
                let mut stmt = conn.prepare(
                    "SELECT id, feed_id, guid, title, url, summary, content, author,
                            published_at, is_read, is_bookmarked, created_at
                     FROM articles WHERE feed_id = ? ORDER BY published_at DESC",
                )?;
                let articles = stmt
                    .query_map(params![fid], |row| {
                        Ok(Article {
                            id: row.get(0)?,
                            feed_id: row.get(1)?,
                            guid: row.get(2)?,
                            title: row.get(3)?,
                            url: row.get(4)?,
                            summary: row.get(5)?,
                            content: row.get(6)?,
                            author: row.get(7)?,
                            published_at: row.get(8)?,
                            is_read: row.get::<_, i32>(9)? != 0,
                            is_bookmarked: row.get::<_, i32>(10)? != 0,
                            created_at: row.get(11)?,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(articles)
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, feed_id, guid, title, url, summary, content, author,
                            published_at, is_read, is_bookmarked, created_at
                     FROM articles ORDER BY published_at DESC",
                )?;
                let articles = stmt
                    .query_map([], |row| {
                        Ok(Article {
                            id: row.get(0)?,
                            feed_id: row.get(1)?,
                            guid: row.get(2)?,
                            title: row.get(3)?,
                            url: row.get(4)?,
                            summary: row.get(5)?,
                            content: row.get(6)?,
                            author: row.get(7)?,
                            published_at: row.get(8)?,
                            is_read: row.get::<_, i32>(9)? != 0,
                            is_bookmarked: row.get::<_, i32>(10)? != 0,
                            created_at: row.get(11)?,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(articles)
            }
        }
    }

    pub fn get(conn: &Connection, id: i64) -> Result<Option<Article>> {
        let mut stmt = conn.prepare(
            "SELECT id, feed_id, guid, title, url, summary, content, author,
                    published_at, is_read, is_bookmarked, created_at
             FROM articles WHERE id = ?",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Article {
                id: row.get(0)?,
                feed_id: row.get(1)?,
                guid: row.get(2)?,
                title: row.get(3)?,
                url: row.get(4)?,
                summary: row.get(5)?,
                content: row.get(6)?,
                author: row.get(7)?,
                published_at: row.get(8)?,
                is_read: row.get::<_, i32>(9)? != 0,
                is_bookmarked: row.get::<_, i32>(10)? != 0,
                created_at: row.get(11)?,
            })
        })?;
        match rows.next() {
            Some(Ok(article)) => Ok(Some(article)),
            _ => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        conn: &Connection,
        feed_id: i64,
        guid: &str,
        title: &str,
        url: Option<&str>,
        summary: Option<&str>,
        content: Option<&str>,
        author: Option<&str>,
        published_at: Option<&str>,
    ) -> Result<Article> {
        conn.execute(
            "INSERT INTO articles (feed_id, guid, title, url, summary, content, author, published_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![feed_id, guid, title, url, summary, content, author, published_at],
        )?;
        let id = conn.last_insert_rowid();
        Self::get(conn, id)?.ok_or_else(|| anyhow::anyhow!("failed to retrieve inserted article"))
    }

    pub fn update(conn: &Connection, article: &Article) -> Result<()> {
        conn.execute(
            "UPDATE articles SET title = ?, url = ?, summary = ?, content = ?,
             author = ?, published_at = ?, is_read = ?, is_bookmarked = ?
             WHERE id = ?",
            params![
                article.title,
                article.url,
                article.summary,
                article.content,
                article.author,
                article.published_at,
                article.is_read as i32,
                article.is_bookmarked as i32,
                article.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, id: i64) -> Result<()> {
        conn.execute("DELETE FROM articles WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Mark an article as read.
    pub fn mark_read(conn: &Connection, id: i64) -> Result<()> {
        conn.execute(
            "UPDATE articles SET is_read = 1 WHERE id = ?",
            params![id],
        )?;
        Ok(())
    }

    /// Toggle the bookmark state of an article.
    pub fn toggle_bookmark(conn: &Connection, id: i64) -> Result<()> {
        conn.execute(
            "UPDATE articles SET is_bookmarked = CASE WHEN is_bookmarked = 0 THEN 1 ELSE 0 END WHERE id = ?",
            params![id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn test_conn() -> Connection {
        init_db(":memory:").expect("failed to create test DB")
    }

    #[test]
    fn test_insert_and_list_feeds() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test Feed", "https://example.com/feed").unwrap();
        assert_eq!(feed.title, "Test Feed");
        assert_eq!(feed.url, "https://example.com/feed");

        let feeds = Feed::list(&conn).unwrap();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].title, "Test Feed");
    }

    #[test]
    fn test_get_feed() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();
        let found = Feed::get(&conn, feed.id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Test");

        let missing = Feed::get(&conn, 999).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_delete_feed_cascades() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();
        Article::insert(
            &conn,
            feed.id,
            "guid-1",
            "Article 1",
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        Feed::delete(&conn, feed.id).unwrap();
        assert!(Feed::get(&conn, feed.id).unwrap().is_none());
        let articles = Article::list(&conn, Some(feed.id)).unwrap();
        assert!(articles.is_empty());
    }

    #[test]
    fn test_insert_and_list_articles() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();

        Article::insert(
            &conn,
            feed.id,
            "guid-1",
            "Article 1",
            Some("https://example.com/1"),
            Some("Summary 1"),
            None,
            Some("Author 1"),
            None,
        )
        .unwrap();
        Article::insert(
            &conn,
            feed.id,
            "guid-2",
            "Article 2",
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let articles = Article::list(&conn, Some(feed.id)).unwrap();
        assert_eq!(articles.len(), 2);
        let guids: Vec<&str> = articles.iter().map(|a| a.guid.as_str()).collect();
        assert!(guids.contains(&"guid-1"));
        assert!(guids.contains(&"guid-2"));
    }

    #[test]
    fn test_article_read_bookmark() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();
        let article = Article::insert(
            &conn, feed.id, "guid-1", "Test", None, None, None, None, None,
        )
        .unwrap();
        assert!(!article.is_read);
        assert!(!article.is_bookmarked);

        let mut updated = article.clone();
        updated.is_read = true;
        updated.is_bookmarked = true;
        Article::update(&conn, &updated).unwrap();

        let fetched = Article::get(&conn, article.id).unwrap().unwrap();
        assert!(fetched.is_read);
        assert!(fetched.is_bookmarked);
    }

    #[test]
    fn test_list_with_unread_count() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();
        // Feed with zero articles appears with unread_count = 0
        let results = Feed::list_with_unread_count(&conn).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, 0);

        // Add unread article
        Article::insert(
            &conn, feed.id, "guid-1", "Unread", None, None, None, None, None,
        )
        .unwrap();
        let results = Feed::list_with_unread_count(&conn).unwrap();
        assert_eq!(results[0].1, 1);
    }

    #[test]
    fn test_mark_read() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();
        let article = Article::insert(
            &conn, feed.id, "guid-1", "Test", None, None, None, None, None,
        )
        .unwrap();
        assert!(!article.is_read);

        Article::mark_read(&conn, article.id).unwrap();
        let fetched = Article::get(&conn, article.id).unwrap().unwrap();
        assert!(fetched.is_read);
    }

    #[test]
    fn test_toggle_bookmark() {
        let conn = test_conn();
        let feed = Feed::insert(&conn, "Test", "https://example.com/feed").unwrap();
        let article = Article::insert(
            &conn, feed.id, "guid-1", "Test", None, None, None, None, None,
        )
        .unwrap();
        assert!(!article.is_bookmarked);

        Article::toggle_bookmark(&conn, article.id).unwrap();
        let fetched = Article::get(&conn, article.id).unwrap().unwrap();
        assert!(fetched.is_bookmarked);

        Article::toggle_bookmark(&conn, article.id).unwrap();
        let fetched = Article::get(&conn, article.id).unwrap().unwrap();
        assert!(!fetched.is_bookmarked);
    }
}
