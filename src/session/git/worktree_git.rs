use crate::cmd::{args, CmdError, CmdExec};

use super::worktree::GitWorktree;

impl GitWorktree {
    /// Execute a git command in the given directory and return the trimmed output.
    fn run_git_command(
        cmd: &dyn CmdExec,
        path: &str,
        git_args: &[&str],
    ) -> Result<String, CmdError> {
        let mut full_args = vec!["-C", path];
        full_args.extend_from_slice(git_args);
        cmd.output("git", &args(&full_args))
            .map(|s| s.trim().to_string())
    }

    /// Push changes: stage all, commit, and push to remote.
    ///
    /// First tries `gh repo sync`, falling back to `git push -u origin {branch}`.
    pub fn push_changes(&self, title: &str, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        // Stage all changes
        cmd.run("git", &args(&["-C", &self.worktree_dir, "add", "."]))?;

        // Commit
        cmd.run(
            "git",
            &args(&[
                "-C",
                &self.worktree_dir,
                "commit",
                "--no-verify",
                "-m",
                title,
            ]),
        )?;

        // Try gh repo sync first, fallback to git push
        if cmd
            .run(
                "gh",
                &args(&["-C", &self.worktree_dir, "repo", "sync"]),
            )
            .is_err()
        {
            cmd.run(
                "git",
                &args(&[
                    "-C",
                    &self.worktree_dir,
                    "push",
                    "-u",
                    "origin",
                    &self.branch,
                ]),
            )?;
        }

        Ok(())
    }

    /// Commit changes if the worktree is dirty.
    ///
    /// Stages all files and commits with the given title.
    /// Returns Ok(()) if no changes to commit.
    pub fn commit_changes(&self, title: &str, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        if !self.is_dirty(cmd)? {
            return Ok(());
        }

        cmd.run("git", &args(&["-C", &self.worktree_dir, "add", "."]))?;

        cmd.run(
            "git",
            &args(&[
                "-C",
                &self.worktree_dir,
                "commit",
                "--no-verify",
                "-m",
                title,
            ]),
        )
    }

    /// Check if the worktree has any uncommitted changes.
    pub fn is_dirty(&self, cmd: &dyn CmdExec) -> Result<bool, CmdError> {
        let output = Self::run_git_command(cmd, &self.worktree_dir, &["status", "--porcelain"])?;
        Ok(!output.is_empty())
    }

    /// Check if the branch is currently checked out in the main repo.
    pub fn is_branch_checked_out(&self, cmd: &dyn CmdExec) -> Result<bool, CmdError> {
        let head_ref =
            Self::run_git_command(cmd, &self.repo_path, &["symbolic-ref", "HEAD"])?;
        Ok(head_ref == format!("refs/heads/{}", self.branch))
    }

    /// Open the branch in the browser using `gh browse`.
    pub fn open_branch_url(&self, cmd: &dyn CmdExec) -> Result<(), CmdError> {
        cmd.run("gh", &args(&["browse", "-b", &self.branch]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::MockCmdExec;

    fn make_worktree() -> GitWorktree {
        GitWorktree::from_storage(
            "/repo".to_string(),
            "/worktree".to_string(),
            "sess".to_string(),
            "league/test".to_string(),
            "abc123".to_string(),
        )
    }

    #[test]
    fn test_is_dirty_with_changes() {
        let wt = make_worktree();
        let mut mock = MockCmdExec::new();
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "--porcelain")
            })
            .returning(|_, _| Ok(" M file.rs\n".to_string()));

        assert!(wt.is_dirty(&mock).unwrap());
    }

    #[test]
    fn test_is_dirty_clean() {
        let wt = make_worktree();
        let mut mock = MockCmdExec::new();
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "--porcelain")
            })
            .returning(|_, _| Ok(String::new()));

        assert!(!wt.is_dirty(&mock).unwrap());
    }

    #[test]
    fn test_is_branch_checked_out_yes() {
        let wt = make_worktree();
        let mut mock = MockCmdExec::new();
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "symbolic-ref")
            })
            .returning(|_, _| Ok("refs/heads/league/test\n".to_string()));

        assert!(wt.is_branch_checked_out(&mock).unwrap());
    }

    #[test]
    fn test_is_branch_checked_out_no() {
        let wt = make_worktree();
        let mut mock = MockCmdExec::new();
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "symbolic-ref")
            })
            .returning(|_, _| Ok("refs/heads/main\n".to_string()));

        assert!(!wt.is_branch_checked_out(&mock).unwrap());
    }

    #[test]
    fn test_commit_changes_when_clean() {
        let wt = make_worktree();
        let mut mock = MockCmdExec::new();
        // is_dirty check returns clean
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "--porcelain")
            })
            .returning(|_, _| Ok(String::new()));

        // Should succeed without calling add or commit
        wt.commit_changes("test commit", &mock).unwrap();
    }

    #[test]
    fn test_commit_changes_when_dirty() {
        let wt = make_worktree();
        let mut mock = MockCmdExec::new();

        // is_dirty check returns dirty
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "--porcelain")
            })
            .returning(|_, _| Ok("M file.rs\n".to_string()));

        // Expect git add .
        mock.expect_run()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "add")
            })
            .returning(|_, _| Ok(()));

        // Expect git commit
        mock.expect_run()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "commit")
            })
            .returning(|_, _| Ok(()));

        wt.commit_changes("test commit", &mock).unwrap();
    }
}
