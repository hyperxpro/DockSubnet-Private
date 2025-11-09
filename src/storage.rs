use crate::types::IpamState;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::RwLock;

/// Manages persistence of IPAM state to a YAML file
pub struct Storage {
    file_path: PathBuf,
    state: RwLock<IpamState>,
}

impl Storage {
    /// Create a new Storage instance
    pub async fn new(file_path: impl AsRef<Path>) -> Result<Self> {
        let file_path = file_path.as_ref().to_path_buf();

        // Try to load existing state or create new one
        let state = if file_path.exists() {
            let contents = fs::read_to_string(&file_path)
                .await
                .context("Failed to read state file")?;
            serde_yaml::from_str(&contents)
                .context("Failed to parse state file")?
        } else {
            // Create parent directory if it doesn't exist
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            IpamState::default()
        };

        Ok(Self {
            file_path,
            state: RwLock::new(state),
        })
    }

    /// Get a read-only reference to the state
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, IpamState> {
        self.state.read().await
    }

    /// Get a mutable reference to the state
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, IpamState> {
        self.state.write().await
    }

    /// Persist the current state to the YAML file
    pub async fn save(&self) -> Result<()> {
        let state = self.state.read().await;
        let yaml = serde_yaml::to_string(&*state)
            .context("Failed to serialize state")?;

        // Write to a temp file first, then rename for atomicity
        let temp_path = self.file_path.with_extension("tmp");
        fs::write(&temp_path, yaml)
            .await
            .context("Failed to write state file")?;

        fs::rename(&temp_path, &self.file_path)
            .await
            .context("Failed to rename temp file")?;

        tracing::debug!("State saved to {:?}", self.file_path);
        Ok(())
    }

    /// Reload state from disk
    pub async fn reload(&self) -> Result<()> {
        if self.file_path.exists() {
            let contents = fs::read_to_string(&self.file_path)
                .await
                .context("Failed to read state file")?;
            let new_state: IpamState = serde_yaml::from_str(&contents)
                .context("Failed to parse state file")?;

            let mut state = self.state.write().await;
            *state = new_state;
            tracing::debug!("State reloaded from {:?}", self.file_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IpLease, PoolInfo};
    use chrono::Utc;
    use std::net::IpAddr;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_new_creates_default_state() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();
        let state = storage.read().await;

        assert!(state.pools.is_empty());
        assert!(state.leases.is_empty());
    }

    #[tokio::test]
    async fn test_storage_save_and_reload() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        // Create storage and add some data
        let storage = Storage::new(&state_file).await.unwrap();
        {
            let mut state = storage.write().await;
            state.pools.insert(
                "pool-1".to_string(),
                PoolInfo {
                    pool_id: "pool-1".to_string(),
                    subnet: "172.18.0.0/16".to_string(),
                    gateway: None,
                },
            );
            state.leases.push(IpLease {
                ip_address: "172.18.0.2".parse::<IpAddr>().unwrap(),
                container_name: "test-container".to_string(),
                lease_time: Utc::now(),
            });
        }

        // Save to disk
        storage.save().await.unwrap();

        // Create new storage instance from same file
        let storage2 = Storage::new(&state_file).await.unwrap();
        let state = storage2.read().await;

        // Verify data was persisted
        assert_eq!(state.pools.len(), 1);
        assert_eq!(state.leases.len(), 1);
        assert_eq!(state.pools.get("pool-1").unwrap().subnet, "172.18.0.0/16");
        assert_eq!(state.leases[0].container_name, "test-container");
    }

    #[tokio::test]
    async fn test_storage_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();

        // Write data
        {
            let mut state = storage.write().await;
            state.leases.push(IpLease {
                ip_address: "10.0.0.1".parse::<IpAddr>().unwrap(),
                container_name: "container1".to_string(),
                lease_time: Utc::now(),
            });
        }

        // Read data
        {
            let state = storage.read().await;
            assert_eq!(state.leases.len(), 1);
            assert_eq!(state.leases[0].ip_address.to_string(), "10.0.0.1");
        }
    }

    #[tokio::test]
    async fn test_storage_atomic_save() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();
        {
            let mut state = storage.write().await;
            state.leases.push(IpLease {
                ip_address: "192.168.1.1".parse::<IpAddr>().unwrap(),
                container_name: "test".to_string(),
                lease_time: Utc::now(),
            });
        }

        // Save multiple times to test atomicity
        storage.save().await.unwrap();
        storage.save().await.unwrap();

        // Verify temp file is cleaned up
        let temp_file = state_file.with_extension("tmp");
        assert!(!temp_file.exists());

        // Verify state file exists and is valid
        assert!(state_file.exists());
    }
}
