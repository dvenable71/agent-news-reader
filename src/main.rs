#![allow(dead_code)]

mod api;
mod app;
mod daemon;
mod db;
mod feed;

use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "agent-news-reader",
    about = "RSS Reader with TUI and Agent HTTP API"
)]
struct Cli {
    /// Database path (overrides DATABASE_URL env var)
    #[arg(long, env = "DATABASE_URL")]
    db_path: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Parser)]
enum Command {
    /// Launch the TUI
    Tui,
    /// Start the HTTP API server
    Serve {
        /// Port to listen on
        #[arg(long, env = "API_PORT", default_value = "3000")]
        port: u16,
    },
    /// Run as a background daemon
    Daemon {
        /// Poll interval in minutes (default: 15)
        #[arg(long, default_value = "15")]
        interval_minutes: u64,
    },
    /// Refresh feeds (all, or a specific feed by ID)
    Refresh {
        /// Only refresh this feed ID
        #[arg(long)]
        feed_id: Option<i64>,
    },
    /// Extract article content from URLs
    Extract {
        /// Only extract articles from this feed
        #[arg(long)]
        feed_id: Option<i64>,
        /// Maximum articles to extract (default 50)
        #[arg(long, default_value = "50")]
        limit: i64,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let db_path = cli
        .db_path
        .unwrap_or_else(db::get_db_path)
        .to_string_lossy()
        .to_string();

    let conn = db::init_db(&db_path).expect("failed to initialize database");

    match &cli.command {
        Command::Tui => {
            #[cfg(not(test))]
            {
                let app = app::App::new(conn);
                app.run_tui();
            }
            #[cfg(test)]
            {
                drop(conn);
            }
        }
        Command::Serve { port } => {
            tracing::info!("starting API server on port {port}");
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(api::serve_api(conn, *port));
        }
        Command::Daemon { interval_minutes } => {
            let interval = std::time::Duration::from_secs((*interval_minutes).max(5) * 60);
            tracing::info!(?interval_minutes, "starting daemon");
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            if let Err(e) = rt.block_on(daemon::run_daemon(db_path.clone(), interval)) {
                tracing::error!("daemon exited with error: {e}");
            }
        }
        Command::Refresh { feed_id } => {
            tracing::info!("refreshing feeds (feed_id: {feed_id:?})");
            feed::refresh_feeds(&conn, *feed_id);
        }
        Command::Extract { feed_id, limit } => {
            tracing::info!(?feed_id, limit, "extracting article content");
            match feed::extract_all(&conn, *feed_id, *limit) {
                Ok(count) => tracing::info!("extracted content for {count} articles"),
                Err(e) => tracing::error!("extraction failed: {e}"),
            }
        }
    }
}
