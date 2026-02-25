use std::path::Path;

use crate::cmd::{args, CmdError, CmdExec};

use super::worktree::GitWorktree;

impl GitWorktree {
    /// Set up the worktree on disk.
    ///
    /// If the branch already exists, reuses it. Otherwise creates a new branch
    /// from HEAD.
    pub fn setup(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        let branch_exists = cmd
            .output(
                "git",
                &args(&[
                    "-C",
                    &self.repo_path,
                    "show-ref",
                    &format!("refs/heads/{}", self.branch),
                ]),
            )
            .is_ok();

        if branch_exists {
            self.setup_from_existing_branch(cmd)
        } else {
            self.setup_new_worktree(cmd)
        }
    }

    /// Set up a worktree using an existing branch.
    ///
    /// If a worktree already exists at the target directory, removes it first.
    fn setup_from_existing_branch(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        // Remove existing worktree directory if present
        if Path::new(&self.worktree_dir).exists() {
            let _ = std::fs::remove_dir_all(&self.worktree_dir);
            let _ = cmd.run("git", &args(&["-C", &self.repo_path, "worktree", "prune"]));
        }

        cmd.run(
            "git",
            &args(&[
                "-C",
                &self.repo_path,
                "worktree",
                "add",
                &self.worktree_dir,
                &self.branch,
            ]),
        )
    }

    /// Set up a new worktree with a new branch from HEAD.
    fn setup_new_worktree(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        // Clean up any stale branch refs that might conflict
        let _ = self.cleanup_existing_branch(cmd);

        cmd.run(
            "git",
            &args(&[
                "-C",
                &self.repo_path,
                "worktree",
                "add",
                "-b",
                &self.branch,
                &self.worktree_dir,
                "HEAD",
            ]),
        )
    }

    /// Remove the worktree completely: delete the directory, the branch, and prune.
    pub fn cleanup(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        // Remove the worktree directory
        if Path::new(&self.worktree_dir).exists() {
            std::fs::remove_dir_all(&self.worktree_dir)
                .map_err(|e| CmdError::Failed(format!("remove worktree dir: {}", e)))?;
        }

        // Delete the branch
        let _ = cmd.run(
            "git",
            &args(&["-C", &self.repo_path, "branch", "-D", &self.branch]),
        );

        // Prune stale worktree entries
        self.prune(cmd)
    }

    /// Remove the worktree directory and prune, but keep the branch.
    pub fn remove(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        if Path::new(&self.worktree_dir).exists() {
            std::fs::remove_dir_all(&self.worktree_dir)
                .map_err(|e| CmdError::Failed(format!("remove worktree dir: {}", e)))?;
        }

        self.prune(cmd)
    }

    /// Prune stale worktree entries from the repository.
    pub fn prune(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        cmd.run(
            "git",
            &args(&["-C", &self.repo_path, "worktree", "prune"]),
        )
    }
}

/// Clean up all worktrees in the config directory's worktrees folder.
///
/// Lists all worktrees using `git worktree list --porcelain` and removes each one.
pub fn cleanup_worktrees(config_dir: &str, cmd: &dyn CmdExec) -> Result<(), CmdError> {
    let worktrees_dir = Path::new(config_dir).join("worktrees");
    if !worktrees_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(&worktrees_dir)
        .map_err(|e| CmdError::Failed(format!("read worktrees dir: {}", e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| CmdError::Failed(format!("read dir entry: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            // Try to find the repo this worktree belongs to and prune
            let git_dir = path.join(".git");
            if git_dir.exists() {
                // Read the .git file to find the main repo
                if let Ok(content) = std::fs::read_to_string(&git_dir) {
                    if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                        let gitdir = gitdir.trim();
                        // The gitdir points to .git/worktrees/<name> in the main repo
                        if let Some(main_git) = Path::new(gitdir).parent().and_then(|p| p.parent())
                        {
                            let main_repo = main_git.parent().unwrap_or(main_git);
                            let repo_str = main_repo.to_string_lossy().to_string();
                            let _ = cmd.run(
                                "git",
                                &args(&["-C", &repo_str, "worktree", "prune"]),
                            );
                        }
                    }
                }
            }

            // Remove the directory
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::SystemCmdExec;

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

    #[test]
    fn test_setup_and_cleanup_worktree() {
        let repo = setup_test_repo();
        let cmd = SystemCmdExec;
        let repo_path = repo.path().to_string_lossy().to_string();

        // Get base commit
        let base = cmd
            .output("git", &args(&["-C", &repo_path, "rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string();

        let wt_dir = tempfile::TempDir::new().unwrap();
        let wt_path = wt_dir.path().join("test-worktree");
        let wt_path_str = wt_path.to_string_lossy().to_string();

        let wt = GitWorktree::from_storage(
            repo_path.clone(),
            wt_path_str.clone(),
            "test-sess".to_string(),
            "league/test-wt".to_string(),
            base,
        );

        // Setup the worktree
        wt.setup(&cmd).expect("setup should succeed");
        assert!(
            Path::new(&wt_path_str).exists(),
            "worktree directory should exist after setup"
        );

        // Verify the branch was created
        let branches = cmd
            .output("git", &args(&["-C", &repo_path, "branch"]))
            .unwrap();
        assert!(
            branches.contains("league/test-wt"),
            "branch should be listed"
        );

        // Cleanup the worktree
        wt.cleanup(&cmd).expect("cleanup should succeed");
        assert!(
            !Path::new(&wt_path_str).exists(),
            "worktree directory should be removed after cleanup"
        );
    }

    #[test]
    fn test_remove_keeps_branch() {
        let repo = setup_test_repo();
        let cmd = SystemCmdExec;
        let repo_path = repo.path().to_string_lossy().to_string();

        let base = cmd
            .output("git", &args(&["-C", &repo_path, "rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string();

        let wt_dir = tempfile::TempDir::new().unwrap();
        let wt_path = wt_dir.path().join("test-worktree-keep");
        let wt_path_str = wt_path.to_string_lossy().to_string();

        let wt = GitWorktree::from_storage(
            repo_path.clone(),
            wt_path_str.clone(),
            "test-sess".to_string(),
            "league/keep-branch".to_string(),
            base,
        );

        wt.setup(&cmd).expect("setup should succeed");
        assert!(Path::new(&wt_path_str).exists());

        // Remove (keeps branch)
        wt.remove(&cmd).expect("remove should succeed");
        assert!(!Path::new(&wt_path_str).exists());

        // Branch should still exist
        let branches = cmd
            .output("git", &args(&["-C", &repo_path, "branch"]))
            .unwrap();
        assert!(
            branches.contains("league/keep-branch"),
            "branch should still exist after remove"
        );
    }

    #[test]
    fn test_setup_existing_branch() {
        let repo = setup_test_repo();
        let cmd = SystemCmdExec;
        let repo_path = repo.path().to_string_lossy().to_string();

        let base = cmd
            .output("git", &args(&["-C", &repo_path, "rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_string();

        // First, create the branch manually
        cmd.run(
            "git",
            &args(&["-C", &repo_path, "branch", "league/reuse-branch"]),
        )
        .unwrap();

        let wt_dir = tempfile::TempDir::new().unwrap();
        let wt_path = wt_dir.path().join("test-worktree-reuse");
        let wt_path_str = wt_path.to_string_lossy().to_string();

        let wt = GitWorktree::from_storage(
            repo_path,
            wt_path_str.clone(),
            "test-sess".to_string(),
            "league/reuse-branch".to_string(),
            base,
        );

        // Setup should succeed using the existing branch
        wt.setup(&cmd).expect("setup with existing branch should succeed");
        assert!(Path::new(&wt_path_str).exists());

        wt.cleanup(&cmd).unwrap();
    }
}
