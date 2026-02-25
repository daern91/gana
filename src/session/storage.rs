use super::instance::Instance;
use std::path::Path;
use thiserror::Error;

const INSTANCES_FILE: &str = "instances.json";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to read instances: {0}")]
    ReadFailed(#[from] std::io::Error),
    #[error("failed to parse instances: {0}")]
    ParseFailed(#[from] serde_json::Error),
}

/// Trait for instance persistence, enabling mock storage in tests.
#[cfg_attr(test, mockall::automock)]
pub trait InstanceStorage: Send + Sync {
    fn save_instances(&self, instances: &[Instance]) -> Result<(), StorageError>;
    fn load_instances(&self) -> Result<Vec<Instance>, StorageError>;
}

/// File-based instance storage.
pub struct FileStorage {
    config_dir: std::path::PathBuf,
}

impl FileStorage {
    pub fn new(config_dir: &Path) -> Self {
        Self {
            config_dir: config_dir.to_path_buf(),
        }
    }
}

impl InstanceStorage for FileStorage {
    fn save_instances(&self, instances: &[Instance]) -> Result<(), StorageError> {
        std::fs::create_dir_all(&self.config_dir)?;
        let path = self.config_dir.join(INSTANCES_FILE);
        // Only persist started instances
        let started: Vec<&Instance> = instances.iter().filter(|i| i.started).collect();
        let json = serde_json::to_string_pretty(&started)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    fn load_instances(&self) -> Result<Vec<Instance>, StorageError> {
        let path = self.config_dir.join(INSTANCES_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let contents = std::fs::read_to_string(&path)?;
        let instances: Vec<Instance> = serde_json::from_str(&contents)?;
        Ok(instances)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::instance::{InstanceOptions, InstanceStatus};
    use tempfile::TempDir;

    #[test]
    fn test_storage_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let storage = FileStorage::new(tmp.path());

        let mut instance = Instance::new(InstanceOptions {
            title: "test-session".to_string(),
            path: "/tmp/test".to_string(),
            program: "claude".to_string(),
            auto_yes: false,
        });
        instance.started = true;
        instance.status = InstanceStatus::Running;

        storage.save_instances(&[instance.clone()]).unwrap();
        let loaded = storage.load_instances().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "test-session");
        assert_eq!(loaded[0].status, InstanceStatus::Running);
    }

    #[test]
    fn test_storage_empty() {
        let tmp = TempDir::new().unwrap();
        let storage = FileStorage::new(tmp.path());

        let loaded = storage.load_instances().unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_storage_skips_unstarted() {
        let tmp = TempDir::new().unwrap();
        let storage = FileStorage::new(tmp.path());

        let instance = Instance::new(InstanceOptions {
            title: "not-started".to_string(),
            path: "/tmp/test".to_string(),
            program: "claude".to_string(),
            auto_yes: false,
        });
        // instance.started is false by default

        storage.save_instances(&[instance]).unwrap();
        let loaded = storage.load_instances().unwrap();
        assert!(loaded.is_empty(), "unstarted instances should not be saved");
    }
}
