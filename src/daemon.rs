use rusqlite::Connection;

pub async fn run_daemon(_conn: Connection) {
    tracing::info!("run_daemon stub");
}
