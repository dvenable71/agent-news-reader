# agent-news-reader

RSS/Atom feed reader with a Ratatui terminal UI and a local HTTP API for AI agent consumption.

## Features

- **TUI feed reader** — Three-pane layout (feeds, headlines, article) with keyboard navigation
- **RSS/Atom parsing** — Fetches and parses any RSS 2.0 or Atom feed
- **Unread/bookmark tracking** — Toggle read status and bookmark articles
- **Background refresh daemon** — Configurable poll interval (default 15 min)
- **Article content extraction** — Extracts readable content from article URLs
- **HTTP API** — Local REST API for agent/tool integration on port 9876
- **Search & filter** — Filter by unread/bookmarked, search by title/author

## Quick Start

### Prerequisites

- Rust (edition 2024, stable toolchain)

### Build and Run

```bash
cargo build --release

# Launch the TUI
cargo run --release -- tui

# Start the HTTP API server (port 9876)
cargo run --release -- serve

# Background refresh daemon (every 15 minutes)
cargo run --release -- daemon

# Refresh feeds once
cargo run --release -- refresh

# Extract article content from URLs
cargo run --release -- extract
```

### Add Your First Feed

Press `a` in the TUI, paste a feed URL (HTTPS only), press Enter.

## TUI Keybindings

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit |
| `Tab` | Cycle focus pane (feeds / headlines / article) |
| `j` / `Down` | Navigate down |
| `k` / `Up` | Navigate up |
| `Enter` | Select article |
| `u` | Scroll article view up |
| `d` | Scroll article view down |
| `r` | Toggle read/unread |
| `b` | Toggle bookmark |
| `a` | Add a feed |
| `D` | Delete selected feed |
| `o` | Open article in browser |
| `/` | Search headlines |
| `R` | Refresh all feeds |
| `f` | Cycle filter (All / Unread / Bookmarked) |
| `m` | Toggle background daemon |
| `Esc` | Cancel input / return to normal mode |

## HTTP API

The API server binds to `127.0.0.1:9876` (localhost only). Configurable via `API_PORT` env var or `--port` flag.

### Endpoints

**`GET /health`** — Health check
```bash
curl http://127.0.0.1:9876/health
# → "ok"
```

**`GET /feeds`** — List all feeds with unread counts
```bash
curl http://127.0.0.1:9876/feeds
# → [{"id": 1, "title": "TechCrunch", "url": "https://...", "unread_count": 5}]
```

**`GET /articles`** — Query articles with filters

| Query Param | Type | Description |
|---|---|---|
| `feed_id` | int | Filter by feed |
| `unread` | bool | Only unread articles |
| `bookmarked` | bool | Only bookmarked articles |
| `since` | ISO 8601 | Articles published after this date |
| `limit` | int | Max results (clamped to 1–500, default 50) |
| `format` | `json` or `summary` | Response format (default JSON) |

```bash
# JSON format (default)
curl 'http://127.0.0.1:9876/articles?unread=true&limit=5'

# Summary format — compact tab-delimited text for agent consumption
curl 'http://127.0.0.1:9876/articles?format=summary&limit=10'
```

JSON response:
```json
{
  "articles": [
    {
      "id": 1,
      "feed_id": 1,
      "feed_title": "TechCrunch",
      "title": "AI Chip Breakthrough",
      "url": "https://...",
      "summary": "NVIDIA announces...",
      "content": "Full extracted article text...",
      "author": "John Doe",
      "published_at": "2026-05-04T10:00:00Z",
      "is_read": false,
      "is_bookmarked": true
    }
  ]
}
```

Summary format response:
```
1	AI Chip Breakthrough	2026-05-04T10:00:00Z	NVIDIA announces a breakthrough in AI chip design...
```

**`GET /articles/:id`** — Single article with full content
```bash
curl http://127.0.0.1:9876/articles/1
```

## Configuration

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `~/.local/share/agent-news-reader/news.db` | SQLite database path |
| `API_PORT` | `9876` | HTTP API server port |
| `RUST_LOG` | (none) | Tracing level: `info`, `debug`, `warn`, `error` |

Example:
```bash
DATABASE_URL=/tmp/my-feeds.db RUST_LOG=debug cargo run -- serve --port 8080
```

## Architecture

```
src/
  main.rs           CLI entry point (clap subcommands)
  app/
    mod.rs          App state, event loop, navigation
    ui.rs           Status bar and pane layout rendering
    components.rs   Feed list, headline list, article pane widgets
    keybindings.rs  Key event → Action dispatch
  api/
    mod.rs          Axum HTTP router, handlers, error types, test helpers
  db/
    mod.rs          SQLite connection, migration runner, path resolution
    models.rs       Feed and Article CRUD operations
  feed/
    mod.rs          HTTP client, feed validation, refresh logic
    extract.rs      HTML content extraction (readability-style)
  daemon.rs         Background refresh daemon (tokio)
```

### Data flow

```
Feed XML  →  feed::refresh_feed()  →  feed-rs parser  →  Article::upsert_by_guid()
                ↓
          Article URL  →  extract::extract_content()  →  scraper  →  article.content
                ↓
          TUI (app/) or HTTP API (api/)  →  db::models::Article::list_filtered()
```

## Security

- **HTTPS only** — All feed and article fetch requests enforce HTTPS. HTTP URLs are rejected.
- **SSRF protection** — Feed URLs are validated against private IP ranges (127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, link-local, loopback IPv6). Redirect chains are also checked.
- **DNS rebinding** — A documented TOCTOU gap exists: DNS resolution and TCP connect are separate operations. Fully closing this requires IP pinning via `ClientBuilder::resolve()`.
- **API binding** — The HTTP API binds to 127.0.0.1 only. No authentication is enforced. This is intentional: the API is designed for local agent/tool consumption only.
- **Response size limits** — Feed responses are capped at 10 MB. Article content is capped at 64 KB.

## Running Tests

```bash
cargo test                    # All tests (76)
cargo test -- --nocapture     # With tracing output
cargo clippy                  # Lint (should be clean — zero warnings)
```

## License

MIT
