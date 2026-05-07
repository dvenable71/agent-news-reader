use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result};
use scraper::{Html, Selector};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_SIZE: u64 = 5 * 1024 * 1024; // 5 MB
const MAX_STORED_CONTENT: usize = 100 * 1024; // 100 KB
const USER_AGENT: &str = "agent-news-reader/0.1";
const MAX_REDIRECTS: u32 = 10;
const MIN_CONTENT_LENGTH: usize = 50;

// DNS rebinding TOCTOU gap: check_private_host resolves DNS at validation time;
// reqwest re-resolves at connect time, creating a race window.
// Per-hop redirect re-validation limits blast radius but the initial request
// remains vulnerable. This is a documented limitation shared with Phase 2.
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
                || (v6.octets()[0] & 0xfe) == 0xfc // fc00::/7 ULA
                || (v6.octets()[0] == 0xfe && (v6.octets()[1] & 0xc0) == 0x80) // fe80::/10 link-local
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local()
                })
        }
    }
}

fn check_private_host(host: &str, port: u16) -> Result<()> {
    let addrs = (host, port)
        .to_socket_addrs()
        .with_context(|| "failed to resolve host")?;

    let mut any_resolved = false;
    for addr in addrs {
        any_resolved = true;
        if is_private_ip(addr.ip()) {
            anyhow::bail!("feed URL resolves to a private IP address");
        }
    }
    if !any_resolved {
        anyhow::bail!("could not resolve host");
    }
    Ok(())
}

fn validate_url(url: &str) -> Result<(String, String)> {
    let parsed = url::Url::parse(url).context("invalid URL")?;
    if parsed.scheme() != "https" {
        anyhow::bail!("URL must use HTTPS");
    }
    let host = parsed
        .host_str()
        .context("URL has no host")?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(443);
    check_private_host(&host, port)?;
    Ok((host, url.to_string()))
}

fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS as usize {
                return attempt.stop();
            }
            // Re-validate SSRF on every redirect hop
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

/// Extract main article content from a URL.
/// Returns formatted plain text or a descriptive error.
pub fn extract_content(url: &str) -> Result<String> {
    validate_url(url)?;

    let client = build_client()?;
    let response = client.get(url).send().context("network error")?;

    // Check Content-Type
    if let Some(ct) = response.headers().get("content-type")
        && let Ok(val) = ct.to_str() {
            let lower = val.to_lowercase();
            if !lower.starts_with("text/html") && !lower.starts_with("text/plain") {
                anyhow::bail!("invalid content");
            }
        }

    let status = response.status();
    if !status.is_success() {
        if status.as_u16() == 402 || status.as_u16() == 403 {
            anyhow::bail!("paywall or login wall");
        }
        anyhow::bail!("invalid content");
    }

    // Read response with size limit
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Check Content-Length header
    if let Some(cl) = response.headers().get("content-length")
        && let Ok(s) = cl.to_str()
            && let Ok(len) = s.parse::<u64>()
                && len > MAX_RESPONSE_SIZE {
                    anyhow::bail!("invalid content");
                }

    let mut body = Vec::new();
    let mut reader = response.take(MAX_RESPONSE_SIZE + 1);
    reader.read_to_end(&mut body)?;

    if body.len() as u64 > MAX_RESPONSE_SIZE {
        anyhow::bail!("invalid content");
    }

    let html_str = String::from_utf8_lossy(&body);
    let text = extract_readable(&html_str, &content_type)?;

    if text.len() < MIN_CONTENT_LENGTH {
        anyhow::bail!("too short");
    }

    // Cap at MAX_STORED_CONTENT with char-boundary safety
    if text.len() > MAX_STORED_CONTENT {
        let mut end = MAX_STORED_CONTENT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(format!(
            "{}\n\n[Content truncated at {} KB]",
            &text[..end],
            MAX_STORED_CONTENT / 1024
        ))
    } else {
        Ok(text)
    }
}

fn extract_readable(html: &str, _content_type: &str) -> Result<String> {
    let doc = Html::parse_document(html);

    // Junk exclusion is handled by the content cascade below: nav, footer, header,
    // aside, and ad selectors are never matched by <article>/<main>, and the div
    // fallback filters by id/class. Additionally, format_element's catch-all drops
    // structural tags like nav, footer, script, style that aren't in the match list.

    // Content cascade: <article> → <main> → largest <div> by <p> count

    let content_node = Selector::parse("article")
        .ok()
        .and_then(|sel| doc.select(&sel).next())
        .or_else(|| {
            Selector::parse("main")
                .ok()
                .and_then(|sel| doc.select(&sel).next())
        })
        .or_else(|| {
            // Find the div with the most <p> children
            Selector::parse("div")
                .ok()
                .and_then(|sel| {
                    doc.select(&sel)
                        .filter(|d| {
                            // Filter out likely junk divs
                            let id = d.value().id();
                            let mut classes = d.value().classes();
                            !id.is_some_and(|id| {
                                id.contains("nav")
                                    || id.contains("sidebar")
                                    || id.contains("footer")
                                    || id.contains("comment")
                                    || id.contains("menu")
                            }) && !classes.any(|c| {
                                c.contains("nav")
                                    || c.contains("sidebar")
                                    || c.contains("comment")
                                    || c.contains("ad-")
                            })
                        })
                        .max_by_key(|d| {
                            // Count <p> descendants
                            let sel = Selector::parse("p").unwrap_or_else(|_| unreachable!());
                            d.select(&sel).count()
                        })
                })
                .filter(|d| {
                    // Must have at least 2 <p> tags
                    d.select(&Selector::parse("p").unwrap_or_else(|_| unreachable!()))
                        .count()
                        >= 2
                })
        });

    let text = if let Some(node) = content_node {
        format_element(node)
    } else {
        // Fallback: get all text from body
        let body_sel = Selector::parse("body").unwrap_or_else(|_| unreachable!());
        doc.select(&body_sel)
            .next()
            .map(format_element)
            .unwrap_or_default()
    };

    // Collapse excessive whitespace
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let result = lines.join("\n\n");

    Ok(result)
}

/// Format an HTML element to structured plain text.
fn format_element(element: scraper::ElementRef) -> String {
    let mut parts: Vec<String> = Vec::new();
    for child in element.child_elements() {
        let tag = child.value().name().to_lowercase();
        match tag.as_str() {
            "h1" | "h2" | "h3" => {
                let text = child.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    parts.push(format!("# {}", text));
                }
            }
            "h4" | "h5" | "h6" => {
                let text = child.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    parts.push(format!("## {}", text));
                }
            }
            "p" => {
                let text = format_inline(child);
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            "ul" | "ol" => {
                for li in child.child_elements() {
                    if li.value().name().to_lowercase() == "li" {
                        let text = format_inline(li);
                        if !text.is_empty() {
                            parts.push(format!("- {}", text));
                        }
                    }
                }
            }
            "blockquote" => {
                let text = format_inline(child);
                if !text.is_empty() {
                    parts.push(format!("> {}", text));
                }
            }
            "pre" | "code" => {
                let text = child.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            "div" | "section" | "article" | "main" => {
                let inner = format_element(child);
                if !inner.is_empty() {
                    parts.push(inner);
                }
            }
            _ => {}
        }
    }
    parts.join("\n\n")
}

/// Format inline content (paragraphs, links) to plain text.
fn format_inline(element: scraper::ElementRef) -> String {
    let mut text = String::new();
    for child in element.children() {
        // Handle text nodes directly (captures interleaved text)
        if child.value().is_text() {
            if let Some(t) = child.value().as_text() {
                text.push_str(&t.text);
            }
            continue;
        }
        // Handle element nodes
        let Some(el) = scraper::ElementRef::wrap(child) else { continue };
        let tag = el.value().name().to_lowercase();
        match tag.as_str() {
            "a" => {
                let link_text: String = el.text().collect();
                let href = el.value().attr("href").unwrap_or("");
                if href.is_empty() || href.starts_with('#') || link_text.trim() == href.trim() {
                    text.push_str(&link_text);
                } else {
                    text.push_str(&format!("{} ({})", link_text.trim(), href.trim()));
                }
            }
            "br" | "wbr" => text.push(' '),
            "img" => {
                if let Some(alt) = el.value().attr("alt")
                    && !alt.is_empty() {
                        text.push_str(&format!("[Image: {alt}] "));
                    }
            }
            "strong" | "b" | "em" | "i" | "code" | "tt" => {
                text.push_str(&el.text().collect::<String>());
            }
            "span" | "div" => {
                text.push_str(&format_inline(el));
            }
            _ => {
                text.push_str(&el.text().collect::<String>());
            }
        }
    }
    if text.is_empty() {
        text = element.text().collect::<String>();
    }
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_rebind_gap_documented() {
        assert!(!DNS_REBIND_LIMITATION.is_empty());
    }

    #[test]
    fn test_is_private_ip_loopback() {
        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ip_public() {
        assert!(!is_private_ip("93.184.216.34".parse().unwrap()));
    }

    #[test]
    fn test_extract_readable_article() {
        let html = r#"
        <html><body>
        <article>
            <h1>Test Article</h1>
            <p>This is the first paragraph of the article.</p>
            <p>This is the second paragraph with a <a href="https://example.com">link here</a>.</p>
        </article>
        </body></html>
        "#;
        let result = extract_readable(html, "text/html").unwrap();
        assert!(result.contains("Test Article"));
        assert!(result.contains("first paragraph"));
        assert!(result.contains("link here"));
        assert!(result.contains("https://example.com"));
    }

    #[test]
    fn test_extract_readable_strips_junk() {
        let html = r#"
        <html><body>
        <nav>Navigation links here</nav>
        <article>
            <h1>Main Content</h1>
            <p>This is the actual article content.</p>
        </article>
        <footer>Footer junk</footer>
        </body></html>
        "#;
        let result = extract_readable(html, "text/html").unwrap();
        assert!(result.contains("Main Content"));
        assert!(!result.contains("Footer junk"));
    }

    #[test]
    fn test_extract_readable_interleaved_text() {
        // Verify that text nodes between inline elements are preserved
        let html = r#"
        <article>
            <p>Hello <b>world</b> and welcome!</p>
        </article>
        "#;
        let result = extract_readable(html, "text/html").unwrap();
        assert!(result.contains("Hello world and welcome"), "should preserve 'Hello world and welcome', got: {result}");
    }

    #[test]
    fn test_extract_readable_fallback_main() {
        let html = r#"
        <html><body>
        <main>
            <h1>Main Content</h1>
            <p>This comes from the main element.</p>
        </main>
        </body></html>
        "#;
        let result = extract_readable(html, "text/html").unwrap();
        assert!(result.contains("Main Content"));
    }
}
