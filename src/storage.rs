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
            serde_yaml::from_str(&contents).context("Failed to parse state file")?
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
        let yaml = serde_yaml::to_string(&*state).context("Failed to serialize state")?;

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
    #[allow(dead_code)]
    pub async fn reload(&self) -> Result<()> {
        if self.file_path.exists() {
            let contents = fs::read_to_string(&self.file_path)
                .await
                .context("Failed to read state file")?;
            let new_state: IpamState =
                serde_yaml::from_str(&contents).context("Failed to parse state file")?;

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
    use std::sync::Arc;
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

    #[tokio::test]
    async fn test_storage_concurrent_reads() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Arc::new(Storage::new(&state_file).await.unwrap());

        // Add some data
        {
            let mut state = storage.write().await;
            state.leases.push(IpLease {
                ip_address: "10.0.0.1".parse::<IpAddr>().unwrap(),
                container_name: "container1".to_string(),
                lease_time: Utc::now(),
            });
        }
        storage.save().await.unwrap();

        // Spawn multiple concurrent readers
        let mut handles = vec![];
        for _ in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                let state = storage_clone.read().await;
                assert_eq!(state.leases.len(), 1);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_storage_file_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();
        {
            let mut state = storage.write().await;
            state.leases.push(IpLease {
                ip_address: "10.0.0.2".parse::<IpAddr>().unwrap(),
                container_name: "test".to_string(),
                lease_time: Utc::now(),
            });
        }
        storage.save().await.unwrap();

        // Verify file exists and is readable
        assert!(state_file.exists());
        let contents = tokio::fs::read_to_string(&state_file).await.unwrap();
        assert!(!contents.is_empty());
    }

    #[tokio::test]
    async fn test_storage_empty_state_serialization() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();
        storage.save().await.unwrap();

        // Read the file and verify it's valid YAML
        let contents = tokio::fs::read_to_string(&state_file).await.unwrap();
        let parsed: IpamState = serde_yaml::from_str(&contents).unwrap();
        assert_eq!(parsed.pools.len(), 0);
        assert_eq!(parsed.leases.len(), 0);
    }

    #[tokio::test]
    async fn test_storage_reload_method() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();

        // Add some data and save
        {
            let mut state = storage.write().await;
            state.leases.push(IpLease {
                ip_address: "10.0.0.3".parse::<IpAddr>().unwrap(),
                container_name: "container-reload".to_string(),
                lease_time: Utc::now(),
            });
        }
        storage.save().await.unwrap();

        // Modify in-memory state
        {
            let mut state = storage.write().await;
            state.leases.clear();
        }

        // Reload from disk
        storage.reload().await.unwrap();

        // Verify data is restored
        let state = storage.read().await;
        assert_eq!(state.leases.len(), 1);
        assert_eq!(state.leases[0].container_name, "container-reload");
    }

    #[tokio::test]
    async fn test_storage_with_pools_and_leases() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("state.yaml");

        let storage = Storage::new(&state_file).await.unwrap();
        {
            let mut state = storage.write().await;

            // Add pool
            state.pools.insert(
                "pool-1".to_string(),
                PoolInfo {
                    pool_id: "pool-1".to_string(),
                    subnet: "192.168.1.0/24".to_string(),
                    gateway: Some("192.168.1.1".to_string()),
                },
            );

            // Add leases
            state.leases.push(IpLease {
                ip_address: "192.168.1.10".parse::<IpAddr>().unwrap(),
                container_name: "container1".to_string(),
                lease_time: Utc::now(),
            });
            state.leases.push(IpLease {
                ip_address: "192.168.1.11".parse::<IpAddr>().unwrap(),
                container_name: "container2".to_string(),
                lease_time: Utc::now(),
            });
        }

        // Save and reload
        storage.save().await.unwrap();
        let new_storage = Storage::new(&state_file).await.unwrap();

        // Verify all data persisted
        let state = new_storage.read().await;
        assert_eq!(state.pools.len(), 1);
        assert_eq!(state.leases.len(), 2);
        assert!(state.pools.contains_key("pool-1"));
        assert_eq!(
            state.pools.get("pool-1").unwrap().gateway,
            Some("192.168.1.1".to_string())
        );
    }
}
