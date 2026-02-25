use ratatui::prelude::*;

/// Shows available key bindings at the bottom of the screen.
pub struct MenuBar;

impl MenuBar {
    pub fn new() -> Self {
        Self
    }
}

/// Key binding entries displayed in the menu bar.
const MENU_ITEMS: &[(&str, &str)] = &[
    ("n", "New"),
    ("a", "Attach"),
    ("d", "Delete"),
    ("D", "Kill"),
    ("q", "Quit"),
    ("?", "Help"),
    ("Tab", "Switch"),
];

impl Widget for &MenuBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let mut spans: Vec<Span<'_>> = Vec::new();

        for (i, (key, desc)) in MENU_ITEMS.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                *key,
                Style::default().add_modifier(Modifier::BOLD),
            ));
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
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        Widget::render(&menu, area, &mut buf);

        // Verify the buffer contains key labels
        let content: String = (0..80)
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
}
