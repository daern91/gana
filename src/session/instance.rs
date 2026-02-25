use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        }
    }

    /// Update the timestamp to now.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}
