pub mod keybindings;
pub mod ui;

pub struct App {
    _conn: rusqlite::Connection,
}

impl App {
    pub fn new(conn: rusqlite::Connection) -> Self {
        tracing::info!("App created");
        Self { _conn: conn }
    }
}
