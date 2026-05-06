use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::feed;

const REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60); // 15 minutes

pub async fn run_daemon(conn: Connection) {
    tracing::info!("daemon started, refresh interval: 15 minutes");
    let conn = Arc::new(Mutex::new(conn));

    loop {
        {
            let conn = conn.clone();
            if let Err(e) = tokio::task::spawn_blocking(move || {
                let conn = conn.blocking_lock();
                feed::refresh_feeds(&conn, None);
            })
            .await
            {
                tracing::error!("feed refresh task panicked: {e}");
            }
        }
        tokio::time::sleep(REFRESH_INTERVAL).await;
    }
}
