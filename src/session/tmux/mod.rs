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
            args(&["capture-pane", "-t", &self.sanitized_name, "-p", "-S", "-"])
        } else {
            args(&["capture-pane", "-t", &self.sanitized_name, "-p"])
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

        let mut session = TmuxSession::new(
            "test-session",
            "claude",
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

        let mut session = TmuxSession::new(
            "existing",
            "claude",
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
}
