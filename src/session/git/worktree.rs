use serde::{Deserialize, Serialize};

/// Represents a git worktree associated with a session instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitWorktree {
    pub repo_path: String,
    pub worktree_dir: String,
    pub session_id: String,
    pub branch: String,
    pub base_commit: String,
}
