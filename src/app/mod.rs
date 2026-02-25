pub mod help;

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Clear;
use std::time::Duration;

use crate::cmd::SystemCmdExec;
use crate::config::Config;
use crate::keys::{map_key, KeyAction};
use crate::session::instance::{Instance, InstanceOptions};
use crate::session::storage::{FileStorage, InstanceStorage};
use crate::ui::diff::DiffView;
use crate::ui::err::ErrorDisplay;
use crate::ui::list::ListPane;
use crate::ui::menu::MenuBar;
use crate::ui::overlay::{centered_rect, ConfirmationOverlay, TextInputOverlay, TextOverlay};
use crate::ui::preview::PreviewPane;
use crate::ui::tabbed_window::{Tab, TabbedWindow};

/// Application state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    Default,
    TextInput,
    Confirm,
    Help,
}

/// Action pending confirmation.
#[derive(Debug, Clone, Copy)]
enum PendingAction {
    KillSession(usize),
    DeleteSession(usize),
}

pub struct App {
    // State
    state: AppState,
    instances: Vec<Instance>,
    running: bool,

    // Config
    config: Config,
    config_dir: std::path::PathBuf,

    // UI components
    list: ListPane,
    preview: PreviewPane,
    diff_view: DiffView,
    tabbed_window: TabbedWindow,
    menu: MenuBar,
    error: ErrorDisplay,

    // Overlays
    confirmation: Option<ConfirmationOverlay>,
    text_input: Option<TextInputOverlay>,
    help_overlay: Option<TextOverlay>,

    // Pending action after confirmation
    pending_action: Option<PendingAction>,
}

impl App {
    /// Create a new App with real config.
    pub fn new(config: Config, config_dir: std::path::PathBuf) -> Self {
        Self {
            state: AppState::Default,
            instances: Vec::new(),
            running: true,
            config,
            config_dir,
            list: ListPane::new(),
            preview: PreviewPane::new(),
            diff_view: DiffView::new(),
            tabbed_window: TabbedWindow::new(),
            menu: MenuBar::new(),
            error: ErrorDisplay::new(),
            confirmation: None,
            text_input: None,
            help_overlay: None,
            pending_action: None,
        }
    }

    /// Run the main TUI event loop.
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        self.load_instances()?;

        while self.running {
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(Duration::from_millis(500))?
                && let Event::Key(key) = event::read()?
            {
                self.handle_key(key)?;
            }

            self.tick()?;
        }
        Ok(())
    }

    /// Handle a raw key event by routing to the current state.
    fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match self.state {
            AppState::TextInput => self.handle_text_input_key(key),
            AppState::Confirm => self.handle_confirm_key(key.code),
            AppState::Help => self.handle_help_key(key.code),
            AppState::Default => {
                if let Some(action) = map_key(key) {
                    self.handle_key_action(action);
                }
                Ok(())
            }
        }
    }

    /// Handle a mapped key action in Default state.
    fn handle_key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Up => self.list.select_previous(),
            KeyAction::Down => self.list.select_next(),
            KeyAction::Enter | KeyAction::Attach => {
                // TODO: attach to selected session
            }
            KeyAction::New => {
                self.state = AppState::TextInput;
                self.text_input = Some(TextInputOverlay::new("New Session"));
            }
            KeyAction::Delete => {
                if !self.instances.is_empty() {
                    let idx = self.list.selected_index();
                    let name = &self.instances[idx].title;
                    let msg = format!("Delete session '{}'? (y/n)", name);
                    self.confirmation = Some(ConfirmationOverlay::new(msg));
                    self.pending_action = Some(PendingAction::DeleteSession(idx));
                    self.state = AppState::Confirm;
                }
            }
            KeyAction::Kill => {
                if !self.instances.is_empty() {
                    let idx = self.list.selected_index();
                    let name = &self.instances[idx].title;
                    let msg = format!("[!] Kill session '{}'? (y/n)", name);
                    self.confirmation = Some(ConfirmationOverlay::new(msg));
                    self.pending_action = Some(PendingAction::KillSession(idx));
                    self.state = AppState::Confirm;
                }
            }
            KeyAction::Quit => {
                self.running = false;
            }
            KeyAction::Help => {
                self.state = AppState::Help;
                self.help_overlay = Some(TextOverlay::new("Help", help::help_text()));
            }
            KeyAction::Tab => {
                self.tabbed_window.switch_tab();
            }
            KeyAction::ScrollUp => {
                self.preview.scroll_up(3);
            }
            KeyAction::ScrollDown => {
                self.preview.scroll_down(3);
            }
            KeyAction::Cancel => {
                self.preview.reset_scroll();
            }
            _ => {}
        }
    }

    /// Handle key events while the text input overlay is active.
    fn handle_text_input_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        if let Some(ref mut input) = self.text_input {
            input.handle_key(key);

            if input.is_submitted() {
                let name = input.input().to_string();
                self.text_input = None;
                self.state = AppState::Default;
                if !name.is_empty()
                    && let Err(e) = self.create_instance(name)
                {
                    self.error.set_error(e.to_string());
                }
            } else if input.is_cancelled() {
                self.text_input = None;
                self.state = AppState::Default;
            }
        }
        Ok(())
    }

    /// Handle key events while the confirmation overlay is active.
    fn handle_confirm_key(&mut self, key: KeyCode) -> anyhow::Result<()> {
        if let Some(ref mut overlay) = self.confirmation {
            overlay.handle_key(key);

            if overlay.is_dismissed() {
                let confirmed = overlay.is_confirmed();
                let action = self.pending_action.take();
                self.confirmation = None;
                self.state = AppState::Default;

                if confirmed
                    && let Some(pending) = action
                {
                    match pending {
                        PendingAction::KillSession(idx) => {
                            if let Err(e) = self.kill_instance(idx) {
                                self.error.set_error(e.to_string());
                            }
                        }
                        PendingAction::DeleteSession(idx) => {
                            if let Err(e) = self.delete_instance(idx) {
                                self.error.set_error(e.to_string());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle key events while the help overlay is active.
    fn handle_help_key(&mut self, key: KeyCode) -> anyhow::Result<()> {
        if let Some(ref mut overlay) = self.help_overlay {
            overlay.handle_key(key);

            if overlay.is_dismissed() {
                self.help_overlay = None;
                self.state = AppState::Default;
            }
        }
        Ok(())
    }

    /// Draw all UI components.
    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        // Main layout: horizontal split [list | right_pane]
        let main_layout = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(area);

        // Right pane: vertical split [tabs | content | error? | menu]
        let right_constraints = if self.error.has_error() {
            vec![
                Constraint::Length(1),  // tab bar
                Constraint::Min(1),     // content
                Constraint::Length(3),  // error
                Constraint::Length(1),  // menu bar
            ]
        } else {
            vec![
                Constraint::Length(1),  // tab bar
                Constraint::Min(1),     // content
                Constraint::Length(1),  // menu bar
            ]
        };
        let right_layout = Layout::vertical(right_constraints).split(main_layout[1]);

        // Render list
        frame.render_widget(&self.list, main_layout[0]);

        // Render tab bar
        frame.render_widget(&self.tabbed_window, right_layout[0]);

        // Render content based on active tab
        match self.tabbed_window.active_tab() {
            Tab::Preview => frame.render_widget(&self.preview, right_layout[1]),
            Tab::Diff => frame.render_widget(&self.diff_view, right_layout[1]),
        }

        // Render error if present
        if self.error.has_error() {
            frame.render_widget(&self.error, right_layout[2]);
            frame.render_widget(&self.menu, right_layout[3]);
        } else {
            frame.render_widget(&self.menu, right_layout[2]);
        }

        // Render overlays on top
        match self.state {
            AppState::Confirm => {
                if let Some(ref overlay) = self.confirmation {
                    let popup_area = centered_rect(50, 30, area);
                    frame.render_widget(Clear, popup_area);
                    overlay.render_content(popup_area, frame.buffer_mut());
                }
            }
            AppState::TextInput => {
                if let Some(ref overlay) = self.text_input {
                    let popup_area = centered_rect(50, 20, area);
                    frame.render_widget(Clear, popup_area);
                    overlay.render_content(popup_area, frame.buffer_mut());
                }
            }
            AppState::Help => {
                if let Some(ref overlay) = self.help_overlay {
                    let popup_area = centered_rect(60, 70, area);
                    frame.render_widget(Clear, popup_area);
                    overlay.render_content(popup_area, frame.buffer_mut());
                }
            }
            AppState::Default => {}
        }
    }

    // ── Instance management ─────────────────────────────────────────

    fn create_instance(&mut self, title: String) -> anyhow::Result<()> {
        let cmd = SystemCmdExec;
        let cwd = std::env::current_dir()?.to_string_lossy().to_string();
        let mut instance = Instance::new(InstanceOptions {
            title,
            path: cwd,
            program: self.config.default_program.clone(),
            auto_yes: self.config.auto_yes,
        });
        instance.start(true, &cmd)?;
        self.instances.push(instance);
        self.refresh_list();
        self.save_instances()?;
        Ok(())
    }

    fn kill_instance(&mut self, idx: usize) -> anyhow::Result<()> {
        let cmd = SystemCmdExec;
        if idx < self.instances.len() {
            self.instances[idx].kill(&cmd)?;
            self.instances.remove(idx);
            self.refresh_list();
            self.save_instances()?;
        }
        Ok(())
    }

    fn delete_instance(&mut self, idx: usize) -> anyhow::Result<()> {
        if idx < self.instances.len() {
            self.instances.remove(idx);
            self.refresh_list();
            self.save_instances()?;
        }
        Ok(())
    }

    fn refresh_list(&mut self) {
        self.list.set_items(&self.instances);
    }

    fn load_instances(&mut self) -> anyhow::Result<()> {
        let storage = FileStorage::new(&self.config_dir);
        match storage.load_instances() {
            Ok(instances) => {
                self.instances = instances;
                self.refresh_list();
            }
            Err(e) => {
                self.error.set_error(format!("Failed to load sessions: {}", e));
            }
        }
        Ok(())
    }

    fn save_instances(&self) -> anyhow::Result<()> {
        let storage = FileStorage::new(&self.config_dir);
        storage.save_instances(&self.instances)?;
        Ok(())
    }

    fn tick(&mut self) -> anyhow::Result<()> {
        let idx = self.list.selected_index();
        if let Some(instance) = self.instances.get_mut(idx) {
            if let Some(content) = instance.preview() {
                self.preview.set_content(&content);
            }
            let cmd = SystemCmdExec;
            instance.update_diff_stats(&cmd);
            if let Some(stats) = instance.get_diff_stats() {
                self.diff_view.set_diff(stats);
            }
        }
        Ok(())
    }
}

/// Set up terminal, run the TUI app, and restore terminal on exit.
pub fn run(config: Config, config_dir: std::path::PathBuf) -> anyhow::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut app = App::new(config, config_dir);
    let result = app.run(&mut terminal);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    result
}

// ── Test support ────────────────────────────────────────────────────

#[cfg(test)]
impl App {
    /// Create an App suitable for unit testing (no real config dir).
    fn new_for_test() -> Self {
        Self::new(Config::default(), std::path::PathBuf::from("/tmp/league-test"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn test_app() -> App {
        App::new_for_test()
    }

    fn make_test_instance(title: &str) -> Instance {
        Instance::new(InstanceOptions {
            title: title.into(),
            path: "/tmp".into(),
            program: "bash".into(),
            auto_yes: false,
        })
    }

    #[test]
    fn test_initial_state_is_default() {
        let app = test_app();
        assert_eq!(app.state, AppState::Default);
        assert!(app.running);
        assert!(app.instances.is_empty());
    }

    #[test]
    fn test_confirmation_modal_state_transitions() {
        let mut app = test_app();
        assert_eq!(app.state, AppState::Default);

        // Add an instance so kill has something to target
        app.instances.push(make_test_instance("test"));
        app.refresh_list();

        // Press D to kill -> should enter Confirm state
        app.handle_key_action(KeyAction::Kill);
        assert_eq!(app.state, AppState::Confirm);
        assert!(app.confirmation.is_some());

        // Press Esc to cancel -> back to Default
        app.handle_confirm_key(KeyCode::Esc).unwrap();
        assert_eq!(app.state, AppState::Default);
        assert!(app.confirmation.is_none());
    }

    #[test]
    fn test_confirmation_key_handling() {
        let mut app = test_app();
        app.instances.push(make_test_instance("sess1"));
        app.refresh_list();

        // Enter confirm state for kill
        app.handle_key_action(KeyAction::Kill);
        assert_eq!(app.state, AppState::Confirm);

        // Random key should not dismiss
        app.handle_confirm_key(KeyCode::Char('x')).unwrap();
        assert_eq!(app.state, AppState::Confirm);

        // 'n' cancels
        app.handle_confirm_key(KeyCode::Char('n')).unwrap();
        assert_eq!(app.state, AppState::Default);
        // Instance should still be there (not killed)
        assert_eq!(app.instances.len(), 1);
    }

    #[test]
    fn test_confirmation_y_confirms_delete() {
        let mut app = test_app();
        app.instances.push(make_test_instance("to-delete"));
        app.refresh_list();

        // Enter confirm state for delete
        app.handle_key_action(KeyAction::Delete);
        assert_eq!(app.state, AppState::Confirm);

        // 'y' confirms -> instance removed
        app.handle_confirm_key(KeyCode::Char('y')).unwrap();
        assert_eq!(app.state, AppState::Default);
        assert!(app.instances.is_empty());
    }

    #[test]
    fn test_text_input_flow() {
        let mut app = test_app();
        assert_eq!(app.state, AppState::Default);

        // Press 'n' for new session -> TextInput state
        app.handle_key_action(KeyAction::New);
        assert_eq!(app.state, AppState::TextInput);
        assert!(app.text_input.is_some());

        // Type some characters
        app.handle_text_input_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        app.handle_text_input_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))
            .unwrap();

        assert_eq!(app.text_input.as_ref().unwrap().input(), "te");

        // Cancel with Esc
        app.handle_text_input_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.state, AppState::Default);
        assert!(app.text_input.is_none());
    }

    #[test]
    fn test_help_toggle() {
        let mut app = test_app();
        assert_eq!(app.state, AppState::Default);

        // Press ? for help
        app.handle_key_action(KeyAction::Help);
        assert_eq!(app.state, AppState::Help);
        assert!(app.help_overlay.is_some());

        // Press Esc to dismiss
        app.handle_help_key(KeyCode::Esc).unwrap();
        assert_eq!(app.state, AppState::Default);
        assert!(app.help_overlay.is_none());
    }

    #[test]
    fn test_navigation_updates_selection() {
        let mut app = test_app();
        app.instances.push(make_test_instance("first"));
        app.instances.push(make_test_instance("second"));
        app.instances.push(make_test_instance("third"));
        app.refresh_list();

        assert_eq!(app.list.selected_index(), 0);

        app.handle_key_action(KeyAction::Down);
        assert_eq!(app.list.selected_index(), 1);

        app.handle_key_action(KeyAction::Down);
        assert_eq!(app.list.selected_index(), 2);

        app.handle_key_action(KeyAction::Up);
        assert_eq!(app.list.selected_index(), 1);
    }

    #[test]
    fn test_quit_sets_running_false() {
        let mut app = test_app();
        assert!(app.running);

        app.handle_key_action(KeyAction::Quit);
        assert!(!app.running);
    }

    #[test]
    fn test_tab_switches_view() {
        let mut app = test_app();
        assert_eq!(app.tabbed_window.active_tab(), Tab::Preview);

        app.handle_key_action(KeyAction::Tab);
        assert_eq!(app.tabbed_window.active_tab(), Tab::Diff);

        app.handle_key_action(KeyAction::Tab);
        assert_eq!(app.tabbed_window.active_tab(), Tab::Preview);
    }

    #[test]
    fn test_scroll_in_default_state() {
        let mut app = test_app();

        app.handle_key_action(KeyAction::ScrollUp);
        assert!(app.preview.is_scrolling());

        app.handle_key_action(KeyAction::Cancel);
        assert!(!app.preview.is_scrolling());
    }

    #[test]
    fn test_kill_on_empty_list_does_nothing() {
        let mut app = test_app();
        assert!(app.instances.is_empty());

        // Kill on empty list should stay in Default state
        app.handle_key_action(KeyAction::Kill);
        assert_eq!(app.state, AppState::Default);
    }

    #[test]
    fn test_delete_on_empty_list_does_nothing() {
        let mut app = test_app();
        assert!(app.instances.is_empty());

        app.handle_key_action(KeyAction::Delete);
        assert_eq!(app.state, AppState::Default);
    }

    #[test]
    fn test_key_routing_in_text_input_state() {
        let mut app = test_app();
        app.handle_key_action(KeyAction::New);
        assert_eq!(app.state, AppState::TextInput);

        // In TextInput state, normal keys go to the input, not to actions
        // 'q' should not quit — it should type 'q'
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .unwrap();
        assert!(app.running); // still running
        assert_eq!(app.text_input.as_ref().unwrap().input(), "q");
    }

    #[test]
    fn test_confirmation_message_format_kill() {
        let mut app = test_app();
        app.instances.push(make_test_instance("my-feature"));
        app.refresh_list();

        app.handle_key_action(KeyAction::Kill);
        let msg = app.confirmation.as_ref().unwrap().message();
        assert_eq!(msg, "[!] Kill session 'my-feature'? (y/n)");
    }
}
