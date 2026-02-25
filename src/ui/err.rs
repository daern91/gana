use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Displays an error message in a bordered, red-styled block.
pub struct ErrorDisplay {
    message: Option<String>,
}

impl ErrorDisplay {
    pub fn new() -> Self {
        Self { message: None }
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
    }

    pub fn clear(&mut self) {
        self.message = None;
    }

    pub fn has_error(&self) -> bool {
        self.message.is_some()
    }
}

impl Widget for &ErrorDisplay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let msg = match &self.message {
            Some(m) => m,
            None => return,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Error")
            .border_style(Style::default().fg(Color::Red));

        let text = Line::from(Span::styled(
            format!("Error: {}", msg),
            Style::default().fg(Color::Red),
        ));

        let paragraph = Paragraph::new(text).block(block);
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_initial() {
        let err = ErrorDisplay::new();
        assert!(!err.has_error());
    }

    #[test]
    fn test_error_display_set_and_clear() {
        let mut err = ErrorDisplay::new();

        err.set_error("something went wrong");
        assert!(err.has_error());

        err.clear();
        assert!(!err.has_error());
    }

    #[test]
    fn test_error_display_render_with_error() {
        let mut err = ErrorDisplay::new();
        err.set_error("test error");

        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        Widget::render(&err, area, &mut buf);

        // Check that the buffer contains our error text
        let mut content = String::new();
        for y in 0..3 {
            for x in 0..40 {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(content.contains("Error: test error"));
    }

    #[test]
    fn test_error_display_render_without_error() {
        let err = ErrorDisplay::new();
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        Widget::render(&err, area, &mut buf);

        // Should be empty — no rendering when no error
        let mut content = String::new();
        for y in 0..3 {
            for x in 0..40 {
                content.push_str(buf.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(!content.contains("Error:"));
    }
}
