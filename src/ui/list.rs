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
        let repos: std::collections::HashSet<&str> = instances
            .iter()
            .filter_map(|i| i.git_worktree.as_ref().map(|w| w.repo_name()))
            .collect();
        let show_repo = repos.len() > 1;

        self.items = instances
            .iter()
            .map(|inst| render_instance(inst, show_repo))
            .collect();
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
///
/// When `show_repo` is true and the instance has a git worktree, the repo name
/// is appended after the branch in parentheses (e.g. `[branch] (repo)`).
fn render_instance(inst: &Instance, show_repo: bool) -> ListItem<'static> {
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

    if show_repo {
        if let Some(ref wt) = inst.git_worktree {
            spans.push(Span::styled(
                format!(" ({})", wt.repo_name()),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    if let Some(ref stats) = inst.diff_stats {
        if stats.added_lines > 0 || stats.removed_lines > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("+{}", stats.added_lines),
                Style::default().fg(Color::Green),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("-{}", stats.removed_lines),
                Style::default().fg(Color::Red),
            ));
        }
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

    /// Helper to render a list pane with given instances and extract buffer text for a row.
    fn render_list_row(instances: &[Instance], row: usize) -> String {
        let mut pane = ListPane::new();
        pane.set_items(instances);
        // Use enough space: border takes 2 cols/2 rows
        let area = Rect::new(0, 0, 80, (instances.len() as u16) + 2);
        let mut buf = Buffer::empty(area);
        Widget::render(&pane, area, &mut buf);
        // Row 0 is top border, data rows start at y=1
        let y = (row + 1) as u16;
        (0..80)
            .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
            .collect()
    }

    #[test]
    fn test_render_instance_with_diff_stats() {
        use crate::session::git::DiffStats;

        let mut inst = make_instance("feature", InstanceStatus::Running, "dev");
        inst.diff_stats = Some(DiffStats {
            content: String::new(),
            added_lines: 15,
            removed_lines: 3,
            error: None,
        });

        let content = render_list_row(&[inst], 0);
        assert!(content.contains("+15"), "Expected +15 in: {}", content);
        assert!(content.contains("-3"), "Expected -3 in: {}", content);
    }

    #[test]
    fn test_render_instance_without_diff_stats() {
        let inst = make_instance("feature", InstanceStatus::Running, "dev");
        let content = render_list_row(&[inst], 0);

        // Should have branch but no diff stats
        assert!(content.contains("[dev]"));
        assert!(!content.contains("+0"));
        assert!(!content.contains("-0"));
    }

    #[test]
    fn test_render_instance_zero_diff_stats() {
        use crate::session::git::DiffStats;

        let mut inst = make_instance("feature", InstanceStatus::Running, "dev");
        inst.diff_stats = Some(DiffStats {
            content: String::new(),
            added_lines: 0,
            removed_lines: 0,
            error: None,
        });

        let content = render_list_row(&[inst], 0);

        // Zero stats should not be displayed
        assert!(!content.contains("+0"));
        assert!(!content.contains("-0"));
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

    fn make_instance_with_repo(
        title: &str,
        status: InstanceStatus,
        branch: &str,
        repo_path: &str,
    ) -> Instance {
        use crate::session::git::GitWorktree;
        let mut inst = make_instance(title, status, branch);
        inst.git_worktree = Some(GitWorktree::from_storage(
            repo_path.to_string(),
            "/wt".to_string(),
            "s".to_string(),
            branch.to_string(),
            "abc".to_string(),
        ));
        inst
    }

    /// Render a single instance directly (bypassing set_items multi-repo detection)
    /// and return the rendered text.
    fn render_single_direct(inst: &Instance, show_repo: bool) -> String {
        let item = render_instance(inst, show_repo);
        let list = List::new(vec![item]);
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        Widget::render(list, area, &mut buf);
        (0..80)
            .map(|x| buf.cell((x, 0u16)).unwrap().symbol().to_string())
            .collect()
    }

    #[test]
    fn test_render_instance_multi_repo_shows_name() {
        let inst = make_instance_with_repo(
            "test",
            InstanceStatus::Running,
            "gana/test",
            "/path/to/myrepo",
        );
        let text = render_single_direct(&inst, true);
        assert!(text.contains("(myrepo)"), "Expected (myrepo) in: {}", text);
    }

    #[test]
    fn test_render_instance_single_repo_hides_name() {
        let inst = make_instance_with_repo(
            "test",
            InstanceStatus::Running,
            "gana/test",
            "/path/to/myrepo",
        );
        let text = render_single_direct(&inst, false);
        assert!(
            !text.contains("(myrepo)"),
            "Should not contain repo name: {}",
            text
        );
    }

    #[test]
    fn test_set_items_detects_multi_repo() {
        let instances = vec![
            make_instance_with_repo("a", InstanceStatus::Running, "feat-a", "/repos/alpha"),
            make_instance_with_repo("b", InstanceStatus::Running, "feat-b", "/repos/beta"),
        ];
        let content = render_list_row(&instances, 0);
        assert!(
            content.contains("(alpha)"),
            "Expected (alpha) in: {}",
            content
        );
        let content1 = render_list_row(&instances, 1);
        assert!(
            content1.contains("(beta)"),
            "Expected (beta) in: {}",
            content1
        );
    }

    #[test]
    fn test_set_items_single_repo_hides_name() {
        let instances = vec![
            make_instance_with_repo("a", InstanceStatus::Running, "feat-a", "/repos/same"),
            make_instance_with_repo("b", InstanceStatus::Running, "feat-b", "/repos/same"),
        ];
        let content = render_list_row(&instances, 0);
        assert!(
            !content.contains("(same)"),
            "Should not contain repo name: {}",
            content
        );
    }
}
