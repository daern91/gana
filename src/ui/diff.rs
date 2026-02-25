use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::session::git::diff::DiffStats;

/// Renders colored git diff output.
pub struct DiffView {
    content: String,
    added: usize,
    removed: usize,
}

impl DiffView {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            added: 0,
            removed: 0,
        }
    }

    /// Update the diff from a `DiffStats` value.
    pub fn set_diff(&mut self, stats: &DiffStats) {
        self.content = stats.content.clone();
        self.added = stats.added_lines;
        self.removed = stats.removed_lines;
    }

    /// Summary string like "+15 -3".
    pub fn summary(&self) -> String {
        format!("+{} -{}", self.added, self.removed)
    }
}

impl Widget for &DiffView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::ALL).title("Diff");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let lines: Vec<Line<'_>> = self
            .content
            .lines()
            .map(|line| {
                let style = classify_diff_line(line);
                Line::from(Span::styled(line, style))
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

/// Determine the style for a diff line based on its prefix.
fn classify_diff_line(line: &str) -> Style {
    if line.starts_with("+++") || line.starts_with("---") || line.starts_with("diff") || line.starts_with("index") {
        Style::default().fg(Color::DarkGray)
    } else if line.starts_with('+') {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_view_summary() {
        let mut view = DiffView::new();
        let stats = DiffStats::from_diff("+a\n+b\n+c\n-x\n".to_string());
        view.set_diff(&stats);
        assert_eq!(view.summary(), "+3 -1");
    }

    #[test]
    fn test_diff_view_empty_summary() {
        let view = DiffView::new();
        assert_eq!(view.summary(), "+0 -0");
    }

    #[test]
    fn test_diff_coloring() {
        // Added lines
        let style = classify_diff_line("+added");
        assert_eq!(style.fg, Some(Color::Green));

        // Removed lines
        let style = classify_diff_line("-removed");
        assert_eq!(style.fg, Some(Color::Red));

        // Hunk header
        let style = classify_diff_line("@@ -1,3 +1,4 @@");
        assert_eq!(style.fg, Some(Color::Cyan));

        // Header lines
        let style = classify_diff_line("diff --git a/file b/file");
        assert_eq!(style.fg, Some(Color::DarkGray));

        let style = classify_diff_line("index abc123..def456");
        assert_eq!(style.fg, Some(Color::DarkGray));

        let style = classify_diff_line("--- a/file");
        assert_eq!(style.fg, Some(Color::DarkGray));

        let style = classify_diff_line("+++ b/file");
        assert_eq!(style.fg, Some(Color::DarkGray));

        // Context line (no prefix)
        let style = classify_diff_line(" unchanged line");
        assert_eq!(style.fg, None);
    }

    #[test]
    fn test_diff_render() {
        let mut view = DiffView::new();
        let diff = "+added\n-removed\n context\n";
        let stats = DiffStats::from_diff(diff.to_string());
        view.set_diff(&stats);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        Widget::render(&view, area, &mut buf);
    }
}
