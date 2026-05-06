pub mod models;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

const MIGRATIONS: &[(&str, i32, &str)] = &[
    ("001_initial", 1, include_str!("migrations/001_initial.sql")),
    ("002_feed_cache", 2, include_str!("migrations/002_feed_cache.sql")),
    ("003_unread_count_index", 3, include_str!("migrations/003_articles_unread_count_index.sql")),
    ("004_extract_attempts", 4, include_str!("migrations/004_extract_attempts.sql")),
];

pub fn get_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("DATABASE_URL") {
        return PathBuf::from(path);
    }
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("agent-news-reader").join("news.db")
}

pub fn init_db(db_path: &str) -> Result<Connection> {
    let parent = PathBuf::from(db_path)
        .parent()
        .context("invalid database path")?
        .to_path_buf();
    fs::create_dir_all(&parent)
        .with_context(|| format!("failed to create database directory: {}", parent.display()))?;

    let mut conn =
        Connection::open(db_path).with_context(|| format!("failed to open database: {db_path}"))?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA wal_autocheckpoint = 1000;",
    )
    .context("failed to set database pragmas")?;

    run_migrations(&mut conn)?;

    Ok(conn)
}

fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .context("failed to create schema_version table")?;

    let applied: HashSet<i32> = conn
        .prepare("SELECT version FROM schema_version")?
        .query_map([], |row| row.get::<_, i32>(0))?
        .filter_map(|r| r.ok())
        .collect();

    for (name, version, sql) in MIGRATIONS {
        if !applied.contains(version) {
            conn.execute_batch("BEGIN")?;
            if let Err(e) = conn.execute_batch(sql) {
                conn.execute_batch("ROLLBACK")?;
                return Err(e).with_context(|| format!("failed to run migration {name}"));
            }
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![version],
            )
            .with_context(|| format!("failed to record migration {name}"))?;
            conn.execute_batch("COMMIT")?;
            tracing::info!("applied migration {name} (version {version})");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_db_in_memory() {
        let conn = init_db(":memory:").expect("failed to init in-memory DB");
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 4);
    }
}
