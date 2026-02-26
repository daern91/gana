use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

#[allow(dead_code)]
pub struct TextInputOverlay {
    title: String,
    input: String,
    cursor_pos: usize,
    submitted: bool,
    cancelled: bool,
}

#[allow(dead_code)]
impl TextInputOverlay {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            input: String::new(),
            cursor_pos: 0,
            submitted: false,
            cancelled: false,
        }
    }

    /// Handle a key event. Returns true if the overlay consumed the key.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                self.submitted = true;
                true
            }
            KeyCode::Esc => {
                self.cancelled = true;
                true
            }
            KeyCode::Char(c) => {
                if self.input.len() < 64 {
                    self.input.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                }
                true
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.input.remove(self.cursor_pos);
                }
                true
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                true
            }
            KeyCode::Right => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                }
                true
            }
            _ => false,
        }
    }

    pub fn is_submitted(&self) -> bool {
        self.submitted
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn is_done(&self) -> bool {
        self.submitted || self.cancelled
    }

    /// Render the overlay content (without centering — that's done by the caller).
    pub fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" {} ", self.title));
        let inner = block.inner(area);
        block.render(area, buf);

        // Build the input display with a cursor indicator
        let before_cursor = &self.input[..self.cursor_pos];
        let cursor_char = self.input.get(self.cursor_pos..self.cursor_pos + 1).unwrap_or(" ");
        let after_cursor = if self.cursor_pos < self.input.len() {
            &self.input[self.cursor_pos + 1..]
        } else {
            ""
        };

        let input_line = Line::from(vec![
            Span::raw(before_cursor),
            Span::styled(
                cursor_char,
                Style::default().bg(Color::White).fg(Color::Black),
            ),
            Span::raw(after_cursor),
        ]);

        let counter = format!("({}/32)", self.input.len());
        let text = Paragraph::new(vec![
            input_line,
            Line::from(Span::styled(
                counter,
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(vec![
                Span::styled("[Enter]", Style::default().fg(Color::Green).bold()),
                Span::raw(" Submit  "),
                Span::styled("[Esc]", Style::default().fg(Color::Red).bold()),
                Span::raw(" Cancel"),
            ]),
        ]);
        text.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn test_text_input_typing() {
        let mut input = TextInputOverlay::new("Session name");
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(input.input(), "hi");
    }

    #[test]
    fn test_text_input_backspace() {
        let mut input = TextInputOverlay::new("Name");
        input.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(input.input(), "a");
    }

    #[test]
    fn test_text_input_submit() {
        let mut input = TextInputOverlay::new("Name");
        input.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(input.is_submitted());
        assert_eq!(input.input(), "x");
    }

    #[test]
    fn test_text_input_cancel() {
        let mut input = TextInputOverlay::new("Name");
        input.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(input.is_cancelled());
    }

    #[test]
    fn test_text_input_cursor_movement() {
        let mut input = TextInputOverlay::new("Name");
        input.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        // Move left twice
        input.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        // Insert 'x' at position 1
        input.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(input.input(), "axbc");
    }

    #[test]
    fn test_text_input_is_done() {
        let mut input = TextInputOverlay::new("Name");
        assert!(!input.is_done());
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(input.is_done());
    }
}
