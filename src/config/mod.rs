#[allow(dead_code)]
pub mod state;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

const CONFIG_DIR_NAME: &str = ".league";
const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config directory not found: could not determine home directory")]
    HomeDirNotFound,
    #[error("failed to read config file: {0}")]
    ReadFailed(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    ParseFailed(#[from] serde_json::Error),
    #[error("claude command not found in PATH")]
    ClaudeNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Default program to launch in sessions (e.g. "claude").
    #[serde(default = "default_program")]
    pub default_program: String,

    /// Automatically accept prompts without user confirmation.
    #[serde(default)]
    pub auto_yes: bool,

    /// Daemon polling interval in milliseconds.
    #[serde(default = "default_poll_interval")]
    pub daemon_poll_interval: u64,

    /// Prefix for git branch names created by league.
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,
}

fn default_program() -> String {
    "claude".to_string()
}

fn default_poll_interval() -> u64 {
    1000
}

fn default_branch_prefix() -> String {
    "league/".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_program: default_program(),
            auto_yes: false,
            daemon_poll_interval: default_poll_interval(),
            branch_prefix: default_branch_prefix(),
        }
    }
}

/// Return the config directory path: `~/.league/`
pub fn get_config_dir() -> Result<PathBuf, ConfigError> {
    let home = dirs::home_dir().ok_or(ConfigError::HomeDirNotFound)?;
    Ok(home.join(CONFIG_DIR_NAME))
}

/// Return the config directory, using a custom home for testing.
#[allow(dead_code)]
fn config_dir_from_home(home: &Path) -> PathBuf {
    home.join(CONFIG_DIR_NAME)
}

impl Config {
    /// Load configuration from the default config directory.
    pub fn load_default() -> Result<Self, ConfigError> {
        let dir = get_config_dir()?;
        Self::load(&dir)
    }

    /// Load configuration from the given config directory.
    /// Returns defaults if the file does not exist.
    pub fn load(config_dir: &Path) -> Result<Self, ConfigError> {
        let path = config_dir.join(CONFIG_FILE_NAME);
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)?;
        let config: Config = serde_json::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to the given config directory.
    pub fn save(&self, config_dir: &Path) -> Result<(), ConfigError> {
        std::fs::create_dir_all(config_dir)?;
        let path = config_dir.join(CONFIG_FILE_NAME);
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}

/// Discover the claude command by searching PATH.
#[allow(dead_code)]
pub fn get_claude_command() -> Result<String, ConfigError> {
    // Try to find 'claude' in PATH
    if let Ok(output) = std::process::Command::new("which").arg("claude").output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(path);
        }
    }

    // Try shell-based lookup (handles aliases)
    let shell = std::env::var("SHELL").unwrap_or_default();
    if !shell.is_empty()
        && let Ok(output) = std::process::Command::new(&shell)
            .args(["-ic", "which claude 2>/dev/null || type claude 2>/dev/null"])
            .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Try to parse alias output
        if let Some(path) = parse_alias_output(&text) {
            return Ok(path);
        }
        if !text.is_empty() {
            return Ok(text);
        }
    }

    Err(ConfigError::ClaudeNotFound)
}

/// Parse alias output formats like "claude: aliased to /usr/local/bin/claude"
/// or "claude -> /usr/local/bin/claude".
#[allow(dead_code)]
fn parse_alias_output(text: &str) -> Option<String> {
    let re = regex_lite::Regex::new(r"(?:aliased to|->|=)\s*([^\s]+)").ok()?;
    re.captures(text).map(|caps| caps[1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.default_program.is_empty());
        assert!(!config.auto_yes);
        assert_eq!(config.daemon_poll_interval, 1000);
        assert!(!config.branch_prefix.is_empty());
        assert!(
            config.branch_prefix.ends_with('/'),
            "branch_prefix should end with /"
        );
    }

    #[test]
    fn test_get_config_dir() {
        let dir = get_config_dir().expect("should return config dir");
        assert!(!dir.as_os_str().is_empty());
        assert!(
            dir.ends_with(CONFIG_DIR_NAME),
            "should end with {}",
            CONFIG_DIR_NAME
        );
        assert!(dir.is_absolute(), "should be an absolute path");
    }

    #[test]
    fn test_load_config_missing_file_returns_defaults() {
        let tmp = TempDir::new().unwrap();
        let config = Config::load(tmp.path()).expect("should return defaults");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_load_config_valid_json() {
        let tmp = TempDir::new().unwrap();
        let json = r#"{
            "default_program": "test-claude",
            "auto_yes": true,
            "daemon_poll_interval": 2000,
            "branch_prefix": "test/"
        }"#;
        std::fs::write(tmp.path().join(CONFIG_FILE_NAME), json).unwrap();

        let config = Config::load(tmp.path()).expect("should load config");
        assert_eq!(config.default_program, "test-claude");
        assert!(config.auto_yes);
        assert_eq!(config.daemon_poll_interval, 2000);
        assert_eq!(config.branch_prefix, "test/");
    }

    #[test]
    fn test_load_config_invalid_json_returns_error() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(CONFIG_FILE_NAME), "not json at all").unwrap();

        let result = Config::load(tmp.path());
        assert!(result.is_err(), "invalid JSON should return error");
    }

    #[test]
    fn test_save_config_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let config = Config {
            default_program: "test-claude".to_string(),
            auto_yes: true,
            daemon_poll_interval: 500,
            branch_prefix: "custom/".to_string(),
        };

        config.save(tmp.path()).expect("should save config");

        // Verify file exists
        assert!(tmp.path().join(CONFIG_FILE_NAME).exists());

        // Load it back and compare
        let loaded = Config::load(tmp.path()).expect("should load saved config");
        assert_eq!(config, loaded);
    }

    #[test]
    #[ignore] // Modifies process-global PATH/SHELL env vars, unsafe for parallel execution
    fn test_get_claude_command_missing() {
        // With an empty PATH, claude should not be found
        let original_path = std::env::var("PATH").unwrap_or_default();
        let original_shell = std::env::var("SHELL").ok();

        let tmp = TempDir::new().unwrap();
        // SAFETY: this test must be run in isolation (marked #[ignore])
        // because modifying env vars affects all threads.
        unsafe {
            std::env::set_var("PATH", tmp.path());
            std::env::remove_var("SHELL");
        }

        let result = get_claude_command();
        assert!(result.is_err(), "should fail when claude is not in PATH");

        // Restore
        unsafe {
            std::env::set_var("PATH", &original_path);
            if let Some(shell) = original_shell {
                std::env::set_var("SHELL", &shell);
            }
        }
    }

    #[test]
    fn test_parse_alias_output() {
        assert_eq!(
            parse_alias_output("claude: aliased to /usr/local/bin/claude"),
            Some("/usr/local/bin/claude".to_string())
        );
        assert_eq!(
            parse_alias_output("claude -> /usr/local/bin/claude"),
            Some("/usr/local/bin/claude".to_string())
        );
        assert_eq!(parse_alias_output("/usr/local/bin/claude"), None);
    }
}
