use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct ConfirmationOverlay {
    message: String,
    dismissed: bool,
    confirmed: bool,
}

#[allow(dead_code)]
impl ConfirmationOverlay {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            dismissed: false,
            confirmed: false,
        }
    }

    /// Handle a key press. Returns true if the overlay consumed the key.
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.confirmed = true;
                self.dismissed = true;
                true
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirmed = false;
                self.dismissed = true;
                true
            }
            _ => false,
        }
    }

    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }

    pub fn is_confirmed(&self) -> bool {
        self.confirmed
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Render the overlay content (without centering — that's done by the caller).
    pub fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm ");
        let inner = block.inner(area);
        block.render(area, buf);

        let text = Paragraph::new(vec![
            Line::from(self.message.as_str()),
            Line::from(""),
            Line::from(vec![
                Span::styled("[y]", Style::default().fg(Color::Green).bold()),
                Span::raw(" Confirm  "),
                Span::styled("[n/Esc]", Style::default().fg(Color::Red).bold()),
                Span::raw(" Cancel"),
            ]),
        ])
        .alignment(Alignment::Center);
        text.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirmation_y_confirms() {
        let mut overlay = ConfirmationOverlay::new("Delete session?");
        assert!(!overlay.is_dismissed());

        let consumed = overlay.handle_key(KeyCode::Char('y'));
        assert!(consumed);
        assert!(overlay.is_dismissed());
        assert!(overlay.is_confirmed());
    }

    #[test]
    fn test_confirmation_n_cancels() {
        let mut overlay = ConfirmationOverlay::new("Delete?");
        overlay.handle_key(KeyCode::Char('n'));
        assert!(overlay.is_dismissed());
        assert!(!overlay.is_confirmed());
    }

    #[test]
    fn test_confirmation_esc_cancels() {
        let mut overlay = ConfirmationOverlay::new("Delete?");
        overlay.handle_key(KeyCode::Esc);
        assert!(overlay.is_dismissed());
        assert!(!overlay.is_confirmed());
    }

    #[test]
    fn test_confirmation_other_keys_ignored() {
        let mut overlay = ConfirmationOverlay::new("Delete?");
        let consumed = overlay.handle_key(KeyCode::Char('x'));
        assert!(!consumed);
        assert!(!overlay.is_dismissed());
    }

    #[test]
    fn test_confirmation_message_formatting() {
        let cases = vec![
            ("my-feature", "[!] Kill session 'my-feature'? (y/n)"),
            (
                "feature with spaces",
                "[!] Kill session 'feature with spaces'? (y/n)",
            ),
            (
                "feature/branch-123",
                "[!] Kill session 'feature/branch-123'? (y/n)",
            ),
        ];
        for (name, expected) in cases {
            let msg = format!("[!] Kill session '{}'? (y/n)", name);
            assert_eq!(msg, expected);
        }
    }

    #[test]
    fn test_multiple_confirmations_independent() {
        let mut overlay1 = ConfirmationOverlay::new("First?");
        overlay1.handle_key(KeyCode::Char('n'));
        assert!(!overlay1.is_confirmed());

        let mut overlay2 = ConfirmationOverlay::new("Second?");
        overlay2.handle_key(KeyCode::Char('y'));
        assert!(overlay2.is_confirmed());

        // First overlay state unchanged
        assert!(!overlay1.is_confirmed());
    }

    #[test]
    fn test_confirmation_render_contains_elements() {
        let overlay = ConfirmationOverlay::new("[!] Delete session? (y/n)");
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        overlay.render_content(area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("Confirm"), "should contain confirm text");
    }

    fn buffer_to_string(buf: &Buffer) -> String {
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
            }
            s.push('\n');
        }
        s
    }
}
