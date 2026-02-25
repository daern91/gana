use crate::cmd::{args, CmdError, CmdExec};

/// Sanitize a string for use as a git branch name.
///
/// Rules applied:
/// - Convert to lowercase
/// - Replace spaces with hyphens
/// - Remove disallowed characters (keep alphanumeric, /, _, ., -)
/// - Collapse multiple consecutive hyphens to a single hyphen
/// - Remove leading/trailing hyphens and slashes
pub fn sanitize_branch_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }

    let lowered = name.to_lowercase();

    // Replace spaces with hyphens, remove disallowed chars
    let mut result = String::with_capacity(lowered.len());
    for c in lowered.chars() {
        if c.is_alphanumeric() || c == '/' || c == '_' || c == '.' || c == '-' {
            result.push(c);
        } else if c == ' ' {
            result.push('-');
        }
        // else: discard the character
    }

    // Collapse multiple consecutive hyphens
    let mut collapsed = String::with_capacity(result.len());
    let mut prev_hyphen = false;
    for c in result.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push(c);
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }

    // Trim leading/trailing hyphens and slashes
    let trimmed = collapsed.trim_matches(|c: char| c == '-' || c == '/');
    trimmed.to_string()
}

/// Check if `gh` CLI is available.
#[allow(dead_code)]
pub fn check_gh_cli(cmd: &dyn CmdExec) -> Result<(), CmdError> {
    cmd.run("gh", &args(&["--version"]))
}

/// Check if the given path is inside a git repository.
#[allow(dead_code)]
pub fn is_git_repo(cmd: &dyn CmdExec, path: &str) -> bool {
    cmd.run(
        "git",
        &args(&["-C", path, "rev-parse", "--is-inside-work-tree"]),
    )
    .is_ok()
}

/// Find the root of the git repository containing the given path.
#[allow(dead_code)]
pub fn find_git_repo_root(cmd: &dyn CmdExec, path: &str) -> Result<String, CmdError> {
    cmd.output(
        "git",
        &args(&["-C", path, "rev-parse", "--show-toplevel"]),
    )
    .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_branch_name() {
        let cases = vec![
            ("feature", "feature"),
            ("new feature branch", "new-feature-branch"),
            ("FeAtUrE BrAnCh", "feature-branch"),
            ("feature!@#$%^&*()", "feature"),
            ("feature/sub_branch.v1", "feature/sub_branch.v1"),
            ("feature---branch", "feature-branch"),
            ("-feature-branch-", "feature-branch"),
            ("/feature/branch/", "feature/branch"),
            ("", ""),
            (
                "USER/Feature Branch!@#$%^&*()/v1.0",
                "user/feature-branch/v1.0",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                sanitize_branch_name(input),
                expected,
                "sanitize_branch_name({:?}) should be {:?}",
                input,
                expected
            );
        }
    }
}
