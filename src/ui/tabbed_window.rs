use ratatui::prelude::*;
use ratatui::widgets::Tabs;

/// The active tab in the right-hand pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Preview,
    Diff,
}

/// Manages tab state and renders a tab bar for switching between Preview and Diff.
pub struct TabbedWindow {
    active_tab: Tab,
}

impl TabbedWindow {
    pub fn new() -> Self {
        Self {
            active_tab: Tab::Preview,
        }
    }

    pub fn active_tab(&self) -> Tab {
        self.active_tab
    }

    pub fn switch_tab(&mut self) {
        self.active_tab = match self.active_tab {
            Tab::Preview => Tab::Diff,
            Tab::Diff => Tab::Preview,
        };
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.active_tab = tab;
    }
}

impl Widget for &TabbedWindow {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let titles = vec!["Preview", "Diff"];
        let selected = match self.active_tab {
            Tab::Preview => 0,
            Tab::Diff => 1,
        };

        let tabs = Tabs::new(titles)
            .select(selected)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .divider("|");

        Widget::render(tabs, area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tabbed_window_default() {
        let tw = TabbedWindow::new();
        assert_eq!(tw.active_tab(), Tab::Preview);
    }

    #[test]
    fn test_tabbed_window_switch() {
        let mut tw = TabbedWindow::new();
        assert_eq!(tw.active_tab(), Tab::Preview);

        tw.switch_tab();
        assert_eq!(tw.active_tab(), Tab::Diff);

        tw.switch_tab();
        assert_eq!(tw.active_tab(), Tab::Preview);
    }

    #[test]
    fn test_tabbed_window_set_tab() {
        let mut tw = TabbedWindow::new();
        tw.set_tab(Tab::Diff);
        assert_eq!(tw.active_tab(), Tab::Diff);

        tw.set_tab(Tab::Preview);
        assert_eq!(tw.active_tab(), Tab::Preview);
    }

    #[test]
    fn test_tabbed_window_render() {
        let tw = TabbedWindow::new();
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        Widget::render(&tw, area, &mut buf);

        let content: String = (0..40)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(content.contains("Preview"));
        assert!(content.contains("Diff"));
    }
}
