use crate::cmd::{args, CmdExec};

use super::worktree::GitWorktree;

/// Statistics from a git diff.
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub content: String,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub error: Option<String>,
}

impl DiffStats {
    /// Count added and removed lines from a unified diff string.
    ///
    /// Lines starting with "+" (but not "+++") count as added.
    /// Lines starting with "-" (but not "---") count as removed.
    pub fn from_diff(content: String) -> Self {
        let mut added = 0;
        let mut removed = 0;

        for line in content.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                added += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                removed += 1;
            }
        }

        Self {
            content,
            added_lines: added,
            removed_lines: removed,
            error: None,
        }
    }
}

impl GitWorktree {
    /// Compute a diff between the worktree and the base commit.
    ///
    /// 1. Stages untracked files with `git add -N .` (intent-to-add)
    /// 2. Runs `git diff {base_commit}` in the worktree
    /// 3. Parses the output to count added/removed lines
    pub fn diff(&self, cmd: &dyn CmdExec) -> DiffStats {
        // Stage untracked files so they appear in the diff
        if let Err(e) = cmd.run(
            "git",
            &args(&["-C", &self.worktree_dir, "add", "-N", "."]),
        ) {
            return DiffStats {
                error: Some(format!("failed to stage untracked files: {}", e)),
                ..Default::default()
            };
        }

        // Run the diff
        let diff_output = cmd.output(
            "git",
            &args(&[
                "-C",
                &self.worktree_dir,
                "--no-pager",
                "diff",
                &self.base_commit,
            ]),
        );

        match diff_output {
            Ok(output) => DiffStats::from_diff(output),
            Err(e) => DiffStats {
                error: Some(format!("failed to run diff: {}", e)),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_stats_from_empty_diff() {
        let stats = DiffStats::from_diff(String::new());
        assert_eq!(stats.added_lines, 0);
        assert_eq!(stats.removed_lines, 0);
        assert!(stats.content.is_empty());
        assert!(stats.error.is_none());
    }

    #[test]
    fn test_diff_stats_counts_correctly() {
        let diff = r#"diff --git a/file.rs b/file.rs
index abc123..def456 100644
--- a/file.rs
+++ b/file.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!("old");
+    println!("new");
+    println!("extra");
 }
"#;
        let stats = DiffStats::from_diff(diff.to_string());
        assert_eq!(stats.added_lines, 2);
        assert_eq!(stats.removed_lines, 1);
        assert_eq!(stats.content, diff);
    }

    #[test]
    fn test_diff_stats_ignores_header_markers() {
        // "+++" and "---" lines should NOT be counted as added/removed
        let diff = "--- a/file.rs\n+++ b/file.rs\n+added\n-removed\n";
        let stats = DiffStats::from_diff(diff.to_string());
        assert_eq!(stats.added_lines, 1);
        assert_eq!(stats.removed_lines, 1);
    }

    #[test]
    fn test_diff_stats_multiple_files() {
        let diff = r#"diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
+line1
+line2
-old1
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
+new1
+new2
+new3
-removed1
-removed2
"#;
        let stats = DiffStats::from_diff(diff.to_string());
        assert_eq!(stats.added_lines, 5);
        assert_eq!(stats.removed_lines, 3);
    }

    #[test]
    fn test_diff_with_mock_cmd() {
        use crate::cmd::MockCmdExec;

        let wt = GitWorktree::from_storage(
            "/repo".to_string(),
            "/worktree".to_string(),
            "sess".to_string(),
            "league/test".to_string(),
            "abc123".to_string(),
        );

        let mut mock = MockCmdExec::new();

        // git add -N .
        mock.expect_run()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "-N")
            })
            .returning(|_, _| Ok(()));

        // git diff
        mock.expect_output()
            .withf(|name, cmd_args| {
                name == "git" && cmd_args.iter().any(|a| a == "diff")
            })
            .returning(|_, _| Ok("+added line\n-removed line\n".to_string()));

        let stats = wt.diff(&mock);
        assert_eq!(stats.added_lines, 1);
        assert_eq!(stats.removed_lines, 1);
        assert!(stats.error.is_none());
    }

    #[test]
    fn test_diff_stage_error() {
        use crate::cmd::{CmdError, MockCmdExec};

        let wt = GitWorktree::from_storage(
            "/repo".to_string(),
            "/worktree".to_string(),
            "sess".to_string(),
            "league/test".to_string(),
            "abc123".to_string(),
        );

        let mut mock = MockCmdExec::new();

        // git add -N . fails
        mock.expect_run()
            .returning(|_, _| Err(CmdError::Failed("not a repo".to_string())));

        let stats = wt.diff(&mock);
        assert!(stats.error.is_some());
        assert_eq!(stats.added_lines, 0);
        assert_eq!(stats.removed_lines, 0);
    }
}
