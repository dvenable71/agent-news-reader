use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    CyclePane,
    Up,
    Down,
    Select,
    ScrollUp,
    ScrollDown,
    ToggleBookmark,
    ToggleRead,
    AddFeed,
    DeleteFeed,
    OpenInBrowser,
    Search,
    Escape,
    Refresh,
    CycleFilter,
    None,
}

pub fn dispatch(event: &Event) -> Action {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        }) => Action::Quit,
        Event::Key(KeyEvent {
            code: KeyCode::Tab, ..
        }) => Action::CyclePane,
        Event::Key(KeyEvent {
            code: KeyCode::Char('j'), ..
        })
        | Event::Key(KeyEvent {
            code: KeyCode::Down, ..
        }) => Action::Down,
        Event::Key(KeyEvent {
            code: KeyCode::Char('k'), ..
        })
        | Event::Key(KeyEvent {
            code: KeyCode::Up, ..
        }) => Action::Up,
        Event::Key(KeyEvent {
            code: KeyCode::Enter, ..
        }) => Action::Select,
        Event::Key(KeyEvent {
            code: KeyCode::Char('u'), ..
        }) => Action::ScrollUp,
        Event::Key(KeyEvent {
            code: KeyCode::Char('d'), ..
        }) => Action::ScrollDown,
        Event::Key(KeyEvent {
            code: KeyCode::Char('b'), ..
        }) => Action::ToggleBookmark,
        Event::Key(KeyEvent {
            code: KeyCode::Char('r'), ..
        }) => Action::ToggleRead,
        Event::Key(KeyEvent {
            code: KeyCode::Char('a'), ..
        }) => Action::AddFeed,
        Event::Key(KeyEvent {
            code: KeyCode::Char('D'), ..
        }) => Action::DeleteFeed,
        Event::Key(KeyEvent {
            code: KeyCode::Char('o'), ..
        }) => Action::OpenInBrowser,
        Event::Key(KeyEvent {
            code: KeyCode::Char('/'), ..
        }) => Action::Search,
        Event::Key(KeyEvent {
            code: KeyCode::Esc, ..
        }) => Action::Escape,
        Event::Key(KeyEvent {
            code: KeyCode::Char('R'), ..
        }) => Action::Refresh,
        Event::Key(KeyEvent {
            code: KeyCode::Char('f'), ..
        }) => Action::CycleFilter,
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }) => Action::Quit,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_dispatch_quit() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))), Action::Quit);
    }

    #[test]
    fn test_dispatch_ctrl_c() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))), Action::Quit);
    }

    #[test]
    fn test_dispatch_tab() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))), Action::CyclePane);
    }

    #[test]
    fn test_dispatch_j() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))), Action::Down);
    }

    #[test]
    fn test_dispatch_k() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))), Action::Up);
    }

    #[test]
    fn test_dispatch_enter() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))), Action::Select);
    }

    #[test]
    fn test_dispatch_toggle_bookmark() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE))), Action::ToggleBookmark);
    }

    #[test]
    fn test_dispatch_toggle_read() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))), Action::ToggleRead);
    }

    #[test]
    fn test_dispatch_add_feed() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))), Action::AddFeed);
    }

    #[test]
    fn test_dispatch_delete_feed() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::NONE))), Action::DeleteFeed);
    }

    #[test]
    fn test_dispatch_open_in_browser() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE))), Action::OpenInBrowser);
    }

    #[test]
    fn test_dispatch_search() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE))), Action::Search);
    }

    #[test]
    fn test_dispatch_escape() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))), Action::Escape);
    }

    #[test]
    fn test_dispatch_refresh() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE))), Action::Refresh);
    }

    #[test]
    fn test_dispatch_cycle_filter() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE))), Action::CycleFilter);
    }

    #[test]
    fn test_dispatch_unknown() {
        assert_eq!(dispatch(&Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))), Action::None);
    }
}
