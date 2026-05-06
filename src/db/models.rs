use anyhow::Result;
use rusqlite::{Connection, params};

#[derive(Debug, Clone)]
pub struct Feed {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_fetch_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Feed {
    pub fn list(conn: &Connection) -> Result<Vec<Feed>> {
        let mut stmt = conn.prepare(
            "SELECT id, title, url, site_url, description, etag, last_modified,
                    last_fetch_status, created_at, updated_at
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
                    etag: row.get(5)?,
                    last_modified: row.get(6)?,
                    last_fetch_status: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(feeds)
    }

    pub fn get(conn: &Connection, id: i64) -> Result<Option<Feed>> {
        let mut stmt = conn.prepare(
            "SELECT id, title, url, site_url, description, etag, last_modified,
                    last_fetch_status, created_at, updated_at
             FROM feeds WHERE id = ?",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Feed {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                site_url: row.get(3)?,
                description: row.get(4)?,
                etag: row.get(5)?,
                last_modified: row.get(6)?,
                last_fetch_status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
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

    pub fn update_cache_headers(
        conn: &Connection,
        id: i64,
        etag: Option<&str>,
        last_modified: Option<&str>,
        status: Option<&str>,
    ) -> Result<()> {
        conn.execute(
            "UPDATE feeds SET etag = ?, last_modified = ?, last_fetch_status = ?,
             updated_at = datetime('now') WHERE id = ?",
            params![etag, last_modified, status, id],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, id: i64) -> Result<()> {
        conn.execute("DELETE FROM feeds WHERE id = ?", params![id])?;
        Ok(())
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

    /// Upsert an article by GUID: insert if new, update title/summary/url/author/published_at if exists.
    /// Returns the article ID (existing or newly created).
    /// Note: last_insert_rowid() returns the matched row ID on DO UPDATE conflict
    /// and the new row ID on insert — both work correctly for this usage.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_by_guid(
        conn: &Connection,
        feed_id: i64,
        guid: &str,
        title: &str,
        url: Option<&str>,
        summary: Option<&str>,
        author: Option<&str>,
        published_at: Option<&str>,
    ) -> Result<i64> {
        conn.execute(
            "INSERT INTO articles (feed_id, guid, title, url, summary, author, published_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(guid) DO UPDATE SET
                 feed_id       = excluded.feed_id,
                 title         = excluded.title,
                 url           = excluded.url,
                 summary       = excluded.summary,
                 author        = excluded.author,
                 published_at  = excluded.published_at",
            params![feed_id, guid, title, url, summary, author, published_at],
        )?;
        Ok(conn.last_insert_rowid())
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
}
