use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget};

use crate::session::instance::{Instance, InstanceStatus};

/// A selectable list pane displaying session instances with status indicators.
pub struct ListPane {
    selected: usize,
    items: Vec<ListItem<'static>>,
}

impl ListPane {
    pub fn new() -> Self {
        Self {
            selected: 0,
            items: Vec::new(),
        }
    }

    /// Rebuild the rendered list items from a slice of instances.
    pub fn set_items(&mut self, instances: &[Instance]) {
        self.items = instances.iter().map(|inst| render_instance(inst)).collect();
        // Clamp selection
        if !self.items.is_empty() && self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
    }

    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    pub fn select_previous(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, idx: usize) {
        if !self.items.is_empty() {
            self.selected = idx.min(self.items.len() - 1);
        }
    }

    pub fn num_items(&self) -> usize {
        self.items.len()
    }

    /// Create a `ListState` pointing at the current selection.
    fn list_state(&self) -> ListState {
        let mut state = ListState::default();
        if !self.items.is_empty() {
            state.select(Some(self.selected));
        }
        state
    }
}

impl StatefulWidget for &ListPane {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let list = List::new(self.items.clone())
            .block(Block::default().borders(Borders::ALL).title("Sessions"))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ");

        StatefulWidget::render(list, area, buf, state);
    }
}

impl Widget for &ListPane {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut state = self.list_state();
        StatefulWidget::render(self, area, buf, &mut state);
    }
}

/// Build a styled `ListItem` from an `Instance`.
fn render_instance(inst: &Instance) -> ListItem<'static> {
    let (icon, icon_style) = match inst.status {
        InstanceStatus::Running => ("●", Style::default().fg(Color::Green)),
        InstanceStatus::Ready => ("○", Style::default()),
        InstanceStatus::Loading => ("◌", Style::default().fg(Color::Yellow)),
        InstanceStatus::Paused => ("⏸", Style::default().add_modifier(Modifier::DIM)),
    };

    let mut spans = vec![
        Span::styled(icon.to_string(), icon_style),
        Span::raw(" "),
        Span::raw(inst.title.clone()),
    ];

    if !inst.branch.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("[{}]", inst.branch),
            Style::default().fg(Color::Cyan),
        ));
    }

    ListItem::new(Line::from(spans))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::instance::InstanceOptions;

    fn make_instance(title: &str, status: InstanceStatus, branch: &str) -> Instance {
        let mut inst = Instance::new(InstanceOptions {
            title: title.to_string(),
            path: "/tmp".to_string(),
            program: "bash".to_string(),
            auto_yes: false,
        });
        inst.status = status;
        inst.branch = branch.to_string();
        inst
    }

    #[test]
    fn test_list_navigation() {
        let mut pane = ListPane::new();
        let instances = vec![
            make_instance("one", InstanceStatus::Running, "main"),
            make_instance("two", InstanceStatus::Ready, ""),
            make_instance("three", InstanceStatus::Loading, "feat"),
        ];
        pane.set_items(&instances);

        assert_eq!(pane.selected_index(), 0);
        assert_eq!(pane.num_items(), 3);

        // Move down
        pane.select_next();
        assert_eq!(pane.selected_index(), 1);

        pane.select_next();
        assert_eq!(pane.selected_index(), 2);

        // Wrap around forward
        pane.select_next();
        assert_eq!(pane.selected_index(), 0);

        // Wrap around backward
        pane.select_previous();
        assert_eq!(pane.selected_index(), 2);

        // Move up
        pane.select_previous();
        assert_eq!(pane.selected_index(), 1);

        // Set directly
        pane.set_selected(0);
        assert_eq!(pane.selected_index(), 0);

        // Clamp out-of-range
        pane.set_selected(100);
        assert_eq!(pane.selected_index(), 2);
    }

    #[test]
    fn test_list_empty() {
        let mut pane = ListPane::new();
        assert_eq!(pane.num_items(), 0);
        assert_eq!(pane.selected_index(), 0);

        // Navigation on empty list should not panic
        pane.select_next();
        pane.select_previous();
        assert_eq!(pane.selected_index(), 0);
    }

    #[test]
    fn test_list_set_items_clamps_selection() {
        let mut pane = ListPane::new();
        let instances = vec![
            make_instance("one", InstanceStatus::Running, ""),
            make_instance("two", InstanceStatus::Ready, ""),
            make_instance("three", InstanceStatus::Ready, ""),
        ];
        pane.set_items(&instances);
        pane.set_selected(2);
        assert_eq!(pane.selected_index(), 2);

        // Shrink list — selection should clamp
        let smaller = vec![make_instance("one", InstanceStatus::Running, "")];
        pane.set_items(&smaller);
        assert_eq!(pane.selected_index(), 0);
    }
}
