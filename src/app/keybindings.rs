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
        let e = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::Quit);
    }

    #[test]
    fn test_dispatch_ctrl_c() {
        let e = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(dispatch(&e), Action::Quit);
    }

    #[test]
    fn test_dispatch_tab() {
        let e = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::CyclePane);
    }

    #[test]
    fn test_dispatch_j() {
        let e = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::Down);
    }

    #[test]
    fn test_dispatch_k() {
        let e = Event::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::Up);
    }

    #[test]
    fn test_dispatch_enter() {
        let e = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::Select);
    }

    #[test]
    fn test_dispatch_toggle_bookmark() {
        let e = Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::ToggleBookmark);
    }

    #[test]
    fn test_dispatch_scroll() {
        let u = Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
        let d = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(dispatch(&u), Action::ScrollUp);
        assert_eq!(dispatch(&d), Action::ScrollDown);
    }

    #[test]
    fn test_dispatch_unknown() {
        let e = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(dispatch(&e), Action::None);
    }
}
