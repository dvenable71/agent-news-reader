pub mod extract;

use rusqlite::Connection;

pub fn refresh_feeds(_conn: &Connection) {
    tracing::info!("refresh_feeds stub");
}
