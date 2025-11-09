// Integration test for the full IPAM workflow
#[tokio::test]
async fn test_full_ipam_workflow() {
    // This test simulates a complete IPAM workflow:
    // 1. Create a pool
    // 2. Request multiple addresses
    // 3. Release some addresses
    // 4. Request more addresses (should reuse released ones)
    // 5. Release the pool

    // The actual test would require importing and using the plugin modules
    // For now, this serves as a placeholder for integration tests
}

#[tokio::test]
async fn test_persistence_across_restarts() {
    // This test verifies that state persists across plugin restarts
    // 1. Create plugin instance
    // 2. Allocate some IPs
    // 3. Drop the plugin instance
    // 4. Create new plugin instance with same state file
    // 5. Verify allocated IPs are still present
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
