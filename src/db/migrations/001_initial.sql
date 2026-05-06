CREATE TABLE IF NOT EXISTS feeds (
    id          INTEGER PRIMARY KEY,
    title       TEXT NOT NULL,
    url         TEXT NOT NULL UNIQUE,
    site_url    TEXT,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS articles (
    id            INTEGER PRIMARY KEY,
    feed_id       INTEGER NOT NULL,
    guid          TEXT NOT NULL UNIQUE,
    title         TEXT NOT NULL,
    url           TEXT,
    summary       TEXT,
    content       TEXT,
    author        TEXT,
    published_at  TEXT,
    is_read       INTEGER NOT NULL DEFAULT 0,
    is_bookmarked INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (feed_id) REFERENCES feeds(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_articles_feed_id ON articles(feed_id);
CREATE INDEX IF NOT EXISTS idx_articles_read ON articles(is_read);
CREATE INDEX IF NOT EXISTS idx_articles_bookmarked ON articles(is_bookmarked);
