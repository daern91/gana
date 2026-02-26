pub mod help;

use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::Clear;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::cmd::{args, CmdExec, SystemCmdExec};
use crate::config::Config;
use crate::session::git::DiffStats;
use crate::keys::{map_key, KeyAction};
use crate::session::instance::{Instance, InstanceOptions, InstanceStatus};
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

/// Signal from handle_key that the caller needs to perform an action
/// that requires leaving the TUI temporarily.
enum AppAction {
    None,
    AttachSession(usize),
}

/// Background update messages from worker threads.
enum BackgroundUpdate {
    PreviewContent(usize, String),
    DiffComputed(usize, DiffStats),
    InstanceReady(usize, crate::session::git::GitWorktree),
    InstanceFailed(usize, String),
    SessionDied(usize),
}

/// Action pending confirmation.
#[derive(Debug, Clone, Copy)]
enum PendingAction {
    KillSession(usize),
    DeleteSession(usize),
    PushSession(usize),
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

    // Prompt flow state (N key: new session with initial prompt)
    creating_with_prompt: bool,
    pending_instance_title: Option<String>,

    // Prompts waiting for async session creation to complete
    pending_prompts: std::collections::HashMap<usize, String>,

    // Background update channels (async tick to prevent TUI freezing)
    bg_sender: mpsc::Sender<BackgroundUpdate>,
    bg_receiver: mpsc::Receiver<BackgroundUpdate>,
}

impl App {
    /// Create a new App with real config.
    pub fn new(config: Config, config_dir: std::path::PathBuf) -> Self {
        let (bg_sender, bg_receiver) = mpsc::channel();
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
            creating_with_prompt: false,
            pending_instance_title: None,
            pending_prompts: std::collections::HashMap::new(),
            bg_sender,
            bg_receiver,
        }
    }

    /// Run the main TUI event loop.
    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        self.load_instances()?;
        self.restore_loaded_instances();

        // Show Ganesha fallback art when there are no sessions
        self.preview.set_fallback();

        // Show help on first run
        let persistent_state = crate::config::state::AppState::load(&self.config_dir);
        if !persistent_state.has_flag(crate::config::state::FLAG_HELP_SEEN) {
            self.state = AppState::Help;
            self.help_overlay = Some(TextOverlay::new("Welcome", help::help_text()));
            let mut persistent_state = persistent_state;
            persistent_state.set_flag(crate::config::state::FLAG_HELP_SEEN);
            let _ = persistent_state.save(&self.config_dir);
        }

        let mut last_bg_tick = Instant::now();

        while self.running {
            terminal.draw(|frame| self.draw(frame))?;

            // Process background results (non-blocking)
            self.process_background_updates();

            // Advance spinner animation for Loading sessions
            let has_loading = self.instances.iter().any(|i| i.status == InstanceStatus::Loading);
            if has_loading {
                self.list.advance_spinner();
                self.refresh_list();
            }

            // Show loading animation or fallback in preview pane
            let sel_idx = self.list.selected_index();
            if sel_idx < self.instances.len() {
                if self.instances[sel_idx].status == InstanceStatus::Loading {
                    let tick = self.list.spinner_tick();
                    let name = self.instances[sel_idx].title.clone();
                    self.preview.set_loading(tick, &name);
                }
            } else if self.instances.is_empty() {
                if self.preview.is_empty() {
                    self.preview.set_fallback();
                }
            }

            // Poll for key events with short timeout for responsiveness
            if event::poll(Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
            {
                let action = self.handle_key(key)?;

                if let AppAction::AttachSession(idx) = action {
                    if idx < self.instances.len() {
                        // 1. Leave TUI FIRST so terminal is back to normal
                        crossterm::terminal::disable_raw_mode()?;
                        crossterm::execute!(
                            std::io::stdout(),
                            crossterm::terminal::LeaveAlternateScreen
                        )?;

                        // 2. NOW get the real terminal size (not TUI size)
                        //    and resize both tmux window + PTY
                        if let Ok((tw, th)) = crossterm::terminal::size() {
                            if let Some(ref mut tmux) =
                                self.instances[idx].tmux_session
                            {
                                let _ = tmux.set_size(tw, th);
                                tmux.resize_pty(tw, th);
                            }
                        }

                        // 3. Enable raw mode for Ctrl+Q detection
                        crossterm::terminal::enable_raw_mode()?;

                        // 4. Attach: pipes stdin/stdout directly to tmux PTY.
                        //    Blocks until user presses Ctrl+Q.
                        let result = self.instances[idx].attach();

                        // Restore TUI
                        crossterm::terminal::disable_raw_mode()?;
                        crossterm::terminal::enable_raw_mode()?;
                        crossterm::execute!(
                            std::io::stdout(),
                            crossterm::terminal::EnterAlternateScreen
                        )?;
                        terminal.clear()?;

                        if let Err(e) = result {
                            self.error
                                .set_error(format!("Failed to attach: {}", e));
                        }
                    }
                }
            }

            // Schedule background updates every 500ms
            if last_bg_tick.elapsed() >= Duration::from_millis(500) {
                self.schedule_background_updates();
                last_bg_tick = Instant::now();
            }
        }

        // Save state on exit so sessions persist across restarts
        let _ = self.save_instances();
        Ok(())
    }

    /// Handle a raw key event by routing to the current state.
    /// Returns an AppAction if the caller needs to do something outside the TUI.
    fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<AppAction> {
        match self.state {
            AppState::TextInput => {
                self.handle_text_input_key(key)?;
                Ok(AppAction::None)
            }
            AppState::Confirm => {
                self.handle_confirm_key(key.code)?;
                Ok(AppAction::None)
            }
            AppState::Help => {
                self.handle_help_key(key.code)?;
                Ok(AppAction::None)
            }
            AppState::Default => {
                if let Some(action) = map_key(key) {
                    return Ok(self.handle_key_action(action));
                }
                Ok(AppAction::None)
            }
        }
    }

    /// Handle a mapped key action in Default state.
    fn handle_key_action(&mut self, action: KeyAction) -> AppAction {
        match action {
            KeyAction::Up => self.list.select_previous(),
            KeyAction::Down => self.list.select_next(),
            KeyAction::Enter | KeyAction::Attach => {
                if !self.instances.is_empty() {
                    return AppAction::AttachSession(self.list.selected_index());
                }
            }
            KeyAction::New => {
                self.menu.highlight_key("n");
                self.state = AppState::TextInput;
                self.text_input = Some(TextInputOverlay::new("New Session"));
                self.creating_with_prompt = false;
            }
            KeyAction::Prompt => {
                self.menu.highlight_key("N");
                self.state = AppState::TextInput;
                self.text_input = Some(TextInputOverlay::new("New Session (with prompt)"));
                self.creating_with_prompt = true;
            }
            KeyAction::Delete => {
                if !self.instances.is_empty() {
                    self.menu.highlight_key("d");
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
                    self.menu.highlight_key("D");
                    let idx = self.list.selected_index();
                    let name = &self.instances[idx].title;
                    let msg = format!("[!] Kill session '{}'? (y/n)", name);
                    self.confirmation = Some(ConfirmationOverlay::new(msg));
                    self.pending_action = Some(PendingAction::KillSession(idx));
                    self.state = AppState::Confirm;
                }
            }
            KeyAction::Pause => {
                if !self.instances.is_empty() {
                    let idx = self.list.selected_index();
                    let cmd = crate::cmd::SystemCmdExec;
                    if self.instances[idx].status == InstanceStatus::Paused {
                        if let Err(e) = self.instances[idx].resume(&cmd) {
                            self.error.set_error(format!("Resume failed: {}", e));
                        }
                    } else if self.instances[idx].status == InstanceStatus::Running {
                        if let Err(e) = self.instances[idx].pause(&cmd) {
                            self.error.set_error(format!("Pause failed: {}", e));
                        }
                    }
                    self.refresh_list();
                    let _ = self.save_instances();
                }
            }
            KeyAction::Push => {
                if !self.instances.is_empty() {
                    let idx = self.list.selected_index();
                    if self.instances[idx].status == InstanceStatus::Running {
                        self.menu.highlight_key("P");
                        let name = &self.instances[idx].title;
                        let msg = format!("Push & create PR for '{}'? (y/n)", name);
                        self.confirmation = Some(ConfirmationOverlay::new(msg));
                        self.pending_action = Some(PendingAction::PushSession(idx));
                        self.state = AppState::Confirm;
                    }
                }
            }
            KeyAction::Quit => {
                self.menu.highlight_key("q");
                self.running = false;
            }
            KeyAction::Help => {
                self.menu.highlight_key("?");
                self.state = AppState::Help;
                self.help_overlay = Some(TextOverlay::new("Help", help::help_text()));
            }
            KeyAction::Tab => {
                self.menu.highlight_key("Tab");
                self.tabbed_window.switch_tab();
            }
            KeyAction::ScrollUp => {
                if !self.preview.is_scrolling() {
                    // Entering scroll mode: fetch full history
                    let history = self
                        .instances
                        .get(self.list.selected_index())
                        .and_then(|inst| inst.preview_full_history());
                    if let Some(history) = history {
                        self.preview.enter_scroll_mode(&history);
                    } else {
                        // No full history available; enter scroll mode with current content
                        self.preview.enter_scroll_mode("");
                    }
                }
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
        AppAction::None
    }

    /// Handle key events while the text input overlay is active.
    fn handle_text_input_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        if let Some(ref mut input) = self.text_input {
            input.handle_key(key);

            if input.is_submitted() {
                let text = input.input().to_string();
                self.text_input = None;

                if self.creating_with_prompt && self.pending_instance_title.is_none() {
                    // First input was the title, now get the prompt
                    if !text.is_empty() {
                        self.pending_instance_title = Some(text);
                        self.text_input = Some(TextInputOverlay::new("Enter prompt"));
                        // Stay in TextInput state
                    } else {
                        self.state = AppState::Default;
                        self.creating_with_prompt = false;
                    }
                } else if self.creating_with_prompt && self.pending_instance_title.is_some() {
                    // Second input was the prompt
                    let title = self.pending_instance_title.take().unwrap();
                    self.state = AppState::Default;
                    self.creating_with_prompt = false;
                    if let Err(e) = self.create_instance_with_prompt(title, text) {
                        self.error.set_error(e.to_string());
                    }
                } else {
                    // Normal new session (no prompt)
                    self.state = AppState::Default;
                    if !text.is_empty() {
                        if let Err(e) = self.create_instance(text) {
                            self.error.set_error(e.to_string());
                        }
                    }
                }
            } else if input.is_cancelled() {
                self.text_input = None;
                self.state = AppState::Default;
                self.creating_with_prompt = false;
                self.pending_instance_title = None;
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
                        PendingAction::PushSession(idx) => {
                            let cmd = SystemCmdExec;
                            if let Err(e) = self.instances[idx].push_and_pr(&cmd) {
                                self.error.set_error(format!("Push failed: {}", e));
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
        let cwd = std::env::current_dir()?.to_string_lossy().to_string();

        // Create placeholder instance with Loading status
        let mut instance = Instance::new(InstanceOptions {
            title: title.clone(),
            path: cwd.clone(),
            program: self.config.default_program.clone(),
            auto_yes: self.config.auto_yes,
        });
        instance.status = InstanceStatus::Loading;
        self.instances.push(instance);
        let idx = self.instances.len() - 1;
        self.refresh_list();

        // Spawn background thread for slow git worktree + tmux creation
        let sender = self.bg_sender.clone();
        let program = self.config.default_program.clone();
        std::thread::spawn(move || {
            let cmd = SystemCmdExec;

            // Create worktree (slow: 0.5-5s)
            let worktree = match crate::session::git::GitWorktree::new(&title, &cwd, &program, &title, &cmd) {
                Ok(wt) => wt,
                Err(e) => {
                    let _ = sender.send(BackgroundUpdate::InstanceFailed(idx, e.to_string()));
                    return;
                }
            };

            // Setup worktree on disk (slow: git worktree add)
            if let Err(e) = worktree.setup(&cmd) {
                let _ = sender.send(BackgroundUpdate::InstanceFailed(idx, e.to_string()));
                return;
            }

            // Create tmux session (medium: 50-500ms)
            let sanitized = crate::session::tmux::sanitize_name(&title);
            // Kill existing session if any
            let _ = cmd.run("tmux", &args(&["kill-session", "-t", &sanitized]));
            // Create new detached session
            let worktree_path = worktree.worktree_path().to_string();
            if let Err(e) = cmd.run("tmux", &args(&[
                "new-session", "-d", "-s", &sanitized, "-c", &worktree_path, &program,
            ])) {
                let _ = sender.send(BackgroundUpdate::InstanceFailed(idx, e.to_string()));
                return;
            }

            // Handle trust prompt (slow: 0-45s polling)
            let timeout_secs: u64 = match program.as_str() {
                "claude" => 30,
                "aider" | "gemini" => 45,
                _ => 0,
            };
            if timeout_secs > 0 {
                let start = std::time::Instant::now();
                let mut interval = std::time::Duration::from_millis(100);
                let (trust_string, response_keys): (&str, Vec<&str>) = if program == "claude" {
                    ("Do you trust the files in this folder?", vec!["Enter"])
                } else {
                    ("Open documentation url", vec!["d", "Enter"])
                };

                while start.elapsed().as_secs() < timeout_secs {
                    std::thread::sleep(interval);
                    if let Ok(content) = cmd.output("tmux", &args(&[
                        "capture-pane", "-p", "-t", &sanitized,
                    ])) {
                        if content.contains(trust_string) {
                            for key in &response_keys {
                                let _ = cmd.run("tmux", &args(&["send-keys", "-t", &sanitized, key]));
                            }
                            break;
                        }
                    }
                    interval = std::cmp::min(
                        std::time::Duration::from_millis((interval.as_millis() as f64 * 1.2) as u64),
                        std::time::Duration::from_secs(1),
                    );
                }
            }

            // Success -- send worktree back to main thread
            let _ = sender.send(BackgroundUpdate::InstanceReady(idx, worktree));
        });

        Ok(())
    }

    fn create_instance_with_prompt(
        &mut self,
        title: String,
        prompt: String,
    ) -> anyhow::Result<()> {
        // Store the prompt for delivery after InstanceReady arrives
        let idx = self.instances.len(); // will be the index after create_instance pushes
        if !prompt.is_empty() {
            self.pending_prompts.insert(idx, prompt);
        }
        self.create_instance(title)
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

    /// Reconnect loaded instances to their still-running tmux sessions.
    /// If a tmux session no longer exists, mark the instance as Ready.
    fn restore_loaded_instances(&mut self) {
        use crate::session::InstanceStatus;
        for instance in &mut self.instances {
            if instance.status == InstanceStatus::Running {
                if instance.restore_session().is_err() {
                    // tmux session is gone — mark as not running
                    instance.status = InstanceStatus::Ready;
                    instance.started = false;
                }
            }
        }
        self.refresh_list();
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

    /// Spawn background threads to fetch preview content and diff stats.
    /// Results arrive via `bg_sender` channel and are processed by
    /// `process_background_updates()`.
    fn schedule_background_updates(&self) {
        let idx = self.list.selected_index();
        if let Some(instance) = self.instances.get(idx) {
            if instance.status != InstanceStatus::Running || !instance.started {
                return;
            }

            // Preview: check session exists, then capture pane content
            let title = instance.title.clone();
            let sender = self.bg_sender.clone();
            let s1 = sender.clone();
            std::thread::spawn(move || {
                let sanitized = crate::session::tmux::sanitize_name(&title);
                let cmd = SystemCmdExec;

                // Check if tmux session still exists
                if cmd.run("tmux", &args(&["has-session", "-t", &sanitized])).is_err() {
                    let _ = s1.send(BackgroundUpdate::SessionDied(idx));
                    return;
                }

                if let Ok(content) = cmd.output(
                    "tmux",
                    &args(&["capture-pane", "-p", "-e", "-J", "-t", &sanitized]),
                ) {
                    let _ = s1.send(BackgroundUpdate::PreviewContent(idx, content));
                }
            });

            // Diff: compute git diff in background
            if let Some(ref worktree) = instance.git_worktree {
                let wt = worktree.clone();
                std::thread::spawn(move || {
                    let cmd = SystemCmdExec;
                    let stats = wt.diff(&cmd);
                    let _ = sender.send(BackgroundUpdate::DiffComputed(idx, stats));
                });
            }
        }
    }

    /// Drain the background update channel and apply results to the UI.
    /// This is non-blocking — `try_recv()` returns immediately if empty.
    fn process_background_updates(&mut self) {
        while let Ok(update) = self.bg_receiver.try_recv() {
            match update {
                BackgroundUpdate::PreviewContent(idx, content) => {
                    if idx == self.list.selected_index() {
                        self.preview.set_content(&content);
                    }
                }
                BackgroundUpdate::DiffComputed(idx, stats) => {
                    if idx == self.list.selected_index() {
                        self.diff_view.set_diff(&stats);
                    }
                    if let Some(instance) = self.instances.get_mut(idx) {
                        instance.diff_stats = Some(stats);
                        self.refresh_list();
                    }
                }
                BackgroundUpdate::InstanceReady(idx, worktree) => {
                    if let Some(instance) = self.instances.get_mut(idx) {
                        instance.branch = worktree.branch().to_string();
                        instance.git_worktree = Some(worktree);

                        // Attach to the tmux session (fast -- just opens PTY)
                        if instance.restore_session().is_ok() {
                            instance.status = InstanceStatus::Running;
                        } else {
                            instance.status = InstanceStatus::Ready;
                            self.error.set_error("Failed to attach to session".to_string());
                        }

                        // Send pending prompt if any
                        if let Some(prompt) = self.pending_prompts.remove(&idx) {
                            if !prompt.is_empty() {
                                instance.send_prompt(&prompt);
                            }
                        }

                        self.refresh_list();
                        let _ = self.save_instances();
                    }
                }
                BackgroundUpdate::InstanceFailed(idx, msg) => {
                    if idx < self.instances.len() {
                        self.instances.remove(idx);
                        self.pending_prompts.remove(&idx);
                        self.refresh_list();
                    }
                    self.error.set_error(format!("Session creation failed: {}", msg));
                }
                BackgroundUpdate::SessionDied(idx) => {
                    if let Some(instance) = self.instances.get_mut(idx) {
                        if instance.status == InstanceStatus::Running {
                            instance.status = InstanceStatus::Ready;
                            instance.tmux_session = None;
                            instance.started = false;
                            self.refresh_list();
                            let _ = self.save_instances();
                        }
                    }
                }
            }
        }
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
        Self::new(Config::default(), std::path::PathBuf::from("/tmp/gana-test"))
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

    #[test]
    fn test_prompt_flow_two_step_input() {
        let mut app = test_app();
        assert_eq!(app.state, AppState::Default);

        // Press 'N' for new session with prompt
        app.handle_key_action(KeyAction::Prompt);
        assert_eq!(app.state, AppState::TextInput);
        assert!(app.creating_with_prompt);
        assert!(app.text_input.is_some());

        // Type a session name
        app.handle_text_input_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
            .unwrap();
        app.handle_text_input_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.text_input.as_ref().unwrap().input(), "my");

        // Submit the title
        app.handle_text_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();

        // Should now be asking for the prompt (second text input)
        assert_eq!(app.state, AppState::TextInput);
        assert!(app.text_input.is_some());
        assert_eq!(app.pending_instance_title.as_deref(), Some("my"));
        assert!(app.creating_with_prompt);

        // The new input should be empty (fresh prompt input)
        assert_eq!(app.text_input.as_ref().unwrap().input(), "");
    }

    #[test]
    fn test_prompt_flow_cancel_at_title() {
        let mut app = test_app();

        app.handle_key_action(KeyAction::Prompt);
        assert_eq!(app.state, AppState::TextInput);
        assert!(app.creating_with_prompt);

        // Cancel during title input
        app.handle_text_input_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.state, AppState::Default);
        assert!(!app.creating_with_prompt);
        assert!(app.pending_instance_title.is_none());
    }

    #[test]
    fn test_prompt_flow_cancel_at_prompt() {
        let mut app = test_app();

        // Enter prompt flow, submit title
        app.handle_key_action(KeyAction::Prompt);
        app.handle_text_input_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE))
            .unwrap();
        app.handle_text_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.pending_instance_title.as_deref(), Some("t"));

        // Cancel during prompt input
        app.handle_text_input_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.state, AppState::Default);
        assert!(!app.creating_with_prompt);
        assert!(app.pending_instance_title.is_none());
    }

    #[test]
    fn test_prompt_flow_empty_title_cancels() {
        let mut app = test_app();

        app.handle_key_action(KeyAction::Prompt);

        // Submit empty title
        app.handle_text_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();

        // Should return to default (empty title cancels flow)
        assert_eq!(app.state, AppState::Default);
        assert!(!app.creating_with_prompt);
    }

    #[test]
    fn test_first_run_help_shown() {
        // Use a unique temp dir to ensure clean state
        let dir = std::path::PathBuf::from("/tmp/gana-test-first-run-help");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        let app_state = crate::config::state::AppState::load(&dir);
        assert!(!app_state.has_flag(crate::config::state::FLAG_HELP_SEEN));

        // After marking as seen, it should be set
        let mut app_state = app_state;
        app_state.set_flag(crate::config::state::FLAG_HELP_SEEN);
        let _ = app_state.save(&dir);

        let app_state2 = crate::config::state::AppState::load(&dir);
        assert!(app_state2.has_flag(crate::config::state::FLAG_HELP_SEEN));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_text_input_32_char_limit() {
        let mut input = TextInputOverlay::new("Test");
        // Type 32 characters
        for _ in 0..32 {
            input.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        }
        assert_eq!(input.input().len(), 32);

        // 33rd character should be rejected
        input.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(input.input().len(), 32);
    }

    #[test]
    fn test_prompt_key_mapping() {
        let event = KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT);
        assert_eq!(map_key(event), Some(KeyAction::Prompt));
    }

    #[test]
    fn test_pause_running_session() {
        let mut app = test_app();
        let mut inst = make_test_instance("pause-test");
        inst.status = InstanceStatus::Running;
        app.instances.push(inst);
        app.refresh_list();

        // Pause action on a Running instance triggers pause.
        // Since there's no real tmux/git, pause() will fail, but
        // the code path is exercised and an error is set.
        app.handle_key_action(KeyAction::Pause);
        // The instance should still exist (pause doesn't remove it)
        assert_eq!(app.instances.len(), 1);
    }

    #[test]
    fn test_push_with_confirmation() {
        let mut app = test_app();
        let mut inst = make_test_instance("push-test");
        inst.status = InstanceStatus::Running;
        app.instances.push(inst);
        app.refresh_list();

        // Push should enter confirmation state
        app.handle_key_action(KeyAction::Push);
        assert_eq!(app.state, AppState::Confirm);
        assert!(app.confirmation.is_some());
        let msg = app.confirmation.as_ref().unwrap().message();
        assert!(msg.contains("push-test"));

        // Cancel with 'n'
        app.handle_confirm_key(KeyCode::Char('n')).unwrap();
        assert_eq!(app.state, AppState::Default);
    }
}
