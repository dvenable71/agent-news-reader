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
    Daemon,
    /// Refresh feeds (all, or a specific feed by ID)
    Refresh {
        /// Only refresh this feed ID
        #[arg(long)]
        feed_id: Option<i64>,
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
            tracing::info!("starting TUI");
            let _app = app::App::new(conn);
            tracing::info!("TUI stub completed");
        }
        Command::Serve { port } => {
            tracing::info!("starting API server on port {port}");
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(api::serve_api(conn, *port));
        }
        Command::Daemon => {
            tracing::info!("starting daemon");
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(daemon::run_daemon(conn));
        }
        Command::Refresh { feed_id } => {
            tracing::info!("refreshing feeds (feed_id: {feed_id:?})");
            feed::refresh_feeds(&conn, *feed_id);
        }
    }
}
