use axum::{Router, routing::get};
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

async fn health() -> &'static str {
    "ok"
}

pub async fn serve_api(conn: Connection, port: u16) {
    let _state = Arc::new(Mutex::new(conn));

    let app = Router::new().route("/health", get(health));

    let addr = format!("127.0.0.1:{port}");
    tracing::info!("API server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind address");

    axum::serve(listener, app).await.expect("server error");
}
