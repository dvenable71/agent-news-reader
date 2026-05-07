use std::time::Duration;

use anyhow::Result;

use crate::feed;

/// Run the background refresh daemon.
/// Opens its own DB connection each cycle. Exits cleanly on SIGINT.
pub async fn run_daemon(db_path: String, interval: Duration) -> Result<()> {
    tracing::info!(interval_secs = interval.as_secs(), "daemon started");
    run_refresh_cycle(&db_path).await;

    loop {
        let sleep = tokio::time::sleep(interval);
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received SIGINT, shutting down daemon");
                break Ok(());
            }
            _ = sleep => {
                run_refresh_cycle(&db_path).await;
            }
        }
    }
}

async fn run_refresh_cycle(db_path: &str) {
    let start = std::time::Instant::now();

    let conn = match crate::db::init_db(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "daemon: failed to open database");
            return;
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        feed::refresh_feeds(&conn, None);
        conn
    })
    .await;

    match result {
        Ok(_conn) => {
            // Connection dropped here
            tracing::info!(
                duration_ms = start.elapsed().as_millis(),
                "daemon: refresh cycle complete"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "daemon: refresh cycle panicked");
        }
    }
}
