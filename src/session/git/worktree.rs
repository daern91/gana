use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cmd::{args, CmdError, CmdExec};
use crate::config::{get_config_dir, Config};
use crate::session::git::util::sanitize_branch_name;

/// Represents a git worktree associated with a session instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitWorktree {
    pub repo_path: String,
    pub worktree_dir: String,
    pub session_id: String,
    pub branch: String,
    pub base_commit: String,
}

impl GitWorktree {
    /// Create a new GitWorktree by inspecting the git repo at `path`.
    ///
    /// - Resolves `path` to an absolute path
    /// - Finds the git repo root
    /// - Generates a branch name from `title` using the configured branch prefix
    /// - Creates a unique worktree directory path under `{config_dir}/worktrees/`
    /// - Records the current HEAD as the base commit
    pub fn new(
        title: &str,
        path: &str,
        _program: &str,
        session_id: &str,
        cmd: &dyn CmdExec,
    ) -> Result<Self, CmdError> {
        let config = Config::load_default().unwrap_or_default();
        let config_dir = get_config_dir()
            .map_err(|e| CmdError::Failed(format!("failed to get config dir: {}", e)))?;
        Self::new_with_config(title, path, session_id, cmd, &config, &config_dir)
    }

    /// Like `new`, but accepts an explicit config and config directory.
    /// This avoids depending on the home directory, making it suitable for tests.
    pub fn new_with_config(
        title: &str,
        path: &str,
        session_id: &str,
        cmd: &dyn CmdExec,
        config: &Config,
        config_dir: &std::path::Path,
    ) -> Result<Self, CmdError> {
        // Resolve to absolute path
        let abs_path = std::fs::canonicalize(path)
            .map_err(|e| CmdError::Failed(format!("failed to resolve path {}: {}", path, e)))?;
        let abs_path_str = abs_path.to_string_lossy().to_string();

        // Find git repo root
        let repo_path = cmd
            .output("git", &args(&["-C", &abs_path_str, "rev-parse", "--show-toplevel"]))?
            .trim()
            .to_string();

        // Generate branch name
        let sanitized = sanitize_branch_name(title);
        let branch = format!("{}{}", config.branch_prefix, sanitized);

        // Generate unique worktree directory
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let worktree_dir = config_dir
            .join("worktrees")
            .join(format!("{}_{}", session_id, nanos))
            .to_string_lossy()
            .to_string();

        // Get base commit
        let base_commit = cmd
            .output("git", &args(&["-C", &repo_path, "rev-parse", "HEAD"]))?
            .trim()
            .to_string();

        Ok(Self {
            repo_path,
            worktree_dir,
            session_id: session_id.to_string(),
            branch,
            base_commit,
        })
    }

    /// Reconstruct a GitWorktree from previously stored data (e.g. loaded from disk).
    pub fn from_storage(
        repo_path: String,
        worktree_dir: String,
        session_id: String,
        branch: String,
        base_commit: String,
    ) -> Self {
        Self {
            repo_path,
            worktree_dir,
            session_id,
            branch,
            base_commit,
        }
    }

    /// Return the worktree directory path.
    pub fn worktree_path(&self) -> &str {
        &self.worktree_dir
    }

    /// Return the branch name.
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Return the path to the main repository.
    pub fn repo_path(&self) -> &str {
        &self.repo_path
    }

    /// Return just the repository name (last component of the repo path).
    pub fn repo_name(&self) -> &str {
        Path::new(&self.repo_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.repo_path)
    }

    /// Return the base commit SHA.
    pub fn base_commit_sha(&self) -> &str {
        &self.base_commit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_storage_and_accessors() {
        let wt = GitWorktree::from_storage(
            "/home/user/repos/myproject".to_string(),
            "/home/user/.gana/worktrees/sess_123".to_string(),
            "sess".to_string(),
            "gana/my-feature".to_string(),
            "abc123def456".to_string(),
        );

        assert_eq!(wt.worktree_path(), "/home/user/.gana/worktrees/sess_123");
        assert_eq!(wt.branch(), "gana/my-feature");
        assert_eq!(wt.repo_path(), "/home/user/repos/myproject");
        assert_eq!(wt.repo_name(), "myproject");
        assert_eq!(wt.base_commit_sha(), "abc123def456");
    }

    #[test]
    fn test_repo_name_simple_path() {
        let wt = GitWorktree::from_storage(
            "/a/b/c".to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
        assert_eq!(wt.repo_name(), "c");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let wt = GitWorktree::from_storage(
            "/repo".to_string(),
            "/wt".to_string(),
            "sid".to_string(),
            "branch".to_string(),
            "commit".to_string(),
        );

        let json = serde_json::to_string(&wt).unwrap();
        let wt2: GitWorktree = serde_json::from_str(&json).unwrap();

        assert_eq!(wt.repo_path, wt2.repo_path);
        assert_eq!(wt.worktree_dir, wt2.worktree_dir);
        assert_eq!(wt.session_id, wt2.session_id);
        assert_eq!(wt.branch, wt2.branch);
        assert_eq!(wt.base_commit, wt2.base_commit);
    }

    #[test]
    fn test_new_with_real_git_repo() {
        use crate::cmd::SystemCmdExec;
        use crate::config::Config;

        let tmp = setup_test_repo();
        let config_dir = tempfile::TempDir::new().unwrap();
        let cmd = SystemCmdExec;
        let path = tmp.path().to_string_lossy().to_string();
        let config = Config::default();

        let wt = GitWorktree::new_with_config(
            "Test Feature",
            &path,
            "test-sess",
            &cmd,
            &config,
            config_dir.path(),
        )
        .unwrap();

        assert!(!wt.repo_path.is_empty());
        assert!(!wt.worktree_dir.is_empty());
        assert_eq!(wt.session_id, "test-sess");
        assert!(wt.branch.contains("test-feature"));
        assert!(!wt.base_commit.is_empty());
        // base_commit should be a hex SHA
        assert!(wt.base_commit.len() >= 7);
    }

    fn setup_test_repo() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::fs::write(tmp.path().join("test.txt"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        tmp
    }
}
