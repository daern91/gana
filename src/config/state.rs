use serde::{Deserialize, Serialize};
use std::path::Path;

const STATE_FILE_NAME: &str = "state.json";

/// Application state that persists across runs (e.g., help screen visibility).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    /// Bitfield for UI state flags.
    #[serde(default)]
    pub flags: u32,
}

/// Flag: user has seen the help screen.
pub const FLAG_HELP_SEEN: u32 = 1 << 0;

impl AppState {
    pub fn has_flag(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    pub fn set_flag(&mut self, flag: u32) {
        self.flags |= flag;
    }

    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join(STATE_FILE_NAME);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let path = config_dir.join(STATE_FILE_NAME);
        let contents =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e))?;
        std::fs::write(&path, contents)
    }
}
