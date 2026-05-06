CREATE INDEX IF NOT EXISTS idx_articles_unread ON articles(feed_id) WHERE is_read = 0;
