use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Strip ANSI escape sequences from a string.
/// Handles CSI sequences (ESC[...m) and OSC sequences (ESC]...BEL/ST).
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    chars.next(); // consume '['
                    // Skip until we hit a letter (the terminator)
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next(); // consume ']'
                    // Skip until BEL (\x07) or ST (ESC \)
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {} // other escape, skip just the ESC
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Renders tmux pane content with scroll support.
pub struct PreviewPane {
    normal_content: Vec<String>,
    content: Vec<String>,
    scroll_offset: usize,
    is_scrolling: bool,
    width: u16,
    height: u16,
}

impl PreviewPane {
    pub fn new() -> Self {
        Self {
            normal_content: Vec::new(),
            content: Vec::new(),
            scroll_offset: 0,
            is_scrolling: false,
            width: 0,
            height: 0,
        }
    }

    /// Replace content by splitting text into lines.
    /// Strips ANSI escape sequences since ratatui renders plain text.
    /// When not scrolling, updates the displayed content immediately.
    pub fn set_content(&mut self, text: &str) {
        self.normal_content = text.lines().map(|l| strip_ansi(l)).collect();
        if !self.is_scrolling {
            self.content = self.normal_content.clone();
        }
    }

    /// Set the preview to the fallback Ganesha art (shown when no session is selected).
    pub fn set_fallback(&mut self) {
        self.set_content(crate::ui::consts::FALLBACK_TEXT);
    }

    /// Returns true when there is no content to display.
    pub fn is_empty(&self) -> bool {
        self.normal_content.is_empty()
    }

    /// Enter scroll mode with full history content.
    pub fn enter_scroll_mode(&mut self, full_history: &str) {
        self.content = full_history.lines().map(|l| strip_ansi(l)).collect();
        self.is_scrolling = true;
        self.scroll_offset = 0;
    }

    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
        self.clamp_scroll();
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        if self.scroll_offset == 0 {
            self.is_scrolling = false;
        }
    }

    pub fn reset_scroll(&mut self) {
        self.content = self.normal_content.clone();
        self.scroll_offset = 0;
        self.is_scrolling = false;
    }

    pub fn is_scrolling(&self) -> bool {
        self.is_scrolling
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Ensure scroll offset doesn't exceed content bounds.
    fn clamp_scroll(&mut self) {
        let max = self.content.len().saturating_sub(1);
        if self.scroll_offset > max {
            self.scroll_offset = max;
        }
    }
}

impl Widget for &PreviewPane {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::ALL).title("Preview");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let visible_height = if self.is_scrolling {
            // Reserve one line for scroll indicator
            inner.height.saturating_sub(1) as usize
        } else {
            inner.height as usize
        };

        // Compute the range of lines to show, working from the bottom of content.
        let total = self.content.len();
        let end = total.saturating_sub(self.scroll_offset);
        let start = end.saturating_sub(visible_height);

        let lines: Vec<Line<'_>> = self.content[start..end]
            .iter()
            .map(|l| Line::from(l.as_str()))
            .collect();

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);

        // Show scroll indicator
        if self.is_scrolling && inner.height > 0 {
            let indicator = "-- SCROLL MODE (ESC to exit) --";
            let indicator_line = Line::from(Span::styled(
                indicator,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            let indicator_area = Rect {
                x: inner.x,
                y: inner.y + inner.height - 1,
                width: inner.width,
                height: 1,
            };
            Paragraph::new(indicator_line)
                .alignment(Alignment::Center)
                .render(indicator_area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_scrolling() {
        let mut preview = PreviewPane::new();
        let content: String = (0..100)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        preview.set_content(&content);
        preview.set_size(80, 30);

        assert!(!preview.is_scrolling());
        assert_eq!(preview.scroll_offset(), 0);

        preview.enter_scroll_mode(&content);
        preview.scroll_up(5);
        assert!(preview.is_scrolling());
        assert_eq!(preview.scroll_offset(), 5);

        preview.scroll_down(3);
        assert_eq!(preview.scroll_offset(), 2);

        preview.reset_scroll();
        assert!(!preview.is_scrolling());
        assert_eq!(preview.scroll_offset(), 0);
    }

    #[test]
    fn test_preview_content_without_scrolling() {
        let mut preview = PreviewPane::new();
        preview.set_content("$ echo test\ntest");
        preview.set_size(80, 30);
        assert!(!preview.is_scrolling());
        assert_eq!(preview.content.len(), 2);
    }

    #[test]
    fn test_preview_scroll_clamp() {
        let mut preview = PreviewPane::new();
        let content = "line 0\nline 1\nline 2";
        preview.set_content(content);
        preview.set_size(80, 30);

        // Enter scroll mode then scroll beyond content should clamp
        preview.enter_scroll_mode(content);
        preview.scroll_up(1000);
        assert!(preview.is_scrolling());
        assert_eq!(preview.scroll_offset(), 2); // max is len-1
    }

    #[test]
    fn test_preview_scroll_down_to_zero_exits_scroll_mode() {
        let mut preview = PreviewPane::new();
        let content = "a\nb\nc";
        preview.set_content(content);
        preview.set_size(80, 30);

        preview.enter_scroll_mode(content);
        preview.scroll_up(2);
        assert!(preview.is_scrolling());

        preview.scroll_down(2);
        assert!(!preview.is_scrolling());
        assert_eq!(preview.scroll_offset(), 0);
    }

    #[test]
    fn test_preview_render() {
        let mut preview = PreviewPane::new();
        preview.set_content("hello\nworld");
        preview.set_size(80, 10);

        // Just verify rendering doesn't panic
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        Widget::render(&preview, area, &mut buf);
    }

    #[test]
    fn test_enter_scroll_mode_uses_full_history() {
        let mut preview = PreviewPane::new();
        preview.set_content("recent line 1\nrecent line 2");
        preview.set_size(80, 30);

        let full_history = "old line 1\nold line 2\nold line 3\nrecent line 1\nrecent line 2";
        preview.enter_scroll_mode(full_history);

        assert!(preview.is_scrolling());
        assert_eq!(preview.content.len(), 5);
        assert_eq!(preview.scroll_offset(), 0);
    }

    #[test]
    fn test_reset_scroll_restores_normal_content() {
        let mut preview = PreviewPane::new();
        preview.set_content("normal 1\nnormal 2");
        assert_eq!(preview.content.len(), 2);

        let full_history = "hist 1\nhist 2\nhist 3\nnormal 1\nnormal 2";
        preview.enter_scroll_mode(full_history);
        assert_eq!(preview.content.len(), 5);

        preview.reset_scroll();
        assert!(!preview.is_scrolling());
        assert_eq!(preview.content.len(), 2);
        assert_eq!(preview.content[0], "normal 1");
    }

    #[test]
    fn test_set_content_during_scroll_does_not_change_displayed() {
        let mut preview = PreviewPane::new();
        preview.set_content("initial");

        preview.enter_scroll_mode("full history line 1\nfull history line 2");
        assert_eq!(preview.content.len(), 2);

        // set_content during scroll should update normal_content but not displayed content
        preview.set_content("updated 1\nupdated 2\nupdated 3");
        assert_eq!(preview.content.len(), 2); // still showing full history
        assert_eq!(preview.normal_content.len(), 3);

        preview.reset_scroll();
        assert_eq!(preview.content.len(), 3); // now shows updated normal content
        assert_eq!(preview.content[0], "updated 1");
    }
}
