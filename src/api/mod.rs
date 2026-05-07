use std::collections::HashMap;

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
#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
struct ArticlesResponse {
    articles: Vec<ArticleResponse>,
}

/// Consistent error type implementing axum IntoResponse.
/// Internal details are logged via tracing and replaced with a generic message.
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn internal(msg: impl std::fmt::Display) -> Self {
        tracing::error!("internal error: {msg}");
        AppError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "internal server error".into(),
        }
    }
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

/// Resolve a map of feed_id → feed_title from the database.
/// Returns an empty map on error (already logged via tracing).
fn load_feed_titles(conn: &Connection) -> HashMap<i64, String> {
    match crate::db::models::Feed::list(conn) {
        Ok(feeds) => feeds.into_iter().map(|f| (f.id, f.title)).collect(),
        Err(e) => {
            tracing::error!("failed to load feed titles: {e}");
            HashMap::new()
        }
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn list_feeds(State(state): State<Arc<AppState>>) -> Result<axum::Json<Vec<FeedResponse>>, AppError> {
    let state = Arc::clone(&state);
    let result = spawn_blocking(move || {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        crate::db::models::Feed::list_with_unread_count(&conn).map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| AppError::internal(format!("join error: {e}")))?
    .map_err(AppError::internal)?;

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
    // Precedence: unread=true wins over bookmarked=true if both are set.
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

    let feed_id = params.feed_id;
    let state = Arc::clone(&state);
    let is_summary = params.format.as_deref() == Some("summary");

    let result = spawn_blocking(move || {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let articles = crate::db::models::Article::list_filtered(&conn, feed_id, filter, since.as_deref(), limit)
            .map_err(|e| format!("{e}"))?;

        // Batch-resolve feed titles (single query instead of N+1)
        let feed_titles = load_feed_titles(&conn);

        let response: Vec<ArticleResponse> = articles
            .into_iter()
            .map(|a| {
                let feed_title = feed_titles.get(&a.feed_id).cloned().unwrap_or_default();
                ArticleResponse {
                    id: a.id,
                    feed_id: a.feed_id,
                    feed_title,
                    title: a.title,
                    url: a.url,
                    summary: a.summary,
                    content: a.content,
                    author: a.author,
                    published_at: a.published_at,
                    is_read: a.is_read,
                    is_bookmarked: a.is_bookmarked,
                }
            })
            .collect();

        Ok::<_, String>(response)
    })
    .await
    .map_err(|e| AppError::internal(format!("join error: {e}")))?
    .map_err(AppError::internal)?;

    if is_summary {
        let mut lines = Vec::new();
        for a in &result {
            let date = a.published_at.as_deref().unwrap_or("unknown");
            let excerpt = a
                .summary
                .as_deref()
                .or(a.content.as_deref())
                .map(|s| s.chars().take(120).collect::<String>().replace('\n', " "))
                .unwrap_or_default();
            lines.push(format!("{}\t{}\t{}\t{}", a.id, a.title, date, excerpt));
        }
        Ok((
            [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            lines.join("\n"),
        )
            .into_response())
    } else {
        Ok(axum::Json(ArticlesResponse { articles: result }).into_response())
    }
}

async fn get_article(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<axum::Json<ArticleResponse>, AppError> {
    let state = Arc::clone(&state);
    let result = spawn_blocking(move || {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let article = crate::db::models::Article::get(&conn, id)
            .map_err(|e| format!("{e}"))?
            .ok_or_else(|| "not found".to_string())?;

        let feed_titles = load_feed_titles(&conn);
        let feed_title = feed_titles.get(&article.feed_id).cloned().unwrap_or_default();

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
    .map_err(|e| AppError::internal(format!("join error: {e}")))?
    .map_err(|e: String| {
        if e == "not found" {
            AppError {
                status: StatusCode::NOT_FOUND,
                message: format!("article {id} not found"),
            }
        } else {
            AppError::internal(e)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::util::ServiceExt;

    fn test_conn() -> Connection {
        crate::db::init_db(":memory:").expect("failed to create test DB")
    }

    fn populate_test_data(conn: &Connection) {
        let feed = crate::db::models::Feed::insert(conn, "Test Feed", "https://example.com/feed").unwrap();
        crate::db::models::Article::insert(
            conn,
            feed.id,
            "guid-1",
            "Article One",
            Some("https://example.com/1"),
            Some("Summary one"),
            Some("Full content one"),
            Some("Author A"),
            Some("2026-01-01T00:00:00Z"),
        )
        .unwrap();
        crate::db::models::Article::insert(
            conn,
            feed.id,
            "guid-2",
            "Article Two",
            Some("https://example.com/2"),
            Some("Summary two"),
            None,
            Some("Author B"),
            Some("2026-02-01T00:00:00Z"),
        )
        .unwrap();
    }

    fn test_app(conn: Connection) -> Router {
        let state = Arc::new(AppState {
            db: Mutex::new(conn),
        });
        Router::new()
            .route("/health", get(health))
            .route("/feeds", get(list_feeds))
            .route("/articles", get(list_articles))
            .route("/articles/{id}", get(get_article))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health() {
        let conn = test_conn();
        let app = test_app(conn);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"ok");
    }

    #[tokio::test]
    async fn test_list_feeds_empty() {
        let conn = test_conn();
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/feeds")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_list_feeds_with_data() {
        let conn = test_conn();
        populate_test_data(&conn);
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/feeds")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Vec<serde_json::Value> = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["title"], "Test Feed");
        assert_eq!(body[0]["unread_count"], 2);
    }

    #[tokio::test]
    async fn test_list_articles_json() {
        let conn = test_conn();
        populate_test_data(&conn);
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/articles")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: ArticlesResponse = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.articles.len(), 2);
        assert_eq!(body.articles[0].feed_title, "Test Feed");
    }

    #[tokio::test]
    async fn test_list_articles_summary() {
        let conn = test_conn();
        populate_test_data(&conn);
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/articles?format=summary")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("Article One"));
        assert!(text.contains("Article Two"));
        // Should be tab-delimited, one per line
        assert_eq!(text.lines().count(), 2);
    }

    #[tokio::test]
    async fn test_get_article_found() {
        let conn = test_conn();
        populate_test_data(&conn);
        // Find Article One by GUID
        let article = crate::db::models::Article::list(&conn, None)
            .unwrap()
            .into_iter()
            .find(|a| a.guid == "guid-1")
            .unwrap();
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/articles/{}", article.id))
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: ArticleResponse = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.id, article.id);
        assert_eq!(body.title, "Article One");
        assert_eq!(body.content.unwrap(), "Full content one");
    }

    #[tokio::test]
    async fn test_get_article_not_found() {
        let conn = test_conn();
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/articles/999")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_articles_unread_filter() {
        let conn = test_conn();
        populate_test_data(&conn);
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/articles?unread=true")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: ArticlesResponse = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.articles.len(), 2); // Both are unread
    }

    #[tokio::test]
    async fn test_list_articles_limit_clamping() {
        let conn = test_conn();
        populate_test_data(&conn);
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/articles?limit=1")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: ArticlesResponse = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.articles.len(), 1);
    }

    #[tokio::test]
    async fn test_list_articles_invalid_since() {
        let conn = test_conn();
        let app = test_app(conn);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/articles?since=not-a-date")
                    .method(Method::GET)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

}
