use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cmd::{CmdExec, SystemCmdExec};
use crate::session::git::{DiffStats, GitWorktree};
use crate::session::tmux::pty::SystemPtyFactory;
use crate::session::tmux::TmuxSession;

/// Status of a session instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceStatus {
    Ready,
    Running,
    Loading,
    Paused,
}

impl std::fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceStatus::Ready => write!(f, "ready"),
            InstanceStatus::Running => write!(f, "running"),
            InstanceStatus::Loading => write!(f, "loading"),
            InstanceStatus::Paused => write!(f, "paused"),
        }
    }
}

/// Options for creating a new Instance.
pub struct InstanceOptions {
    pub title: String,
    pub path: String,
    pub program: String,
    pub auto_yes: bool,
}

/// A session instance that manages a tmux session + git worktree pair.
#[derive(Serialize, Deserialize)]
pub struct Instance {
    pub title: String,
    pub path: String,
    pub branch: String,
    pub status: InstanceStatus,
    pub program: String,
    pub auto_yes: bool,
    pub height: u16,
    pub width: u16,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub started: bool,

    // Runtime-only fields (not serialized)
    #[serde(skip)]
    pub tmux_session: Option<TmuxSession>,
    #[serde(skip)]
    pub git_worktree: Option<GitWorktree>,
    #[serde(skip)]
    pub diff_stats: Option<DiffStats>,
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("title", &self.title)
            .field("path", &self.path)
            .field("branch", &self.branch)
            .field("status", &self.status)
            .field("program", &self.program)
            .field("started", &self.started)
            .field("tmux_session", &self.tmux_session.as_ref().map(|_| "<TmuxSession>"))
            .field("git_worktree", &self.git_worktree)
            .field("diff_stats", &self.diff_stats)
            .finish()
    }
}

impl Clone for Instance {
    fn clone(&self) -> Self {
        Self {
            title: self.title.clone(),
            path: self.path.clone(),
            branch: self.branch.clone(),
            status: self.status,
            program: self.program.clone(),
            auto_yes: self.auto_yes,
            height: self.height,
            width: self.width,
            created_at: self.created_at,
            updated_at: self.updated_at,
            started: self.started,
            // Runtime fields cannot be cloned (TmuxSession has Box<dyn ...>)
            tmux_session: None,
            git_worktree: self.git_worktree.clone(),
            diff_stats: self.diff_stats.clone(),
        }
    }
}

impl Instance {
    /// Create a new instance with the given options.
    pub fn new(opts: InstanceOptions) -> Self {
        let now = Utc::now();
        Self {
            title: opts.title,
            path: opts.path,
            branch: String::new(),
            status: InstanceStatus::Ready,
            program: opts.program,
            auto_yes: opts.auto_yes,
            height: 0,
            width: 0,
            created_at: now,
            updated_at: now,
            started: false,
            tmux_session: None,
            git_worktree: None,
            diff_stats: None,
        }
    }

    /// Update the timestamp to now.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Start the instance: create git worktree + tmux session.
    ///
    /// If `first_time` is true, creates a new worktree and tmux session.
    /// If false (restore), attaches to an existing tmux session.
    pub fn start(&mut self, first_time: bool, cmd: &dyn CmdExec) -> Result<(), anyhow::Error> {
        if first_time {
            // Create GitWorktree
            let worktree =
                GitWorktree::new(&self.title, &self.path, &self.program, &self.title, cmd)?;

            // Set up the worktree on disk
            worktree.setup(cmd)?;

            let worktree_path = worktree.worktree_path().to_string();
            self.branch = worktree.branch().to_string();

            // Create and start TmuxSession
            let mut tmux = TmuxSession::new(
                &self.title,
                &self.program,
                Box::new(SystemCmdExec),
                Box::new(SystemPtyFactory),
            );
            tmux.start(&worktree_path)?;

            self.tmux_session = Some(tmux);
            self.git_worktree = Some(worktree);
            self.status = InstanceStatus::Running;
            self.started = true;
        } else {
            // Restore: attach to existing tmux session
            let mut tmux = TmuxSession::new(
                &self.title,
                &self.program,
                Box::new(SystemCmdExec),
                Box::new(SystemPtyFactory),
            );
            tmux.restore()?;

            self.tmux_session = Some(tmux);
            self.status = InstanceStatus::Running;
        }

        self.touch();
        Ok(())
    }

    /// Kill the instance: cleanup both tmux and git.
    pub fn kill(&mut self, cmd: &dyn CmdExec) -> Result<(), anyhow::Error> {
        // Close tmux session
        if let Some(ref mut tmux) = self.tmux_session {
            tmux.close()?;
        }
        self.tmux_session = None;

        // Cleanup git worktree (removes directory, branch, and prunes)
        if let Some(ref worktree) = self.git_worktree {
            worktree.cleanup(cmd)?;
        }
        self.git_worktree = None;

        self.status = InstanceStatus::Ready;
        self.started = false;
        self.touch();
        Ok(())
    }

    /// Pause: commit changes, remove worktree (keep branch), close tmux.
    pub fn pause(&mut self, cmd: &dyn CmdExec) -> Result<(), anyhow::Error> {
        // Commit any changes with a timestamp message
        if let Some(ref worktree) = self.git_worktree {
            let msg = format!("league: auto-save {}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
            worktree.commit_changes(&msg, cmd)?;

            // Remove worktree directory but keep the branch
            worktree.remove(cmd)?;
        }

        // Close tmux session
        if let Some(ref mut tmux) = self.tmux_session {
            tmux.close()?;
        }
        self.tmux_session = None;

        self.status = InstanceStatus::Paused;
        self.touch();
        Ok(())
    }

    /// Resume: recreate worktree from branch, restart tmux.
    pub fn resume(&mut self, cmd: &dyn CmdExec) -> Result<(), anyhow::Error> {
        // Setup worktree (from existing branch)
        if let Some(ref worktree) = self.git_worktree {
            worktree.setup(cmd)?;

            let worktree_path = worktree.worktree_path().to_string();

            // Start tmux session
            let mut tmux = TmuxSession::new(
                &self.title,
                &self.program,
                Box::new(SystemCmdExec),
                Box::new(SystemPtyFactory),
            );
            tmux.start(&worktree_path)?;

            self.tmux_session = Some(tmux);
        }

        self.status = InstanceStatus::Running;
        self.touch();
        Ok(())
    }

    /// Get preview content from tmux pane.
    pub fn preview(&self) -> Option<String> {
        self.tmux_session
            .as_ref()
            .and_then(|t| t.capture_pane_content(false).ok())
    }

    /// Get full history from tmux pane.
    pub fn preview_full_history(&self) -> Option<String> {
        self.tmux_session
            .as_ref()
            .and_then(|t| t.capture_pane_content(true).ok())
    }

    /// Send a prompt to the session.
    pub fn send_prompt(&self, prompt: &str) {
        if let Some(ref tmux) = self.tmux_session {
            let _ = tmux.send_keys(prompt);
            let _ = tmux.send_keys("Enter");
        }
    }

    /// Send raw keys to the session.
    pub fn send_keys(&self, keys: &str) {
        if let Some(ref tmux) = self.tmux_session {
            let _ = tmux.send_keys(keys);
        }
    }

    /// Check if tmux session has updated content.
    pub fn has_updated(&mut self) -> bool {
        self.tmux_session
            .as_mut()
            .and_then(|t| t.has_updated().ok())
            .unwrap_or(false)
    }

    /// Update diff stats from git.
    pub fn update_diff_stats(&mut self, cmd: &dyn CmdExec) {
        if let Some(ref worktree) = self.git_worktree {
            self.diff_stats = Some(worktree.diff(cmd));
        }
    }

    /// Get cached diff stats.
    pub fn get_diff_stats(&self) -> Option<&DiffStats> {
        self.diff_stats.as_ref()
    }

    /// Check if paused.
    pub fn is_paused(&self) -> bool {
        self.status == InstanceStatus::Paused
    }

    /// Get repo name from git worktree.
    pub fn repo_name(&self) -> Option<String> {
        self.git_worktree.as_ref().map(|w| w.repo_name().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_instance() -> Instance {
        Instance::new(InstanceOptions {
            title: "test-session".to_string(),
            path: "/tmp/test".to_string(),
            program: "claude".to_string(),
            auto_yes: false,
        })
    }

    #[test]
    fn test_instance_creation() {
        let instance = make_instance();
        assert_eq!(instance.status, InstanceStatus::Ready);
        assert!(!instance.started);
        assert!(instance.tmux_session.is_none());
        assert!(instance.git_worktree.is_none());
        assert!(instance.diff_stats.is_none());
        assert_eq!(instance.title, "test-session");
        assert_eq!(instance.program, "claude");
    }

    #[test]
    fn test_instance_pause_status() {
        let mut instance = make_instance();
        assert_eq!(instance.status, InstanceStatus::Ready);
        assert!(!instance.is_paused());

        instance.status = InstanceStatus::Running;
        assert!(!instance.is_paused());

        instance.status = InstanceStatus::Paused;
        assert!(instance.is_paused());

        instance.status = InstanceStatus::Loading;
        assert!(!instance.is_paused());
    }

    #[test]
    fn test_instance_serialization() {
        let mut instance = make_instance();
        instance.started = true;
        instance.status = InstanceStatus::Running;
        instance.branch = "league/test-branch".to_string();

        // Add a git worktree (this IS serializable)
        instance.git_worktree = Some(GitWorktree::from_storage(
            "/repo".to_string(),
            "/worktree".to_string(),
            "sess".to_string(),
            "league/test".to_string(),
            "abc123".to_string(),
        ));

        // Serialize
        let json = serde_json::to_string(&instance).unwrap();

        // Deserialize
        let loaded: Instance = serde_json::from_str(&json).unwrap();

        // Data fields preserved
        assert_eq!(loaded.title, "test-session");
        assert_eq!(loaded.status, InstanceStatus::Running);
        assert_eq!(loaded.branch, "league/test-branch");
        assert!(loaded.started);

        // Runtime fields are None after deserialization
        assert!(loaded.tmux_session.is_none());
        // git_worktree is skipped by serde, so it's None after deserialize
        assert!(loaded.git_worktree.is_none());
        assert!(loaded.diff_stats.is_none());
    }

    #[test]
    fn test_instance_diff_stats() {
        use crate::cmd::MockCmdExec;

        let mut instance = make_instance();
        instance.git_worktree = Some(GitWorktree::from_storage(
            "/repo".to_string(),
            "/worktree".to_string(),
            "sess".to_string(),
            "league/test".to_string(),
            "abc123".to_string(),
        ));

        // No diff stats initially
        assert!(instance.get_diff_stats().is_none());

        // Update diff stats with a mock
        let mut mock = MockCmdExec::new();
        mock.expect_run()
            .withf(|name, args| name == "git" && args.iter().any(|a| a == "-N"))
            .returning(|_, _| Ok(()));
        mock.expect_output()
            .withf(|name, args| name == "git" && args.iter().any(|a| a == "diff"))
            .returning(|_, _| Ok("+added\n-removed\n+another\n".to_string()));

        instance.update_diff_stats(&mock);

        let stats = instance.get_diff_stats().unwrap();
        assert_eq!(stats.added_lines, 2);
        assert_eq!(stats.removed_lines, 1);
        assert!(stats.error.is_none());
    }

    #[test]
    fn test_instance_repo_name() {
        let mut instance = make_instance();

        // No worktree -> None
        assert!(instance.repo_name().is_none());

        // With worktree
        instance.git_worktree = Some(GitWorktree::from_storage(
            "/home/user/repos/myproject".to_string(),
            "/worktree".to_string(),
            "sess".to_string(),
            "league/test".to_string(),
            "abc123".to_string(),
        ));

        assert_eq!(instance.repo_name(), Some("myproject".to_string()));
    }

    #[test]
    fn test_instance_clone_skips_tmux() {
        let mut instance = make_instance();
        instance.git_worktree = Some(GitWorktree::from_storage(
            "/repo".to_string(),
            "/wt".to_string(),
            "s".to_string(),
            "b".to_string(),
            "c".to_string(),
        ));
        instance.status = InstanceStatus::Running;

        let cloned = instance.clone();
        assert_eq!(cloned.title, instance.title);
        assert_eq!(cloned.status, InstanceStatus::Running);
        // tmux_session is not cloneable, so it's None
        assert!(cloned.tmux_session.is_none());
        // git_worktree IS cloned
        assert!(cloned.git_worktree.is_some());
        assert_eq!(
            cloned.git_worktree.as_ref().unwrap().repo_path(),
            "/repo"
        );
    }
}
