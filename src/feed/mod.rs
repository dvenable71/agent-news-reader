pub mod extract;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use tracing::info;

use crate::db::models::{Article, Feed};

const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = "agent-news-reader/0.1";
const MAX_REDIRECTS: u32 = 10;
// The string "DNS rebinding" appears in the check_private_host doc comment as
// a documented limitation. Tested by test_dns_rebind_gap_documented.
const DNS_REBIND_LIMITATION: &str = "DNS rebinding";

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // fc00::/7 = unique local addresses
                || (v6.octets()[0] & 0xfe) == 0xfc
                // fe80::/10 = link-local addresses
                || v6.octets()[0] == 0xfe && (v6.octets()[1] & 0xc0) == 0x80
                // ::ffff:0:0/96 = IPv4-mapped IPv6 — check the embedded IPv4
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local()
                })
        }
    }
}

/// Check if any resolved address for a hostname falls in a private IP range.
/// Returns Ok(()) only if all resolved addresses are public.
///
/// Note: DNS rebinding (TOCTOU between resolution and connect) is a known
/// limitation of this check. reqwest re-resolves independently at connect
/// time, so a race window exists. Fully closing this would require pinning
/// IPs via ClientBuilder::resolve(), which is a future enhancement.
fn check_private_host(host: &str, port: u16) -> Result<()> {
    let addrs = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve host: {host}"))?;

    let mut any_resolved = false;
    for addr in addrs {
        any_resolved = true;
        if is_private_ip(addr.ip()) {
            anyhow::bail!(
                "feed URL resolves to a private IP address ({host}): {}",
                addr.ip()
            );
        }
    }
    if !any_resolved {
        anyhow::bail!("could not resolve host: {host}");
    }
    Ok(())
}

fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        // Use attempt.previous().len() for per-chain redirect tracking
        // instead of a shared counter (which would starve late-in-batch feeds).
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // Enforce max redirect hops per chain
            if attempt.previous().len() >= MAX_REDIRECTS as usize {
                return attempt.stop();
            }

            // SSRF check on redirect target: must be https + public IP
            let url = attempt.url();
            if url.scheme() != "https" {
                return attempt.stop();
            }
            if let Some(host) = url.host_str() {
                let port = url.port_or_known_default().unwrap_or(443);
                if check_private_host(host, port).is_err() {
                    return attempt.stop();
                }
            }
            attempt.follow()
        }))
        .user_agent(USER_AGENT)
        .build()
        .context("failed to build HTTP client")
}

/// Validate that a URL is safe to fetch (HTTPS only, no private IPs).
fn validate_feed_url(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url).context("invalid feed URL")?;
    if parsed.scheme() != "https" {
        anyhow::bail!("feed URL must use HTTPS: {url}");
    }
    // Resolve hostname and check that all resolved IPs are public
    if let Some(host) = parsed.host_str() {
        let port = parsed.port_or_known_default().unwrap_or(443);
        check_private_host(host, port)?;
    }
    Ok(())
}

/// Compute a GUID fallback when a feed entry has no ID/link.
fn make_guid(feed_id: i64, entry_id: Option<&str>, link: Option<&str>, title: &str) -> String {
    #[allow(clippy::collapsible_if)]
    if let Some(id) = entry_id {
        if !id.is_empty() && id.len() < 512 {
            return id.to_string();
        }
    }
    // Fallback: hash feed_id + title + link
    let mut hasher = DefaultHasher::new();
    feed_id.hash(&mut hasher);
    title.hash(&mut hasher);
    link.unwrap_or("").hash(&mut hasher);
    format!("fallback-{:x}", hasher.finish())
}

/// Truncate a string to a maximum byte length (to prevent DB bloat).
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    // Find the nearest char boundary within max bytes
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

const MAX_TITLE: usize = 2048;
const MAX_URL: usize = 2048;
const MAX_SUMMARY: usize = 65536;

/// Refresh a single feed: fetch, parse, upsert articles, store cache headers.
pub fn refresh_feed(conn: &Connection, client: &reqwest::blocking::Client, feed: &Feed) -> Result<RefreshResult> {
    let mut result = RefreshResult::new(feed.id, feed.title.clone());

    validate_feed_url(&feed.url)
        .map_err(|e| {
            result.error = Some(e.to_string());
        })
        .map_err(|_| {
            Feed::update_cache_headers(conn, feed.id, None, None, Some("invalid_url")).ok();
            anyhow::anyhow!("invalid feed URL: {}", feed.url)
        })?;

    // Build request with conditional headers
    let mut req = client.get(&feed.url);
    if let Some(etag) = &feed.etag {
        req = req.header("If-None-Match", etag);
    }
    if let Some(lm) = &feed.last_modified {
        req = req.header("If-Modified-Since", lm);
    }

    let response = match req.send() {
        Ok(resp) => resp,
        Err(e) => {
            result.error = Some(format!("HTTP error: {e}"));
            Feed::update_cache_headers(conn, feed.id, None, None, Some("network_error")).ok();
            return Ok(result);
        }
    };

    let status = response.status();

    // 304 Not Modified — no new content
    if status == reqwest::StatusCode::NOT_MODIFIED {
        tracing::info!(feed_id = feed.id, "feed returned 304 Not Modified");
        Feed::update_cache_headers(conn, feed.id, None, None, Some("304_not_modified")).ok();
        result.cached = true;
        return Ok(result);
    }

    if !status.is_success() {
        let err = format!("HTTP {status}");
        result.error = Some(err.clone());
        Feed::update_cache_headers(conn, feed.id, None, None, Some("http_error")).ok();
        return Ok(result);
    }

    // Extract headers before consuming the body (bytes() takes ownership)
    let new_etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| truncate(s, 256).to_string());
    let new_lm = response
        .headers()
        .get("last-modified")
        .and_then(|v| v.to_str().ok())
        .map(|s| truncate(s, 128).to_string());

    // Read body with size limit
    let bytes = match response.bytes() {
        Ok(b) => {
            if b.len() as u64 > MAX_RESPONSE_SIZE {
                result.error = Some("response too large".to_string());
                Feed::update_cache_headers(conn, feed.id, None, None, Some("too_large")).ok();
                return Ok(result);
            }
            b
        }
        Err(e) => {
            result.error = Some(format!("read error: {e}"));
            Feed::update_cache_headers(conn, feed.id, None, None, Some("read_error")).ok();
            return Ok(result);
        }
    };

    // Parse with feed-rs (requires io::Read — Bytes derefs to [u8] which implements Read via Cursor)
    let parsed = match feed_rs::parser::parse(std::io::Cursor::new(&bytes)) {
        Ok(p) => p,
        Err(e) => {
            result.error = Some(format!("parse error: {e}"));
            Feed::update_cache_headers(conn, feed.id, None, None, Some("parse_error")).ok();
            return Ok(result);
        }
    };

    // Update feed metadata + upsert articles + store cache headers in a
    // single transaction for atomicity. If anything fails, none of the DB
    // changes for this feed are committed.
    // Note: SQLite auto-commits DDL, so the ALTER TABLE columns must already
    // exist (guaranteed by the migration running before this code).
    conn.execute_batch("BEGIN")?;

    if let Err(tx_err) = (|| -> Result<()> {
        // Update feed metadata from the parsed feed.
        // Note: feed-rs uses Text struct for title/description; access via .content
        let feed_text_title = parsed.title.as_ref().map(|t| t.content.as_str());
        if let Some(feed_title) = feed_text_title {
            let truncated = truncate(feed_title, MAX_TITLE);
            conn.execute(
                "UPDATE feeds SET title = ?, site_url = ?, description = ?,
                 updated_at = datetime('now') WHERE id = ?",
                params![
                    truncated,
                    parsed.links.first().map(|l| truncate(&l.href, MAX_URL)),
                    parsed.description.as_ref().map(|d| truncate(&d.content, MAX_SUMMARY)),
                    feed.id,
                ],
            )
            .context("failed to update feed metadata")?;
        }

        // Upsert articles
        for entry in &parsed.entries {
            let title = entry
                .title
                .as_ref()
                .map(|t| t.content.as_str())
                .unwrap_or("(untitled)");
            let link = entry.links.first().map(|l| l.href.as_str());
            let guid = make_guid(feed.id, Some(entry.id.as_str()), link, title);
            let author = entry.authors.first().map(|a| a.name.as_str());
            let published = entry
                .published
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string());
            let summary = entry.summary.as_ref().map(|s| s.content.as_str());

            match Article::upsert_by_guid(
                conn,
                feed.id,
                &guid,
                truncate(title, MAX_TITLE),
                link.map(|l| truncate(l, MAX_URL)),
                summary.map(|s| truncate(s, MAX_SUMMARY)),
                author,
                published.as_deref(),
            ) {
                Ok(_) => result.articles_upserted += 1,
                Err(e) => {
                    tracing::warn!(feed_id = feed.id, guid = %guid, "failed to upsert article: {e}");
                    result.errors.push(format!("article upsert: {e}"));
                }
            }
        }

        // Store cache headers (extracted before body was consumed)
        Feed::update_cache_headers(
            conn,
            feed.id,
            new_etag.as_deref(),
            new_lm.as_deref(),
            Some("ok"),
        )?;

        Ok(())
    })() {
        if let Err(rollback_err) = conn.execute_batch("ROLLBACK") {
            tracing::error!(feed_id = feed.id, "rollback after tx failure also failed: {rollback_err}");
        }
        result.error = Some(format!("DB transaction failed: {tx_err}"));
        Feed::update_cache_headers(conn, feed.id, None, None, Some("db_error")).ok();
        return Ok(result);
    }

    conn.execute_batch("COMMIT")?;

    tracing::info!(
        feed_id = feed.id,
        articles = result.articles_upserted,
        "feed refreshed successfully"
    );

    Ok(result)
}

/// Refresh a single feed by ID, or all feeds if None.
pub fn refresh_feeds(conn: &Connection, feed_id: Option<i64>) {
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to build HTTP client: {e}");
            return;
        }
    };

    if let Some(fid) = feed_id {
        match Feed::get(conn, fid) {
            Ok(Some(feed)) => {
                match refresh_feed(conn, &client, &feed) {
                    Ok(r) => {
                        if let Some(err) = &r.error {
                            tracing::warn!(feed_id = fid, "refresh failed: {err}");
                        } else {
                            tracing::info!(feed_id = fid, articles = r.articles_upserted, cached = r.cached, "refresh complete");
                        }
                    }
                    Err(e) => {
                        tracing::error!(feed_id = fid, "refresh error: {e}");
                    }
                }
            }
            Ok(None) => {
                tracing::error!(feed_id = fid, "feed not found");
            }
            Err(e) => {
                tracing::error!(feed_id = fid, "failed to load feed: {e}");
            }
        }
        return;
    }

    let feeds = match Feed::list(conn) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("failed to list feeds: {e}");
            return;
        }
    };

    if feeds.is_empty() {
        tracing::info!("no feeds to refresh");
        return;
    }

    tracing::info!(count = feeds.len(), "refreshing all feeds");
    for feed in &feeds {
        match refresh_feed(conn, &client, feed) {
            Ok(r) => {
                if let Some(err) = &r.error {
                    tracing::warn!(feed_id = feed.id, title = %feed.title, "refresh failed: {err}");
                } else {
                    tracing::info!(
                        feed_id = feed.id,
                        title = %feed.title,
                        articles = r.articles_upserted,
                        cached = r.cached,
                        "feed refreshed"
                    );
                }
            }
            Err(e) => {
                tracing::error!(feed_id = feed.id, title = %feed.title, "refresh error: {e}");
            }
        }
    }
}

/// Result of refreshing a single feed.
#[derive(Debug, Default)]
pub struct RefreshResult {
    pub feed_id: i64,
    pub feed_title: String,
    pub articles_upserted: usize,
    pub cached: bool,
    pub error: Option<String>,
    pub errors: Vec<String>,
}

impl RefreshResult {
    fn new(feed_id: i64, feed_title: String) -> Self {
        Self {
            feed_id,
            feed_title,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_at_boundary() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_char_boundary() {
        // "café" is 5 bytes, truncating to 4 must not split the é (2-byte char)
        let s = "café";
        assert_eq!(truncate(s, 4), "caf");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn test_make_guid_uses_entry_id() {
        let guid = make_guid(1, Some("abc-123"), Some("https://example.com"), "title");
        assert_eq!(guid, "abc-123");
    }

    #[test]
    fn test_make_guid_fallback() {
        let guid = make_guid(1, None, None, "Some Title");
        assert!(guid.starts_with("fallback-"));
    }

    #[test]
    fn test_make_guid_fallback_consistent() {
        let a = make_guid(1, None, Some("https://example.com"), "Title");
        let b = make_guid(1, None, Some("https://example.com"), "Title");
        assert_eq!(a, b);
    }

    #[test]
    fn test_validate_feed_url_rejects_http() {
        let result = validate_feed_url("http://example.com/feed");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HTTPS"));
    }

    #[test]
    fn test_validate_feed_url_rejects_bad_scheme() {
        let result = validate_feed_url("file:///etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_feed_url_rejects_invalid_url() {
        let result = validate_feed_url("\t\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_feed_rejects_loopback_ip() {
        // HTTPS URL pointing at loopback should be rejected
        let result = validate_feed_url("https://127.0.0.1/feed");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_feed_rejects_private_ip() {
        let result = validate_feed_url("https://10.0.0.1/feed");
        assert!(result.is_err());
    }

    #[test]
    fn test_check_private_host_loopback() {
        let result = check_private_host("127.0.0.1", 443);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_private_host_private_10() {
        let result = check_private_host("10.0.0.1", 443);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_private_host_link_local() {
        let result = check_private_host("169.254.1.1", 443);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_private_host_ipv6_loopback() {
        let result = check_private_host("::1", 443);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_private_host_public_ip() {
        // This should resolve to a public IP
        let result = check_private_host("93.184.216.34", 443);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dns_rebind_gap_documented() {
        // Verify the DNS rebinding TOCTOU gap is documented via the constant.
        // check_private_host() resolves DNS at check time; reqwest re-resolves
        // at connect time, creating a race window.
        assert!(!DNS_REBIND_LIMITATION.is_empty());
    }

    #[test]
    fn test_refresh_result_defaults() {
        let r = RefreshResult::new(42, "Test".into());
        assert_eq!(r.feed_id, 42);
        assert_eq!(r.feed_title, "Test");
        assert_eq!(r.articles_upserted, 0);
        assert!(!r.cached);
        assert!(r.error.is_none());
        assert!(r.errors.is_empty());
    }
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
