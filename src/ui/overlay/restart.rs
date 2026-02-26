use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Restart options overlay — shown when user presses 'r' on a running session.
#[allow(dead_code)]
pub struct RestartOverlay {
    pub skip_permissions: bool,
    pub resume_conversation: bool,
    selected: usize, // 0 = skip_permissions, 1 = resume, 2 = confirm button
    submitted: bool,
    cancelled: bool,
}

impl RestartOverlay {
    pub fn new() -> Self {
        Self {
            skip_permissions: false,
            resume_conversation: true, // default on
            selected: 0,
            submitted: false,
            cancelled: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected < 2 {
                    self.selected += 1;
                }
                true
            }
            KeyCode::Char(' ') => {
                // Toggle checkbox
                match self.selected {
                    0 => self.skip_permissions = !self.skip_permissions,
                    1 => self.resume_conversation = !self.resume_conversation,
                    2 => self.submitted = true, // space on confirm = submit
                    _ => {}
                }
                true
            }
            KeyCode::Enter => {
                self.submitted = true;
                true
            }
            KeyCode::Esc => {
                self.cancelled = true;
                true
            }
            _ => true,
        }
    }

    pub fn is_submitted(&self) -> bool {
        self.submitted
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let checkbox = |checked: bool| if checked { "[x]" } else { "[ ]" };
        let highlight = |idx: usize, text: &str| {
            if idx == self.selected {
                format!(" > {}", text)
            } else {
                format!("   {}", text)
            }
        };

        let line1 = highlight(
            0,
            &format!(
                "{} --dangerously-skip-permissions",
                checkbox(self.skip_permissions)
            ),
        );
        let line2 = highlight(
            1,
            &format!(
                "{} Resume last conversation",
                checkbox(self.resume_conversation)
            ),
        );
        let confirm = if self.selected == 2 {
            " > [ Restart ]"
        } else {
            "   [ Restart ]"
        };

        let text = format!(
            "Restart session with options:\n\n{}\n{}\n\n{}\n\n↑/↓ navigate · Space toggle · Enter confirm · Esc cancel",
            line1, line2, confirm
        );

        let block = Block::default()
            .title(" ☸ Restart ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Color::White));

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restart_defaults() {
        let overlay = RestartOverlay::new();
        assert!(!overlay.skip_permissions);
        assert!(overlay.resume_conversation); // default on
        assert!(!overlay.is_submitted());
        assert!(!overlay.is_cancelled());
    }

    #[test]
    fn test_restart_toggle_permissions() {
        let mut overlay = RestartOverlay::new();
        // Selected is 0 (skip_permissions)
        overlay.handle_key(KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE));
        assert!(overlay.skip_permissions);
        overlay.handle_key(KeyEvent::new(KeyCode::Char(' '), crossterm::event::KeyModifiers::NONE));
        assert!(!overlay.skip_permissions);
    }

    #[test]
    fn test_restart_submit() {
        let mut overlay = RestartOverlay::new();
        overlay.handle_key(KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE));
        assert!(overlay.is_submitted());
    }

    #[test]
    fn test_restart_cancel() {
        let mut overlay = RestartOverlay::new();
        overlay.handle_key(KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE));
        assert!(overlay.is_cancelled());
    }
}
