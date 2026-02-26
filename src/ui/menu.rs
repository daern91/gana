use std::time::Instant;

use ratatui::prelude::*;

/// Shows available key bindings at the bottom of the screen.
pub struct MenuBar {
    highlighted_key: Option<(String, Instant)>,
}

impl MenuBar {
    pub fn new() -> Self {
        Self {
            highlighted_key: None,
        }
    }

    /// Highlight a key for a brief flash (500ms).
    pub fn highlight_key(&mut self, key: &str) {
        self.highlighted_key = Some((key.to_string(), Instant::now()));
    }
}

/// Key binding entries displayed in the menu bar.
const MENU_ITEMS: &[(&str, &str)] = &[
    ("n", "New"),
    ("N", "Prompt"),
    ("a", "Attach"),
    ("d", "Delete"),
    ("D", "Kill"),
    ("p", "Pause"),
    ("P", "Push"),
    ("r", "Restart"),
    ("q", "Quit"),
    ("?", "Help"),
    ("Tab", "Switch"),
];

impl Widget for &MenuBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let highlight_key = self.highlighted_key.as_ref().and_then(|(k, t)| {
            if t.elapsed() < std::time::Duration::from_millis(500) {
                Some(k.as_str())
            } else {
                None
            }
        });

        let mut spans: Vec<Span<'_>> = Vec::new();

        for (i, (key, desc)) in MENU_ITEMS.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let key_style = if highlight_key == Some(*key) {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(*key, key_style));
            spans.push(Span::raw(":"));
            spans.push(Span::styled(
                *desc,
                Style::default().add_modifier(Modifier::DIM),
            ));
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_bar_renders() {
        let menu = MenuBar::new();
        let area = Rect::new(0, 0, 120, 1);
        let mut buf = Buffer::empty(area);
        Widget::render(&menu, area, &mut buf);

        // Verify the buffer contains key labels
        let content: String = (0..120)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(content.contains("n:New"));
        assert!(content.contains("q:Quit"));
        assert!(content.contains("Tab:Switch"));
    }

    #[test]
    fn test_menu_bar_zero_area() {
        let menu = MenuBar::new();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        // Should not panic
        Widget::render(&menu, area, &mut buf);
    }

    #[test]
    fn test_menu_bar_highlight_key() {
        let mut menu = MenuBar::new();
        assert!(menu.highlighted_key.is_none());

        menu.highlight_key("n");
        assert!(menu.highlighted_key.is_some());
        let (key, instant) = menu.highlighted_key.as_ref().unwrap();
        assert_eq!(key, "n");
        assert!(instant.elapsed() < std::time::Duration::from_millis(500));
    }

    #[test]
    fn test_menu_bar_highlight_renders_differently() {
        // Render without highlight
        let menu_normal = MenuBar::new();
        let area = Rect::new(0, 0, 80, 1);
        let mut buf_normal = Buffer::empty(area);
        Widget::render(&menu_normal, area, &mut buf_normal);

        // Render with highlight on "n"
        let mut menu_highlighted = MenuBar::new();
        menu_highlighted.highlight_key("n");
        let mut buf_highlighted = Buffer::empty(area);
        Widget::render(&menu_highlighted, area, &mut buf_highlighted);

        // The "n" cell should have different styling
        let cell_normal = buf_normal.cell((0, 0)).unwrap();
        let cell_highlighted = buf_highlighted.cell((0, 0)).unwrap();
        // Both should contain "n"
        assert_eq!(cell_normal.symbol(), "n");
        assert_eq!(cell_highlighted.symbol(), "n");
        // Highlighted should have yellow foreground
        assert_eq!(cell_highlighted.fg, Color::Yellow);
        // Normal should not have yellow foreground
        assert_ne!(cell_normal.fg, Color::Yellow);
    }

    #[test]
    fn test_menu_bar_highlight_expires() {
        use std::time::{Duration, Instant};

        let mut menu = MenuBar::new();
        // Set a highlight that already expired
        menu.highlighted_key = Some(("n".to_string(), Instant::now() - Duration::from_secs(1)));

        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        Widget::render(&menu, area, &mut buf);

        // Expired highlight should render as normal (no yellow)
        let cell = buf.cell((0, 0)).unwrap();
        assert_eq!(cell.symbol(), "n");
        assert_ne!(cell.fg, Color::Yellow);
    }
}
