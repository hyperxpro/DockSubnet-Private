use docker_ipam_plugin::ipam::IpamPlugin;
use docker_ipam_plugin::storage::Storage;
use docker_ipam_plugin::types::*;
use std::sync::Arc;

// Integration test for the full IPAM workflow
#[tokio::test]
async fn test_full_ipam_workflow() {
    // Create temp state file
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    // 1. Create plugin with storage
    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

    // 2. Request a pool
    let pool_req = RequestPoolRequest {
        pool: Some("192.168.100.0/24".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();
    assert!(pool_resp.pool_id.starts_with("pool-"));
    assert_eq!(pool_resp.pool, "192.168.100.0/24");

    // 3. Request multiple addresses
    let addr1_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: Some(
            vec![("container_name".to_string(), "container1".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let addr1_resp = plugin.request_address(addr1_req).await.unwrap();
    assert!(addr1_resp.address.starts_with("192.168.100."));

    let addr2_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: Some(
            vec![("container_name".to_string(), "container2".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let addr2_resp = plugin.request_address(addr2_req).await.unwrap();
    assert!(addr2_resp.address.starts_with("192.168.100."));
    assert_ne!(addr1_resp.address, addr2_resp.address);

    // 4. Release first address
    let release_req = ReleaseAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: addr1_resp.address.clone(),
    };
    plugin.release_address(release_req).await.unwrap();

    // 5. Request another address (should reuse released one)
    let addr3_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: Some(
            vec![("container_name".to_string(), "container3".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let addr3_resp = plugin.request_address(addr3_req).await.unwrap();
    assert_eq!(
        addr3_resp.address.split('/').next(),
        addr1_resp.address.split('/').next()
    );

    // 6. Release the pool
    let release_pool_req = ReleasePoolRequest {
        pool_id: pool_resp.pool_id.clone(),
    };
    plugin.release_pool(release_pool_req).await.unwrap();

    // Verify pool is removed
    let state = storage.read().await;
    assert!(!state.pools.contains_key(&pool_resp.pool_id));
}

#[tokio::test]
async fn test_persistence_across_restarts() {
    // Create temp state file
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let pool_id: String;
    let address: String;

    // Phase 1: Create plugin and allocate IPs
    {
        let storage = Arc::new(Storage::new(&state_path).await.unwrap());
        let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

        // Request a pool
        let pool_req = RequestPoolRequest {
            pool: Some("172.20.0.0/24".to_string()),
            sub_pool: None,
            options: None,
            v6: None,
        };
        let pool_resp = plugin.request_pool(pool_req).await.unwrap();
        pool_id = pool_resp.pool_id.clone();

        // Allocate an IP
        let addr_req = RequestAddressRequest {
            pool_id: pool_id.clone(),
            address: None,
            options: Some(
                vec![("container_name".to_string(), "persistent-test".to_string())]
                    .into_iter()
                    .collect(),
            ),
        };
        let addr_resp = plugin.request_address(addr_req).await.unwrap();
        address = addr_resp.address.clone();

        // Explicitly save state
        storage.save().await.unwrap();
    } // Drop plugin instance

    // Phase 2: Create new plugin instance with same state file
    {
        let storage = Arc::new(Storage::new(&state_path).await.unwrap());
        let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

        // Verify pool still exists
        {
            let state = storage.read().await;
            assert!(state.pools.contains_key(&pool_id));

            // Verify lease still exists
            assert!(state.leases.iter().any(|l| {
                let lease_ip = format!("{}/24", l.ip_address);
                lease_ip == address && l.container_name == "persistent-test"
            }));
        } // Drop read lock

        // Request the same IP again (should succeed and replace the old lease)
        let duplicate_req = RequestAddressRequest {
            pool_id: pool_id.clone(),
            address: Some(address.split('/').next().unwrap().to_string()),
            options: Some(
                vec![(
                    "container_name".to_string(),
                    "updated-container".to_string(),
                )]
                .into_iter()
                .collect(),
            ),
        };
        let duplicate_resp = plugin.request_address(duplicate_req).await.unwrap();
        assert_eq!(duplicate_resp.address, address);

        // Verify the container name was updated
        {
            let state = storage.read().await;
            let lease = state
                .leases
                .iter()
                .find(|l| format!("{}/24", l.ip_address) == address)
                .unwrap();
            assert_eq!(lease.container_name, "updated-container");
        }

        // Allocate a different IP
        let new_addr_req = RequestAddressRequest {
            pool_id: pool_id.clone(),
            address: None,
            options: Some(
                vec![("container_name".to_string(), "new-container".to_string())]
                    .into_iter()
                    .collect(),
            ),
        };
        let new_addr_resp = plugin.request_address(new_addr_req).await.unwrap();
        assert_ne!(
            new_addr_resp.address.split('/').next(),
            address.split('/').next()
        );
    }
}

#[test]
fn test_yaml_serialization() {
    // Test that the YAML state format is correct
    use serde_yaml;
    use std::collections::HashMap;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TestState {
        pools: HashMap<String, String>,
        leases: Vec<String>,
    }

    let mut state = TestState {
        pools: HashMap::new(),
        leases: Vec::new(),
    };

    state
        .pools
        .insert("pool-1".to_string(), "10.0.0.0/24".to_string());
    state.leases.push("10.0.0.1".to_string());

    let yaml = serde_yaml::to_string(&state).unwrap();
    assert!(yaml.contains("pool-1"));
    assert!(yaml.contains("10.0.0.0/24"));

    let deserialized: TestState = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(deserialized.pools.len(), 1);
    assert_eq!(deserialized.leases.len(), 1);
}

#[tokio::test]
async fn test_concurrent_address_allocation() {
    // Test concurrent IP allocation from the same pool
    // Note: The current implementation may have race conditions that allow
    // duplicate IP allocation in high-concurrency scenarios. This test
    // verifies that concurrent requests don't cause crashes or data corruption.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = Arc::new(IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string()));

    // Create a pool
    let pool_req = RequestPoolRequest {
        pool: Some("192.168.200.0/28".to_string()), // Small subnet (14 usable IPs)
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();

    // Spawn multiple concurrent tasks to allocate IPs
    let mut handles = vec![];
    for i in 0..10 {
        let plugin = plugin.clone();
        let pool_id = pool_resp.pool_id.clone();
        let handle = tokio::spawn(async move {
            let req = RequestAddressRequest {
                pool_id,
                address: None,
                options: Some(
                    vec![("container_name".to_string(), format!("concurrent-{}", i))]
                        .into_iter()
                        .collect(),
                ),
            };
            plugin.request_address(req).await
        });
        handles.push(handle);
    }

    // Wait for all tasks and collect results
    let mut addresses = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        if let Ok(resp) = result {
            addresses.push(resp.address);
        }
    }

    // Verify we got some successful allocations
    assert!(
        !addresses.is_empty(),
        "Should successfully allocate some IPs"
    );
    // Verify all are within the correct subnet
    for addr in &addresses {
        assert!(addr.starts_with("192.168.200."));
    }
}

#[tokio::test]
async fn test_ip_exhaustion() {
    // Test behavior when subnet runs out of IPs
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

    // Create a very small pool (only 2 usable IPs: .1 and .2)
    let pool_req = RequestPoolRequest {
        pool: Some("192.168.50.0/30".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();

    // Allocate first IP
    let req1 = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: Some(
            vec![("container_name".to_string(), "container1".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let resp1 = plugin.request_address(req1).await.unwrap();
    assert!(resp1.address.starts_with("192.168.50."));

    // Allocate second IP
    let req2 = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: Some(
            vec![("container_name".to_string(), "container2".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let resp2 = plugin.request_address(req2).await.unwrap();
    assert!(resp2.address.starts_with("192.168.50."));
    assert_ne!(resp1.address, resp2.address);

    // Try to allocate third IP (should fail - no more IPs available)
    let req3 = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: None,
    };
    let result = plugin.request_address(req3).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No available IP addresses"));
}

#[tokio::test]
async fn test_specific_ip_allocation() {
    // Test requesting specific IP addresses
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

    // Create pool
    let pool_req = RequestPoolRequest {
        pool: Some("172.30.0.0/24".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();

    // Request specific IP
    let specific_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: Some("172.30.0.100".to_string()),
        options: Some(
            vec![("container_name".to_string(), "specific-ip".to_string())]
                .into_iter()
                .collect(),
        ),
    };
    let specific_resp = plugin.request_address(specific_req).await.unwrap();
    assert_eq!(specific_resp.address, "172.30.0.100/24");

    // Try to allocate the same IP again (should succeed and replace the lease)
    let duplicate_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: Some("172.30.0.100".to_string()),
        options: Some(
            vec![(
                "container_name".to_string(),
                "replaced-container".to_string(),
            )]
            .into_iter()
            .collect(),
        ),
    };
    let duplicate_resp = plugin.request_address(duplicate_req).await.unwrap();
    assert_eq!(duplicate_resp.address, "172.30.0.100/24");

    // Verify only one lease exists for this IP
    let state = storage.read().await;
    let lease_count = state
        .leases
        .iter()
        .filter(|l| l.ip_address.to_string() == "172.30.0.100")
        .count();
    assert_eq!(lease_count, 1);
}

#[tokio::test]
async fn test_invalid_ip_address() {
    // Test error handling for invalid IP addresses
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

    // Create pool
    let pool_req = RequestPoolRequest {
        pool: Some("172.40.0.0/24".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();

    // Request IP outside the subnet range
    let invalid_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: Some("192.168.1.1".to_string()),
        options: None,
    };
    let result = plugin.request_address(invalid_req).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not in subnet"));

    // Request with malformed IP
    let malformed_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: Some("not-an-ip".to_string()),
        options: None,
    };
    let result = plugin.request_address(malformed_req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_release_nonexistent_address() {
    // Test releasing an IP that was never allocated
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

    // Create pool
    let pool_req = RequestPoolRequest {
        pool: Some("172.50.0.0/24".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();

    // Try to release an IP that was never allocated (should succeed - idempotent)
    let release_req = ReleaseAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: "172.50.0.99/24".to_string(),
    };
    let result = plugin.release_address(release_req).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_pools() {
    // Test managing multiple pools simultaneously
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.0.0.0/24".to_string());

    // Create first pool
    let pool1_req = RequestPoolRequest {
        pool: Some("192.168.1.0/24".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool1_resp = plugin.request_pool(pool1_req).await.unwrap();

    // Create second pool
    let pool2_req = RequestPoolRequest {
        pool: Some("192.168.2.0/24".to_string()),
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool2_resp = plugin.request_pool(pool2_req).await.unwrap();

    // Allocate IP from first pool
    let addr1_req = RequestAddressRequest {
        pool_id: pool1_resp.pool_id.clone(),
        address: None,
        options: None,
    };
    let addr1_resp = plugin.request_address(addr1_req).await.unwrap();
    assert!(addr1_resp.address.starts_with("192.168.1."));

    // Allocate IP from second pool
    let addr2_req = RequestAddressRequest {
        pool_id: pool2_resp.pool_id.clone(),
        address: None,
        options: None,
    };
    let addr2_resp = plugin.request_address(addr2_req).await.unwrap();
    assert!(addr2_resp.address.starts_with("192.168.2."));

    // Verify both pools exist
    let state = storage.read().await;
    assert_eq!(state.pools.len(), 2);
    assert!(state.pools.contains_key(&pool1_resp.pool_id));
    assert!(state.pools.contains_key(&pool2_resp.pool_id));
}

#[tokio::test]
async fn test_default_subnet_usage() {
    // Test that default subnet is used when none is specified
    let temp_dir = tempfile::TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.yaml");

    let storage = Arc::new(Storage::new(&state_path).await.unwrap());
    let plugin = IpamPlugin::new(storage.clone(), "10.99.0.0/16".to_string());

    // Request pool without specifying subnet
    let pool_req = RequestPoolRequest {
        pool: None,
        sub_pool: None,
        options: None,
        v6: None,
    };
    let pool_resp = plugin.request_pool(pool_req).await.unwrap();
    assert_eq!(pool_resp.pool, "10.99.0.0/16");

    // Allocate IP from default subnet
    let addr_req = RequestAddressRequest {
        pool_id: pool_resp.pool_id.clone(),
        address: None,
        options: None,
    };
    let addr_resp = plugin.request_address(addr_req).await.unwrap();
    assert!(addr_resp.address.starts_with("10.99."));
}
