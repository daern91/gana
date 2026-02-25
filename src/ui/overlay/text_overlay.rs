use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::*;

#[allow(dead_code)]
pub struct TextOverlay {
    title: String,
    content: String,
    dismissed: bool,
}

#[allow(dead_code)]
impl TextOverlay {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            dismissed: false,
        }
    }

    /// Handle a key press. Returns true if the overlay consumed the key.
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.dismissed = true;
                true
            }
            _ => false,
        }
    }

    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }

    /// Render the overlay content (without centering — that's done by the caller).
    pub fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" {} ", self.title));
        let inner = block.inner(area);
        block.render(area, buf);

        // Split inner area for content and footer
        let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);

        // Content text
        let content = Paragraph::new(self.content.as_str()).wrap(Wrap { trim: false });
        content.render(layout[0], buf);

        // Footer
        let footer = Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Yellow).bold()),
            Span::styled(" to close", Style::default().fg(Color::DarkGray)),
        ]);
        let footer_paragraph = Paragraph::new(footer).alignment(Alignment::Center);
        footer_paragraph.render(layout[1], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_overlay_dismiss_esc() {
        let mut overlay = TextOverlay::new("Help", "Some help text");
        assert!(!overlay.is_dismissed());
        overlay.handle_key(KeyCode::Esc);
        assert!(overlay.is_dismissed());
    }

    #[test]
    fn test_text_overlay_dismiss_q() {
        let mut overlay = TextOverlay::new("Help", "Some help text");
        overlay.handle_key(KeyCode::Char('q'));
        assert!(overlay.is_dismissed());
    }

    #[test]
    fn test_text_overlay_dismiss_enter() {
        let mut overlay = TextOverlay::new("Help", "Some help text");
        overlay.handle_key(KeyCode::Enter);
        assert!(overlay.is_dismissed());
    }

    #[test]
    fn test_text_overlay_other_keys_ignored() {
        let mut overlay = TextOverlay::new("Help", "Some help text");
        let consumed = overlay.handle_key(KeyCode::Char('x'));
        assert!(!consumed);
        assert!(!overlay.is_dismissed());
    }
}
