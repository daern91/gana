use crate::cmd::{args, CmdExec};

use super::worktree::GitWorktree;

impl GitWorktree {
    /// Clean up an existing branch so a fresh worktree can be created.
    ///
    /// This attempts to:
    /// 1. Delete the branch ref (`refs/heads/{branch}`)
    /// 2. Remove any worktree refs for the branch
    /// 3. Clean up git config entries for the branch
    ///
    /// Errors are accumulated and returned as a single combined error.
    pub fn cleanup_existing_branch(&self, cmd: &dyn CmdExec) -> Result<(), String> {
        let mut errors: Vec<String> = Vec::new();

        // Delete the branch ref
        if let Err(e) = cmd.run(
            "git",
            &args(&[
                "-C",
                &self.repo_path,
                "update-ref",
                "-d",
                &format!("refs/heads/{}", self.branch),
            ]),
        ) {
            errors.push(format!("delete branch ref: {}", e));
        }

        // Remove worktree-specific refs
        if let Err(e) = cmd.run(
            "git",
            &args(&[
                "-C",
                &self.repo_path,
                "update-ref",
                "-d",
                &format!("refs/worktree/{}", self.branch),
            ]),
        ) {
            // This is expected to fail if there's no worktree ref; only record
            // if it's a surprising failure.
            let msg = e.to_string();
            if !msg.contains("not found") && !msg.contains("does not exist") {
                errors.push(format!("delete worktree ref: {}", e));
            }
        }

        // Remove branch config section
        if let Err(e) = cmd.run(
            "git",
            &args(&[
                "-C",
                &self.repo_path,
                "config",
                "--remove-section",
                &format!("branch.{}", self.branch),
            ]),
        ) {
            let msg = e.to_string();
            if !msg.contains("No such section") && !msg.contains("does not exist") {
                errors.push(format!("remove branch config: {}", e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(combine_errors(&errors))
        }
    }
}

/// Combine multiple error messages into a single string.
pub fn combine_errors(errors: &[String]) -> String {
    errors.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_errors_empty() {
        assert_eq!(combine_errors(&[]), "");
    }

    #[test]
    fn test_combine_errors_single() {
        let errors = vec!["something failed".to_string()];
        assert_eq!(combine_errors(&errors), "something failed");
    }

    #[test]
    fn test_combine_errors_multiple() {
        let errors = vec![
            "first error".to_string(),
            "second error".to_string(),
            "third error".to_string(),
        ];
        assert_eq!(
            combine_errors(&errors),
            "first error; second error; third error"
        );
    }

    #[test]
    fn test_cleanup_existing_branch_with_mock() {
        use crate::cmd::MockCmdExec;

        let wt = GitWorktree::from_storage(
            "/repo".to_string(),
            "/wt".to_string(),
            "sid".to_string(),
            "gana/test-branch".to_string(),
            "abc123".to_string(),
        );

        let mut mock = MockCmdExec::new();
        // All three commands succeed
        mock.expect_run().times(3).returning(|_, _| Ok(()));

        let result = wt.cleanup_existing_branch(&mock);
        assert!(result.is_ok());
    }
}
