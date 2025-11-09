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
