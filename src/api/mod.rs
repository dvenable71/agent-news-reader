use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::task::spawn_blocking;

/// Shared application state holding the database connection.
/// Lock is acquired inside spawn_blocking to avoid blocking the async runtime.
struct AppState {
    db: Mutex<Connection>,
}

/// Query parameters for GET /articles
#[derive(Debug, Deserialize)]
pub struct ArticlesParams {
    feed_id: Option<i64>,
    unread: Option<bool>,
    bookmarked: Option<bool>,
    since: Option<String>,
    limit: Option<i64>,
    format: Option<String>,
}

/// JSON response for GET /feeds
#[derive(Debug, Serialize)]
struct FeedResponse {
    id: i64,
    title: String,
    url: String,
    site_url: Option<String>,
    unread_count: i64,
}

/// JSON response for GET /articles and GET /articles/:id
#[derive(Debug, Serialize)]
struct ArticleResponse {
    id: i64,
    feed_id: i64,
    feed_title: String,
    title: String,
    url: Option<String>,
    summary: Option<String>,
    content: Option<String>,
    author: Option<String>,
    published_at: Option<String>,
    is_read: bool,
    is_bookmarked: bool,
}

/// Envelope for article list responses
#[derive(Debug, Serialize)]
struct ArticlesResponse {
    articles: Vec<ArticleResponse>,
}

/// Consistent error type implementing axum IntoResponse
struct AppError {
    status: StatusCode,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": self.message,
            "code": self.status.as_u16(),
        });
        (self.status, axum::Json(body)).into_response()
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn list_feeds(State(state): State<Arc<AppState>>) -> Result<axum::Json<Vec<FeedResponse>>, AppError> {
    let state = Arc::clone(&state);
    let result = spawn_blocking(move || {
        let conn = state.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let feeds = crate::db::models::Feed::list_with_unread_count(&conn)
            .map_err(|e| format!("{e}"))?;
        Ok::<_, String>(feeds)
    })
    .await
    .map_err(|e| AppError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("task failed: {e}"),
    })?
    .map_err(|e: String| AppError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: e,
    })?;

    Ok(axum::Json(
        result
            .into_iter()
            .map(|(feed, unread)| FeedResponse {
                id: feed.id,
                title: feed.title,
                url: feed.url,
                site_url: feed.site_url,
                unread_count: unread,
            })
            .collect(),
    ))
}

async fn list_articles(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ArticlesParams>,
) -> Result<Response, AppError> {
    let state = Arc::clone(&state);
    let feed_id = params.feed_id;
    let filter = match (params.unread, params.bookmarked) {
        (Some(true), _) => "unread",
        (_, Some(true)) => "bookmarked",
        _ => "",
    };
    let since = params.since.clone();
    let limit = params.limit.map(|n| n.clamp(1, 500));

    // Validate since format if provided
    if let Some(ref s) = since
        && chrono::DateTime::parse_from_rfc3339(s).is_err()
        && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_err()
    {
        return Err(AppError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: format!("invalid since format: expected ISO 8601, got {s:?}"),
        });
    }

    let result = spawn_blocking(move || {
        let conn = state.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let articles = crate::db::models::Article::list_filtered(&conn, feed_id, filter, since.as_deref(), limit)
            .map_err(|e| format!("{e}"))?;

        // Resolve feed titles
        let mut response = ArticlesResponse { articles: Vec::new() };
        for article in articles {
            let feed_title = crate::db::models::Feed::get(&conn, article.feed_id)
                .ok()
                .flatten()
                .map(|f| f.title)
                .unwrap_or_default();

            response.articles.push(ArticleResponse {
                id: article.id,
                feed_id: article.feed_id,
                feed_title,
                title: article.title,
                url: article.url,
                summary: article.summary,
                content: article.content,
                author: article.author,
                published_at: article.published_at,
                is_read: article.is_read,
                is_bookmarked: article.is_bookmarked,
            });
        }
        Ok::<_, String>(response)
    })
    .await
    .map_err(|e| AppError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("task failed: {e}"),
    })?
    .map_err(|e: String| AppError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: e,
    })?;

    let is_summary = params.format.as_deref() == Some("summary");

    if is_summary {
        let mut lines = Vec::new();
        for a in &result.articles {
            let date = a.published_at.as_deref().unwrap_or("unknown");
            let excerpt = a
                .summary
                .as_deref()
                .or(a.content.as_deref())
                .map(|s| {
                    s.chars().take(120).collect::<String>().replace('\n', " ")
                })
                .unwrap_or_default();
            lines.push(format!("{}\t{}\t{}\t{}", a.id, a.title, date, excerpt));
        }
        Ok(([(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], lines.join("\n")).into_response())
    } else {
        Ok(axum::Json(result).into_response())
    }
}

async fn get_article(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<axum::Json<ArticleResponse>, AppError> {
    let state = Arc::clone(&state);
    let result = spawn_blocking(move || {
        let conn = state.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let article = crate::db::models::Article::get(&conn, id)
            .map_err(|e| format!("{e}"))?
            .ok_or_else(|| "not found".to_string())?;

        let feed_title = crate::db::models::Feed::get(&conn, article.feed_id)
            .ok()
            .flatten()
            .map(|f| f.title)
            .unwrap_or_default();

        Ok::<_, String>(ArticleResponse {
            id: article.id,
            feed_id: article.feed_id,
            feed_title,
            title: article.title,
            url: article.url,
            summary: article.summary,
            content: article.content,
            author: article.author,
            published_at: article.published_at,
            is_read: article.is_read,
            is_bookmarked: article.is_bookmarked,
        })
    })
    .await
    .map_err(|e| AppError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("task failed: {e}"),
    })?
    .map_err(|e: String| {
        if e == "not found" {
            AppError {
                status: StatusCode::NOT_FOUND,
                message: format!("article {id} not found"),
            }
        } else {
            AppError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: e,
            }
        }
    })?;

    Ok(axum::Json(result))
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler");

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = term.recv() => {},
    }
    #[cfg(not(unix))]
    ctrl_c.await.ok();
}

/// Start the HTTP API server.
///
/// **Security note**: This API is unauthenticated and trusts all localhost traffic.
/// Do not bind to a non-local address (e.g., 0.0.0.0).
pub async fn serve_api(conn: Connection, port: u16) {
    let state = Arc::new(AppState {
        db: Mutex::new(conn),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/feeds", get(list_feeds))
        .route("/articles", get(list_articles))
        .route("/articles/{id}", get(get_article))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    tracing::info!("API server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind address");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}
