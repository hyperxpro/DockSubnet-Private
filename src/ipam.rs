use crate::storage::Storage;
use crate::types::*;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use ipnetwork::IpNetwork;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

/// The IPAM Plugin implementation
pub struct IpamPlugin {
    storage: Arc<Storage>,
    default_subnet: String,
}

impl IpamPlugin {
    pub fn new(storage: Arc<Storage>, default_subnet: String) -> Self {
        Self {
            storage,
            default_subnet,
        }
    }

    /// Handle GetCapabilities request
    pub async fn get_capabilities(&self) -> Result<CapabilitiesResponse> {
        Ok(CapabilitiesResponse {
            requires_mac_address: false,
            requires_request_replay: false,
        })
    }

    /// Handle RequestPool request
    pub async fn request_pool(&self, req: RequestPoolRequest) -> Result<RequestPoolResponse> {
        let pool = req.pool.unwrap_or_else(|| self.default_subnet.clone());
        let pool_id = format!("pool-{}", uuid::Uuid::new_v4());

        // Validate the pool is a valid CIDR
        pool.parse::<IpNetwork>()
            .context("Invalid subnet format")?;

        // Store pool info
        let pool_info = PoolInfo {
            pool_id: pool_id.clone(),
            subnet: pool.clone(),
            gateway: None,
        };

        {
            let mut state = self.storage.write().await;
            state.pools.insert(pool_id.clone(), pool_info);
        }
        self.storage.save().await?;

        tracing::info!("Pool requested: {} -> {}", pool_id, pool);

        Ok(RequestPoolResponse {
            pool_id,
            pool,
            data: HashMap::new(),
        })
    }

    /// Handle ReleasePool request
    pub async fn release_pool(&self, req: ReleasePoolRequest) -> Result<()> {
        {
            let mut state = self.storage.write().await;
            // Get the pool info before removing it
            let pool_subnet = state.pools.get(&req.pool_id).map(|p| p.subnet.clone());
            state.pools.remove(&req.pool_id);

            // Also remove all leases from this pool
            if let Some(subnet) = pool_subnet {
                if let Ok(network) = subnet.parse::<IpNetwork>() {
                    state.leases.retain(|lease| !network.contains(lease.ip_address));
                }
            }
        }
        self.storage.save().await?;

        tracing::info!("Pool released: {}", req.pool_id);
        Ok(())
    }

    /// Handle RequestAddress request
    pub async fn request_address(&self, req: RequestAddressRequest) -> Result<RequestAddressResponse> {
        let pool_info = {
            let state = self.storage.read().await;
            state
                .pools
                .get(&req.pool_id)
                .cloned()
                .ok_or_else(|| anyhow!("Pool not found: {}", req.pool_id))?
        };

        let network: IpNetwork = pool_info
            .subnet
            .parse()
            .context("Invalid subnet in pool")?;

        // Extract container name from options
        let container_name = req
            .options
            .as_ref()
            .and_then(|opts| opts.get("com.docker.network.endpoint.name"))
            .or_else(|| {
                req.options
                    .as_ref()
                    .and_then(|opts| opts.get("container_name"))
            })
            .or_else(|| {
                req.options
                    .as_ref()
                    .and_then(|opts| opts.get("com.docker.network.container.id"))
            })
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        // If a specific address is requested, use it
        let ip_addr = if let Some(requested_addr) = req.address {
            requested_addr.parse::<IpAddr>()
                .context("Invalid IP address format")?
        } else {
            // Allocate next available IP
            self.allocate_next_ip(&network).await?
        };

        // Ensure the IP is within the network
        if !network.contains(ip_addr) {
            return Err(anyhow!("IP address {} is not in subnet {}", ip_addr, network));
        }

        // Create the lease
        let lease = IpLease {
            ip_address: ip_addr,
            container_name: container_name.clone(),
            lease_time: Utc::now(),
        };

        // Store the lease
        {
            let mut state = self.storage.write().await;
            // Remove any existing lease for this IP
            state.leases.retain(|l| l.ip_address != ip_addr);
            state.leases.push(lease);
        }
        self.storage.save().await?;

        let cidr_prefix = network.prefix();
        let address_with_cidr = format!("{}/{}", ip_addr, cidr_prefix);

        tracing::info!(
            "Address allocated: {} to container '{}' (pool: {})",
            address_with_cidr,
            container_name,
            req.pool_id
        );

        Ok(RequestAddressResponse {
            address: address_with_cidr,
            data: HashMap::new(),
        })
    }

    /// Handle ReleaseAddress request
    pub async fn release_address(&self, req: ReleaseAddressRequest) -> Result<()> {
        // Parse the address (might have CIDR notation)
        let ip_str = req.address.split('/').next().unwrap_or(&req.address);
        let ip_addr: IpAddr = ip_str.parse()
            .context("Invalid IP address format")?;

        {
            let mut state = self.storage.write().await;
            let initial_len = state.leases.len();
            state.leases.retain(|lease| lease.ip_address != ip_addr);
            let removed = initial_len - state.leases.len();

            if removed > 0 {
                tracing::info!("Address released: {} (pool: {})", ip_addr, req.pool_id);
            } else {
                tracing::warn!("Address not found for release: {}", ip_addr);
            }
        }
        self.storage.save().await?;

        Ok(())
    }

    /// Allocate the next available IP in the network
    async fn allocate_next_ip(&self, network: &IpNetwork) -> Result<IpAddr> {
        let state = self.storage.read().await;

        // Get all allocated IPs
        let allocated: std::collections::HashSet<IpAddr> = state
            .leases
            .iter()
            .filter(|lease| network.contains(lease.ip_address))
            .map(|lease| lease.ip_address)
            .collect();

        // Find first available IP (skip network address and broadcast)
        for ip in network.iter().skip(1) {
            // Skip the last IP if it's IPv4 (broadcast)
            if let IpAddr::V4(_) = ip {
                if ip == network.broadcast() {
                    continue;
                }
            }

            if !allocated.contains(&ip) {
                return Ok(ip);
            }
        }

        Err(anyhow!("No available IP addresses in subnet {}", network))
    }
}

// UUID generation helper (simple implementation)
mod uuid {
    use std::fmt;

    pub struct Uuid(u128);

    impl Uuid {
        pub fn new_v4() -> Self {
            use std::time::{SystemTime, UNIX_EPOCH};
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(nanos)
        }
    }

    impl fmt::Display for Uuid {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{:032x}", self.0)
        }
    }
}
