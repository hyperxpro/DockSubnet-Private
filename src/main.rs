mod ipam;
mod server;
mod storage;
mod types;

use crate::ipam::IpamPlugin;
use crate::server::PluginServer;
use crate::storage::Storage;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "docker_ipam_plugin=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Configuration
    let socket_path = std::env::var("SOCKET_PATH")
        .unwrap_or_else(|_| "/run/docker/plugins/ipam.sock".to_string());

    let state_file = std::env::var("STATE_FILE")
        .unwrap_or_else(|_| "/var/lib/docker-ipam/state.yaml".to_string());

    let default_subnet =
        std::env::var("DEFAULT_SUBNET").unwrap_or_else(|_| "172.18.0.0/16".to_string());

    tracing::info!("Starting Docker IPAM Plugin");
    tracing::info!("Socket path: {}", socket_path);
    tracing::info!("State file: {}", state_file);
    tracing::info!("Default subnet: {}", default_subnet);

    // Initialize storage
    let storage = Arc::new(Storage::new(&state_file).await?);
    tracing::info!("Storage initialized");

    // Initialize IPAM plugin
    let plugin = Arc::new(IpamPlugin::new(storage.clone(), default_subnet));
    tracing::info!("IPAM plugin initialized");

    // Start server
    let server = PluginServer::new(plugin);

    // Check if we should use TCP (for testing) or Unix socket
    if let Ok(tcp_addr) = std::env::var("TCP_ADDR") {
        tracing::warn!("Running in TCP mode (for testing only)");
        server.serve_tcp(&tcp_addr).await?;
    } else {
        server.serve_unix(&socket_path).await?;
    }

    Ok(())
}
