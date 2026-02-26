use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Logical key actions in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    Up,
    Down,
    Left,
    Right,
    Enter,
    New,
    Attach,
    Delete,
    Kill,
    Prompt,
    Quit,
    Help,
    Tab,
    ScrollUp,
    ScrollDown,
    ResetScroll,
    SubmitName,
    Cancel,
}

impl KeyAction {
    /// Human-readable help text for this key action.
    pub fn help_text(&self) -> &'static str {
        match self {
            KeyAction::Up => "Move up",
            KeyAction::Down => "Move down",
            KeyAction::Left => "Move left",
            KeyAction::Right => "Move right",
            KeyAction::Enter => "Select / Attach",
            KeyAction::New => "New session",
            KeyAction::Attach => "Attach to session",
            KeyAction::Delete => "Delete session",
            KeyAction::Kill => "Kill session",
            KeyAction::Prompt => "New with prompt",
            KeyAction::Quit => "Quit",
            KeyAction::Help => "Toggle help",
            KeyAction::Tab => "Switch tab",
            KeyAction::ScrollUp => "Scroll up",
            KeyAction::ScrollDown => "Scroll down",
            KeyAction::ResetScroll => "Reset scroll",
            KeyAction::SubmitName => "Submit name",
            KeyAction::Cancel => "Cancel",
        }
    }

    /// Short key label for display in menus.
    pub fn key_label(&self) -> &'static str {
        match self {
            KeyAction::Up => "k/\u{2191}",
            KeyAction::Down => "j/\u{2193}",
            KeyAction::Left => "h/\u{2190}",
            KeyAction::Right => "l/\u{2192}",
            KeyAction::Enter => "Enter",
            KeyAction::New => "n",
            KeyAction::Attach => "a",
            KeyAction::Delete => "d",
            KeyAction::Kill => "D",
            KeyAction::Prompt => "N",
            KeyAction::Quit => "q",
            KeyAction::Help => "?",
            KeyAction::Tab => "Tab",
            KeyAction::ScrollUp => "K",
            KeyAction::ScrollDown => "J",
            KeyAction::ResetScroll => "Esc",
            KeyAction::SubmitName => "Enter",
            KeyAction::Cancel => "Esc",
        }
    }
}

/// Map a key event to a logical action.
pub fn map_key(event: KeyEvent) -> Option<KeyAction> {
    match event.code {
        // Vim-style navigation
        KeyCode::Char('k') => Some(KeyAction::Up),
        KeyCode::Char('j') => Some(KeyAction::Down),
        KeyCode::Char('h') => Some(KeyAction::Left),
        KeyCode::Char('l') => Some(KeyAction::Right),

        // Arrow keys
        KeyCode::Up => Some(KeyAction::Up),
        KeyCode::Down => Some(KeyAction::Down),
        KeyCode::Left => Some(KeyAction::Left),
        KeyCode::Right => Some(KeyAction::Right),

        // Scroll (uppercase vim keys)
        KeyCode::Char('K') => Some(KeyAction::ScrollUp),
        KeyCode::Char('J') => Some(KeyAction::ScrollDown),

        // Actions
        KeyCode::Enter => Some(KeyAction::Enter),
        KeyCode::Char('n') => Some(KeyAction::New),
        KeyCode::Char('a') => Some(KeyAction::Attach),
        KeyCode::Char('d') => Some(KeyAction::Delete),
        KeyCode::Char('D') => Some(KeyAction::Kill),
        KeyCode::Char('N') => Some(KeyAction::Prompt),
        KeyCode::Char('q') => Some(KeyAction::Quit),
        KeyCode::Char('?') => Some(KeyAction::Help),
        KeyCode::Tab => Some(KeyAction::Tab),
        KeyCode::Esc => Some(KeyAction::Cancel),

        // Ctrl+C as quit
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(KeyAction::Quit)
        }

        _ => None,
    }
}
