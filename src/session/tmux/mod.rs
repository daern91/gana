pub mod pty;

use std::fs::File;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::cmd::{CmdExec, args};
use pty::PtyFactory;

/// Prefix for all league tmux session names.
pub const TMUX_PREFIX: &str = "league_";

#[derive(Debug, Error)]
pub enum TmuxError {
    #[error("tmux command failed: {0}")]
    CommandFailed(String),
    #[error("PTY error: {0}")]
    PtyError(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error(transparent)]
    Cmd(#[from] crate::cmd::CmdError),
}

/// Sanitize a session name for use as a tmux session name.
/// Replaces non-alphanumeric characters with underscores and adds prefix.
pub fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    // Collapse consecutive underscores
    let mut result = String::with_capacity(sanitized.len());
    let mut prev_underscore = false;
    for c in sanitized.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    // Trim trailing underscores
    let trimmed = result.trim_end_matches('_');
    format!("{}{}", TMUX_PREFIX, trimmed)
}

/// A tmux session manager that handles the lifecycle of a tmux session.
pub struct TmuxSession {
    /// Raw session name from the user.
    session_name: String,
    /// Sanitized name used as the tmux session identifier.
    sanitized_name: String,
    /// Current PTY master file descriptor.
    ptmx: Option<File>,
    /// SHA256 hash of the last captured pane content, for change detection.
    status_hash: String,
    /// Program to run in the session (e.g. "claude", "aider").
    program: String,
    /// Command executor for running tmux commands.
    cmd_exec: Box<dyn CmdExec>,
    /// Factory for creating PTY handles.
    pty_factory: Box<dyn PtyFactory>,
    /// Whether the session is currently attached.
    attached: bool,
    /// Terminal height.
    height: u16,
    /// Terminal width.
    width: u16,
}

impl TmuxSession {
    /// Create a new TmuxSession with the given name and program.
    pub fn new(
        name: &str,
        program: &str,
        cmd_exec: Box<dyn CmdExec>,
        pty_factory: Box<dyn PtyFactory>,
    ) -> Self {
        let sanitized_name = sanitize_name(name);
        Self {
            session_name: name.to_string(),
            sanitized_name,
            ptmx: None,
            status_hash: String::new(),
            program: program.to_string(),
            cmd_exec,
            pty_factory,
            attached: false,
            height: 0,
            width: 0,
        }
    }

    /// Returns the raw session name.
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Returns the sanitized tmux session name.
    pub fn sanitized_name(&self) -> &str {
        &self.sanitized_name
    }

    /// Returns whether the session is currently attached.
    pub fn attached(&self) -> bool {
        self.attached
    }

    /// Start a new tmux session in the given working directory.
    ///
    /// 1. If a session with this name already exists, kill it.
    /// 2. Create a new detached session running the program.
    /// 3. Attach to the session and store the PTY handle.
    pub fn start(&mut self, workdir: &str) -> Result<(), TmuxError> {
        // Check if session already exists; if so, kill it
        let has_session_result = self.cmd_exec.run(
            "tmux",
            &args(&["has-session", "-t", &self.sanitized_name]),
        );
        if has_session_result.is_ok() {
            // Session exists, kill it
            self.cmd_exec.run(
                "tmux",
                &args(&["kill-session", "-t", &self.sanitized_name]),
            )?;
        }

        // Create new detached session with PTY
        let mut new_cmd = std::process::Command::new("tmux");
        new_cmd.args([
            "new-session",
            "-d",
            "-s",
            &self.sanitized_name,
            "-c",
            workdir,
            &self.program,
        ]);
        let _first_pty = self.pty_factory.start(&mut new_cmd)?;
        // Close the first PTY - we only needed it to create the session.
        // (dropping _first_pty closes the file descriptor)

        // Attach to the session with a new PTY
        let mut attach_cmd = std::process::Command::new("tmux");
        attach_cmd.args(["attach-session", "-t", &self.sanitized_name]);
        let ptmx = self.pty_factory.start(&mut attach_cmd)?;
        self.ptmx = Some(ptmx);
        self.attached = true;

        // Auto-respond to trust prompts (e.g. "Do you trust the files in this folder?")
        self.handle_trust_prompt()?;

        Ok(())
    }

    /// Poll for and auto-respond to trust prompts from AI programs.
    ///
    /// Different programs show different trust prompts on first launch:
    /// - Claude: "Do you trust the files in this folder?" → Enter
    /// - Aider/Gemini: "Open documentation url" → "d" then Enter
    ///
    /// Uses exponential backoff polling, matching the Go implementation.
    fn handle_trust_prompt(&self) -> Result<(), TmuxError> {
        let (search_string, response_keys, timeout_secs) = match self.program.as_str() {
            "claude" => ("Do you trust the files in this folder?", vec!["Enter"], 30u64),
            "aider" | "gemini" => ("Open documentation url", vec!["d", "Enter"], 45u64),
            _ => return Ok(()), // No trust prompt handling for unknown programs
        };

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let mut poll_interval = std::time::Duration::from_millis(100);

        while start.elapsed() < timeout {
            std::thread::sleep(poll_interval);

            if let Ok(content) = self.capture_pane_content(false) {
                if content.contains(search_string) {
                    for key in &response_keys {
                        self.send_keys(key)?;
                    }
                    return Ok(());
                }
            }

            // Exponential backoff with cap at 1 second (matching Go: *= 1.2, cap 1s)
            poll_interval = std::time::Duration::from_millis(
                ((poll_interval.as_millis() as f64 * 1.2) as u64).min(1000),
            );
        }

        // Timeout is not an error - the prompt may have been handled already
        Ok(())
    }

    /// Restore an existing tmux session by attaching to it.
    /// Unlike `start`, this does not create or kill sessions.
    pub fn restore(&mut self) -> Result<(), TmuxError> {
        // Verify the session exists
        self.cmd_exec
            .run("tmux", &args(&["has-session", "-t", &self.sanitized_name]))
            .map_err(|_| TmuxError::SessionNotFound(self.sanitized_name.clone()))?;

        // Attach to the existing session
        let mut attach_cmd = std::process::Command::new("tmux");
        attach_cmd.args(["attach-session", "-t", &self.sanitized_name]);
        let ptmx = self.pty_factory.start(&mut attach_cmd)?;
        self.ptmx = Some(ptmx);
        self.attached = true;

        Ok(())
    }

    /// Capture the content of the tmux pane.
    ///
    /// If `full_history` is true, captures the entire scrollback buffer.
    /// Otherwise, captures only the visible pane content.
    pub fn capture_pane_content(&self, full_history: bool) -> Result<String, TmuxError> {
        let cmd_args = if full_history {
            args(&["capture-pane", "-p", "-e", "-J", "-t", &self.sanitized_name, "-S", "-"])
        } else {
            args(&["capture-pane", "-p", "-e", "-J", "-t", &self.sanitized_name])
        };
        let output = self.cmd_exec.output("tmux", &cmd_args)?;
        Ok(output)
    }

    /// Check if the pane content has changed since the last check.
    ///
    /// Captures the current pane content, computes its SHA256 hash, and
    /// compares it with the stored hash. Returns true if content has changed.
    /// Also returns true if AI-specific prompts are detected.
    pub fn has_updated(&mut self) -> Result<bool, TmuxError> {
        let content = self.capture_pane_content(false)?;
        let hash = format!("{:x}", Sha256::digest(content.as_bytes()));

        let changed = hash != self.status_hash;
        if changed {
            self.status_hash = hash;
        }

        // Also check for AI-specific prompts that indicate the session needs attention
        let has_prompt = Self::has_ai_prompt(&content, &self.program);

        Ok(changed || has_prompt)
    }

    /// Check if the content contains AI-specific prompts that need user attention.
    fn has_ai_prompt(content: &str, program: &str) -> bool {
        match program {
            "claude" => content.contains("No, and tell Claude what to do differently"),
            "aider" => content.contains("(Y)es/(N)o/(D)on't ask again"),
            "gemini" => content.contains("Yes, allow once"),
            "amp" => {
                // Amp has specific prompt patterns
                content.contains("Allow") && content.contains("Deny")
            }
            _ => false,
        }
    }

    /// Attach interactively to the tmux session.
    ///
    /// Pipes stdin/stdout directly to/from the tmux session's PTY.
    /// Returns when the user presses Ctrl+Q (ASCII 17) to detach.
    /// After returning, calls `detach()` to restore a fresh monitoring PTY.
    pub fn attach_interactive(&mut self) -> Result<(), TmuxError> {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let ptmx = match self.ptmx.as_ref() {
            Some(f) => f,
            None => return Err(TmuxError::CommandFailed("no PTY to attach to".into())),
        };

        // Clone file descriptors for the two threads
        let mut ptmx_reader = ptmx
            .try_clone()
            .map_err(|e| TmuxError::PtyError(e.to_string()))?;
        let mut ptmx_writer = ptmx
            .try_clone()
            .map_err(|e| TmuxError::PtyError(e.to_string()))?;

        // Shared flag to stop the resize monitor thread
        let stop_flag = Arc::new(AtomicBool::new(false));

        // Channel to signal detach
        let (detach_tx, detach_rx) = std::sync::mpsc::channel::<()>();
        let detach_tx2 = detach_tx.clone();

        // Thread 1: copy PTY output -> stdout
        let stdout_handle = std::thread::spawn(move || {
            let mut stdout = std::io::stdout();
            let mut buf = [0u8; 4096];
            loop {
                match ptmx_reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = stdout.write_all(&buf[..n]);
                        let _ = stdout.flush();
                    }
                    Err(_) => break,
                }
            }
            let _ = detach_tx2.send(());
        });

        // Thread 2: read stdin, detect Ctrl+Q, forward rest to PTY
        let stdin_handle = std::thread::spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let mut buf = [0u8; 32];

            // Skip initial terminal control sequences (first 50ms)
            let start = std::time::Instant::now();

            loop {
                match stdin.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        // Skip control sequences in first 50ms
                        if start.elapsed() < std::time::Duration::from_millis(50) {
                            continue;
                        }

                        // Check for Ctrl+Q (ASCII 17)
                        if n == 1 && buf[0] == 17 {
                            let _ = detach_tx.send(());
                            return;
                        }

                        // Forward to tmux
                        let _ = ptmx_writer.write_all(&buf[..n]);
                        let _ = ptmx_writer.flush();
                    }
                    Err(_) => break,
                }
            }
        });

        // Thread 3: monitor terminal size changes and resize tmux window
        let session_name_for_resize = self.sanitized_name.clone();
        let resize_stop = Arc::clone(&stop_flag);
        let _resize_handle = std::thread::spawn(move || {
            let mut last_size = crossterm::terminal::size().unwrap_or((80, 24));
            // Do an initial resize to sync tmux with current terminal size
            let _ = std::process::Command::new("tmux")
                .args([
                    "resize-window",
                    "-t",
                    &session_name_for_resize,
                    "-x",
                    &last_size.0.to_string(),
                    "-y",
                    &last_size.1.to_string(),
                ])
                .output();

            while !resize_stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                if let Ok(current_size) = crossterm::terminal::size() {
                    if current_size != last_size {
                        last_size = current_size;
                        let _ = std::process::Command::new("tmux")
                            .args([
                                "resize-window",
                                "-t",
                                &session_name_for_resize,
                                "-x",
                                &current_size.0.to_string(),
                                "-y",
                                &current_size.1.to_string(),
                            ])
                            .output();
                    }
                }
            }
        });

        // Block until detach signal
        let _ = detach_rx.recv();

        // Signal the resize thread to stop
        stop_flag.store(true, Ordering::Relaxed);

        // Clean up threads (they'll exit when PTY closes or stop flag is set)
        // Don't join stdout_handle - it may be blocked on read
        drop(stdout_handle);
        drop(stdin_handle);

        // Detach: close current PTY and open a fresh one for monitoring
        self.detach()?;

        Ok(())
    }

    /// Send keys to the tmux session.
    pub fn send_keys(&self, keys: &str) -> Result<(), TmuxError> {
        self.cmd_exec.run(
            "tmux",
            &args(&["send-keys", "-t", &self.sanitized_name, keys]),
        )?;
        Ok(())
    }

    /// Detach from the tmux session.
    ///
    /// Closes the current PTY and opens a fresh one for monitoring.
    pub fn detach(&mut self) -> Result<(), TmuxError> {
        // Close the current PTY
        self.ptmx.take();

        // Start a fresh PTY for monitoring
        let mut attach_cmd = std::process::Command::new("tmux");
        attach_cmd.args(["attach-session", "-t", &self.sanitized_name]);
        let ptmx = self.pty_factory.start(&mut attach_cmd)?;
        self.ptmx = Some(ptmx);
        self.attached = false;

        Ok(())
    }

    /// Close the tmux session entirely.
    ///
    /// Closes the PTY and kills the tmux session.
    pub fn close(&mut self) -> Result<(), TmuxError> {
        // Close PTY
        self.ptmx.take();

        // Kill the session
        self.cmd_exec.run(
            "tmux",
            &args(&["kill-session", "-t", &self.sanitized_name]),
        )?;

        Ok(())
    }

    /// Resize the tmux window.
    pub fn set_size(&mut self, width: u16, height: u16) -> Result<(), TmuxError> {
        self.width = width;
        self.height = height;
        self.cmd_exec.run(
            "tmux",
            &args(&[
                "resize-window",
                "-t",
                &self.sanitized_name,
                "-x",
                &width.to_string(),
                "-y",
                &height.to_string(),
            ]),
        )?;
        Ok(())
    }

    /// Clean up all league tmux sessions.
    ///
    /// Lists all tmux sessions and kills any that start with the league prefix.
    pub fn cleanup_sessions(cmd_exec: &dyn CmdExec) -> Result<(), TmuxError> {
        let output = match cmd_exec.output(
            "tmux",
            &args(&["list-sessions", "-F", "#{session_name}"]),
        ) {
            Ok(output) => output,
            Err(_) => {
                // No tmux server running or no sessions - nothing to clean up
                return Ok(());
            }
        };

        for line in output.lines() {
            let session_name = line.trim();
            if session_name.starts_with(TMUX_PREFIX) {
                // Best-effort cleanup - ignore errors for individual sessions
                let _ = cmd_exec.run("tmux", &args(&["kill-session", "-t", session_name]));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // --- Mock CmdExec that records commands ---

    #[derive(Default, Clone)]
    struct RecordingCmdExec {
        commands: Arc<Mutex<Vec<(String, Vec<String>)>>>,
        output_responses: Arc<Mutex<Vec<String>>>,
        run_fail_on: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingCmdExec {
        fn new() -> Self {
            Self::default()
        }

        fn with_output_responses(responses: Vec<String>) -> Self {
            Self {
                output_responses: Arc::new(Mutex::new(responses)),
                ..Self::default()
            }
        }

        /// Configure the mock so that `run()` fails when the args contain the given substring.
        fn fail_run_when_contains(&self, pattern: &str) {
            self.run_fail_on.lock().unwrap().push(pattern.to_string());
        }

        fn commands(&self) -> Vec<(String, Vec<String>)> {
            self.commands.lock().unwrap().clone()
        }
    }

    impl CmdExec for RecordingCmdExec {
        fn run(&self, name: &str, args: &[String]) -> Result<(), crate::cmd::CmdError> {
            let full = format!("{} {}", name, args.join(" "));
            let fail_on = self.run_fail_on.lock().unwrap();
            for pattern in fail_on.iter() {
                if full.contains(pattern) {
                    self.commands
                        .lock()
                        .unwrap()
                        .push((name.to_string(), args.to_vec()));
                    return Err(crate::cmd::CmdError::Failed(format!(
                        "mock failure: {}",
                        full
                    )));
                }
            }
            self.commands
                .lock()
                .unwrap()
                .push((name.to_string(), args.to_vec()));
            Ok(())
        }

        fn output(&self, name: &str, args: &[String]) -> Result<String, crate::cmd::CmdError> {
            self.commands
                .lock()
                .unwrap()
                .push((name.to_string(), args.to_vec()));
            let mut responses = self.output_responses.lock().unwrap();
            if responses.is_empty() {
                Ok(String::new())
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    // --- Mock PtyFactory ---

    struct MockPtyFactory {
        files: Mutex<Vec<File>>,
    }

    impl MockPtyFactory {
        fn new() -> Self {
            Self {
                files: Mutex::new(Vec::new()),
            }
        }

        fn file_count(&self) -> usize {
            self.files.lock().unwrap().len()
        }
    }

    impl PtyFactory for MockPtyFactory {
        fn start(
            &self,
            _cmd: &mut std::process::Command,
        ) -> Result<File, TmuxError> {
            let tmp = tempfile::NamedTempFile::new().unwrap();
            let file = tmp.into_file();
            // Clone the file descriptor for tracking
            let clone = file.try_clone().unwrap();
            self.files.lock().unwrap().push(clone);
            Ok(file)
        }

        fn close(&self) {}
    }

    // --- Tests for sanitize_name ---

    #[test]
    fn test_sanitize_name_simple() {
        assert_eq!(sanitize_name("asdf"), format!("{}asdf", TMUX_PREFIX));
    }

    #[test]
    fn test_sanitize_name_special_chars() {
        assert_eq!(
            sanitize_name("a sd f . . asdf"),
            format!("{}a_sd_f_asdf", TMUX_PREFIX)
        );
    }

    // --- Tests for TmuxSession ---

    #[test]
    fn test_start_tmux_session() {
        let cmd_exec = RecordingCmdExec::new();
        // has-session should fail (session doesn't exist) so we don't try to kill
        cmd_exec.fail_run_when_contains("has-session");

        let pty_factory = Arc::new(MockPtyFactory::new());
        let pty_clone = Arc::clone(&pty_factory);

        // We need to wrap the Arc in a struct that implements PtyFactory
        struct ArcPtyFactory(Arc<MockPtyFactory>);
        impl PtyFactory for ArcPtyFactory {
            fn start(&self, cmd: &mut std::process::Command) -> Result<File, TmuxError> {
                self.0.start(cmd)
            }
            fn close(&self) {
                self.0.close()
            }
        }

        // Use "vim" to skip trust prompt polling (which would block on timeout)
        let mut session = TmuxSession::new(
            "test-session",
            "vim",
            Box::new(cmd_exec.clone()),
            Box::new(ArcPtyFactory(pty_factory)),
        );

        session.start("/tmp/workdir").unwrap();

        // Verify exactly 2 PTY commands were created (new-session + attach-session)
        assert_eq!(pty_clone.file_count(), 2);

        // Verify the session has a PTY stored (from attach)
        assert!(session.ptmx.is_some());
        assert!(session.attached);

        // Verify the tmux commands that were run
        let commands = cmd_exec.commands();
        // First command: has-session check
        assert_eq!(commands[0].0, "tmux");
        assert!(commands[0].1.contains(&"has-session".to_string()));
        // has-session failed, so no kill-session should have been issued
        assert_eq!(commands.len(), 1); // Only the has-session check via cmd_exec
    }

    #[test]
    fn test_start_tmux_session_kills_existing() {
        let cmd_exec = RecordingCmdExec::new();
        // has-session succeeds (session exists), so it should be killed

        // Use "vim" to skip trust prompt polling (which would block on timeout)
        let mut session = TmuxSession::new(
            "existing",
            "vim",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.start("/tmp/workdir").unwrap();

        let commands = cmd_exec.commands();
        // Should have: has-session, kill-session
        assert_eq!(commands[0].1[0], "has-session");
        assert_eq!(commands[1].1[0], "kill-session");
    }

    #[test]
    fn test_capture_pane_content() {
        let expected_content = "Hello, world!\nLine 2\n".to_string();
        let cmd_exec = RecordingCmdExec::with_output_responses(vec![expected_content.clone()]);

        let session = TmuxSession::new(
            "test-capture",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        let content = session.capture_pane_content(false).unwrap();
        assert_eq!(content, expected_content);

        let commands = cmd_exec.commands();
        assert_eq!(commands[0].0, "tmux");
        assert!(commands[0].1.contains(&"capture-pane".to_string()));
        assert!(commands[0].1.contains(&"-p".to_string()));
        // Must include -e (ANSI escape sequences) and -J (join wrapped lines)
        assert!(commands[0].1.contains(&"-e".to_string()));
        assert!(commands[0].1.contains(&"-J".to_string()));
        // Should NOT contain -S - when full_history is false
        assert!(!commands[0].1.contains(&"-S".to_string()));
    }

    #[test]
    fn test_capture_pane_content_full_history() {
        let cmd_exec = RecordingCmdExec::with_output_responses(vec!["full history".to_string()]);

        let session = TmuxSession::new(
            "test-history",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        let content = session.capture_pane_content(true).unwrap();
        assert_eq!(content, "full history");

        let commands = cmd_exec.commands();
        assert!(commands[0].1.contains(&"-S".to_string()));
        assert!(commands[0].1.contains(&"-".to_string()));
        // Must include -e (ANSI escape sequences) and -J (join wrapped lines)
        assert!(commands[0].1.contains(&"-e".to_string()));
        assert!(commands[0].1.contains(&"-J".to_string()));
    }

    #[test]
    fn test_has_updated() {
        let cmd_exec = RecordingCmdExec::with_output_responses(vec![
            "content v1".to_string(),
            "content v2".to_string(),
            "content v2".to_string(), // Same as previous
        ]);

        let mut session = TmuxSession::new(
            "test-update",
            "claude",
            Box::new(cmd_exec),
            Box::new(MockPtyFactory::new()),
        );

        // First call: always updated (hash changes from empty)
        assert!(session.has_updated().unwrap());

        // Second call: content changed
        assert!(session.has_updated().unwrap());

        // Third call: same content, no change
        assert!(!session.has_updated().unwrap());
    }

    #[test]
    fn test_has_updated_detects_ai_prompt() {
        let prompt_content = "Some output\nNo, and tell Claude what to do differently\n";
        let cmd_exec = RecordingCmdExec::with_output_responses(vec![
            prompt_content.to_string(),
            prompt_content.to_string(), // Same content but has AI prompt
        ]);

        let mut session = TmuxSession::new(
            "test-prompt",
            "claude",
            Box::new(cmd_exec),
            Box::new(MockPtyFactory::new()),
        );

        // First call: updated due to hash change + prompt
        assert!(session.has_updated().unwrap());

        // Second call: hash same, but AI prompt detected
        assert!(session.has_updated().unwrap());
    }

    #[test]
    fn test_send_keys() {
        let cmd_exec = RecordingCmdExec::new();

        let session = TmuxSession::new(
            "test-keys",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.send_keys("Enter").unwrap();

        let commands = cmd_exec.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "tmux");
        assert_eq!(commands[0].1[0], "send-keys");
        assert!(commands[0].1.contains(&session.sanitized_name.clone()));
        assert!(commands[0].1.contains(&"Enter".to_string()));
    }

    #[test]
    fn test_close_kills_session() {
        let cmd_exec = RecordingCmdExec::new();

        let mut session = TmuxSession::new(
            "test-close",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        // Give the session a PTY to close
        let tmp = tempfile::NamedTempFile::new().unwrap();
        session.ptmx = Some(tmp.into_file());

        session.close().unwrap();

        // PTY should be closed
        assert!(session.ptmx.is_none());

        // kill-session command should have been issued
        let commands = cmd_exec.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "tmux");
        assert_eq!(commands[0].1[0], "kill-session");
        assert!(commands[0].1.contains(&session.sanitized_name.clone()));
    }

    #[test]
    fn test_set_size() {
        let cmd_exec = RecordingCmdExec::new();

        let mut session = TmuxSession::new(
            "test-resize",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.set_size(120, 40).unwrap();

        assert_eq!(session.width, 120);
        assert_eq!(session.height, 40);

        let commands = cmd_exec.commands();
        assert_eq!(commands[0].0, "tmux");
        assert!(commands[0].1.contains(&"resize-window".to_string()));
        assert!(commands[0].1.contains(&"120".to_string()));
        assert!(commands[0].1.contains(&"40".to_string()));
    }

    #[test]
    fn test_cleanup_sessions() {
        let cmd_exec = RecordingCmdExec::with_output_responses(vec![
            format!("{}session1\n{}session2\nother_session\n", TMUX_PREFIX, TMUX_PREFIX),
        ]);

        TmuxSession::cleanup_sessions(&cmd_exec).unwrap();

        let commands = cmd_exec.commands();
        // First: list-sessions
        assert_eq!(commands[0].1[0], "list-sessions");
        // Then kill the two league sessions (not the other one)
        assert_eq!(commands.len(), 3); // list + 2 kills
        assert_eq!(commands[1].1[0], "kill-session");
        assert_eq!(commands[2].1[0], "kill-session");
    }

    #[test]
    fn test_cleanup_sessions_no_server() {
        // When tmux server isn't running, cleanup should succeed silently
        let cmd_exec = RecordingCmdExec::new();
        cmd_exec.fail_run_when_contains("list-sessions");

        // output() is separate from run(), but our mock returns empty by default.
        // We need a CmdExec that fails on output for list-sessions.
        struct FailingOutputExec;
        impl CmdExec for FailingOutputExec {
            fn run(&self, _name: &str, _args: &[String]) -> Result<(), crate::cmd::CmdError> {
                Ok(())
            }
            fn output(
                &self,
                _name: &str,
                _args: &[String],
            ) -> Result<String, crate::cmd::CmdError> {
                Err(crate::cmd::CmdError::Failed(
                    "no server running".to_string(),
                ))
            }
        }

        // Should not error - gracefully handles missing tmux server
        TmuxSession::cleanup_sessions(&FailingOutputExec).unwrap();
    }

    #[test]
    fn test_has_ai_prompt_aider() {
        assert!(TmuxSession::has_ai_prompt(
            "output\n(Y)es/(N)o/(D)on't ask again\n> ",
            "aider"
        ));
        assert!(!TmuxSession::has_ai_prompt("normal output", "aider"));
    }

    #[test]
    fn test_has_ai_prompt_gemini() {
        assert!(TmuxSession::has_ai_prompt(
            "Do you want to proceed? Yes, allow once",
            "gemini"
        ));
        assert!(!TmuxSession::has_ai_prompt("normal output", "gemini"));
    }

    #[test]
    fn test_restore_existing_session() {
        let cmd_exec = RecordingCmdExec::new();

        let mut session = TmuxSession::new(
            "test-restore",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.restore().unwrap();

        // Should have checked has-session
        let commands = cmd_exec.commands();
        assert_eq!(commands[0].1[0], "has-session");

        // Should have a PTY
        assert!(session.ptmx.is_some());
        assert!(session.attached);
    }

    #[test]
    fn test_restore_missing_session() {
        let cmd_exec = RecordingCmdExec::new();
        cmd_exec.fail_run_when_contains("has-session");

        let mut session = TmuxSession::new(
            "test-restore-missing",
            "claude",
            Box::new(cmd_exec),
            Box::new(MockPtyFactory::new()),
        );

        let result = session.restore();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TmuxError::SessionNotFound(_)));
    }

    #[test]
    fn test_detach() {
        let cmd_exec = RecordingCmdExec::new();

        let mut session = TmuxSession::new(
            "test-detach",
            "claude",
            Box::new(cmd_exec),
            Box::new(MockPtyFactory::new()),
        );

        // Give the session an initial PTY
        let tmp = tempfile::NamedTempFile::new().unwrap();
        session.ptmx = Some(tmp.into_file());
        session.attached = true;

        session.detach().unwrap();

        // Should still have a PTY (new one for monitoring)
        assert!(session.ptmx.is_some());
        // But no longer attached
        assert!(!session.attached);
    }

    #[test]
    fn test_capture_pane_includes_ansi_and_join_flags() {
        // Verify that both normal and full_history capture include -e and -J
        let cmd_exec =
            RecordingCmdExec::with_output_responses(vec!["normal".into(), "history".into()]);
        let session = TmuxSession::new(
            "test-flags",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.capture_pane_content(false).unwrap();
        session.capture_pane_content(true).unwrap();

        let commands = cmd_exec.commands();
        // Normal capture (index 0)
        assert!(commands[0].1.contains(&"-e".to_string()), "normal capture missing -e flag");
        assert!(commands[0].1.contains(&"-J".to_string()), "normal capture missing -J flag");
        assert!(!commands[0].1.contains(&"-S".to_string()), "normal capture should not have -S");

        // Full history capture (index 1)
        assert!(commands[1].1.contains(&"-e".to_string()), "full history missing -e flag");
        assert!(commands[1].1.contains(&"-J".to_string()), "full history missing -J flag");
        assert!(commands[1].1.contains(&"-S".to_string()), "full history missing -S flag");
    }

    #[test]
    fn test_handle_trust_prompt_claude_detects_and_sends_enter() {
        // Mock returns the Claude trust prompt on the first capture
        let cmd_exec = RecordingCmdExec::with_output_responses(vec![
            "Welcome to Claude\nDo you trust the files in this folder?\n> ".to_string(),
        ]);

        let session = TmuxSession::new(
            "test-trust",
            "claude",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.handle_trust_prompt().unwrap();

        let commands = cmd_exec.commands();
        // Should have: capture-pane (to detect prompt), then send-keys Enter
        let capture_cmd = commands.iter().find(|(_, args)| args.contains(&"capture-pane".to_string()));
        assert!(capture_cmd.is_some(), "should have captured pane content");

        let send_cmd = commands.iter().find(|(_, args)| args.contains(&"send-keys".to_string()));
        assert!(send_cmd.is_some(), "should have sent keys");
        let send_args = &send_cmd.unwrap().1;
        assert!(send_args.contains(&"Enter".to_string()), "should send Enter for claude");
    }

    #[test]
    fn test_handle_trust_prompt_aider_sends_d_and_enter() {
        let cmd_exec = RecordingCmdExec::with_output_responses(vec![
            "Open documentation url for more info\n".to_string(),
        ]);

        let session = TmuxSession::new(
            "test-trust-aider",
            "aider",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.handle_trust_prompt().unwrap();

        let commands = cmd_exec.commands();
        let send_cmds: Vec<_> = commands
            .iter()
            .filter(|(_, args)| args.contains(&"send-keys".to_string()))
            .collect();
        // Aider sends "d" then "Enter"
        assert_eq!(send_cmds.len(), 2, "aider should send two send-keys commands");
        assert!(send_cmds[0].1.contains(&"d".to_string()), "first key should be 'd'");
        assert!(send_cmds[1].1.contains(&"Enter".to_string()), "second key should be 'Enter'");
    }

    #[test]
    fn test_handle_trust_prompt_unknown_program_skips() {
        let cmd_exec = RecordingCmdExec::new();

        let session = TmuxSession::new(
            "test-trust-unknown",
            "vim",
            Box::new(cmd_exec.clone()),
            Box::new(MockPtyFactory::new()),
        );

        session.handle_trust_prompt().unwrap();

        // No commands should have been issued
        let commands = cmd_exec.commands();
        assert!(commands.is_empty(), "unknown program should skip trust prompt handling");
    }
}
