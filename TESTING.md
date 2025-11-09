# Testing Docker IPAM Plugin

This document describes how to test the Docker IPAM plugin.

## Table of Contents

- [Unit Tests](#unit-tests)
- [Integration Tests](#integration-tests)
- [Docker Build Tests](#docker-build-tests)
- [Manual Testing](#manual-testing)
- [Test Scripts](#test-scripts)

## Unit Tests

Run all unit tests:

```bash
cargo test
```

Run tests with verbose output:

```bash
cargo test -- --nocapture
```

Run specific test module:

```bash
cargo test storage::tests
cargo test ipam::tests
```

## Test Coverage

The test suite includes:

### Storage Module Tests (`src/storage.rs`)
- ✓ `test_storage_new_creates_default_state` - Verifies new storage creates empty state
- ✓ `test_storage_save_and_reload` - Tests persistence across instances
- ✓ `test_storage_write_and_read` - Tests concurrent read/write operations
- ✓ `test_storage_atomic_save` - Verifies atomic file writes

### IPAM Module Tests (`src/ipam.rs`)
- ✓ `test_get_capabilities` - Tests capability reporting
- ✓ `test_request_pool` - Tests pool creation with custom subnet
- ✓ `test_request_pool_with_default_subnet` - Tests default subnet usage
- ✓ `test_request_and_release_address` - Tests IP allocation and release
- ✓ `test_request_specific_address` - Tests specific IP assignment
- ✓ `test_multiple_address_allocation` - Tests allocating multiple IPs
- ✓ `test_release_pool` - Tests pool cleanup
- ✓ `test_allocate_next_ip` - Tests IP allocation algorithm
- ✓ `test_invalid_pool_request` - Tests error handling

### Integration Tests (`tests/integration_test.rs`)
- ✓ `test_full_ipam_workflow` - End-to-end workflow test
- ✓ `test_persistence_across_restarts` - State persistence test
- ✓ `test_yaml_serialization` - YAML format validation

## Docker Build Tests

### Build the Docker image:

```bash
make docker-build
```

Or use the build script:

```bash
./build.sh
```

### Test the Docker image:

```bash
make docker-test
```

### Run the plugin in Docker:

```bash
docker-compose up --build
```

## Manual Testing

### 1. Run Plugin in TCP Mode (for testing)

Start the plugin in TCP mode for easier testing:

```bash
TCP_ADDR=127.0.0.1:8080 \
STATE_FILE=./test-state.yaml \
DEFAULT_SUBNET=10.99.0.0/24 \
RUST_LOG=docker_ipam_plugin=debug \
cargo run
```

### 2. Test API Endpoints

Test the plugin endpoints using curl:

```bash
# Test activation
curl -X POST http://127.0.0.1:8080/Plugin.Activate
# Expected: {"Implements":["IpamDriver"]}

# Test capabilities
curl -X POST http://127.0.0.1:8080/IpamDriver.GetCapabilities
# Expected: {"RequiresMACAddress":false,"RequiresRequestReplay":false}

# Test default address spaces
curl -X POST http://127.0.0.1:8080/IpamDriver.GetDefaultAddressSpaces
# Expected: {"LocalDefaultAddressSpace":"local","GlobalDefaultAddressSpace":"global"}

# Request a pool
curl -X POST http://127.0.0.1:8080/IpamDriver.RequestPool \
  -H "Content-Type: application/json" \
  -d '{"Pool":"192.168.1.0/24"}'
# Expected: {"PoolID":"pool-xxxxx","Pool":"192.168.1.0/24","Data":{}}

# Request an address (use the PoolID from above)
curl -X POST http://127.0.0.1:8080/IpamDriver.RequestAddress \
  -H "Content-Type: application/json" \
  -d '{"PoolID":"pool-xxxxx","Options":{"container_name":"test-container"}}'
# Expected: {"Address":"192.168.1.1/24","Data":{}}

# Release an address
curl -X POST http://127.0.0.1:8080/IpamDriver.ReleaseAddress \
  -H "Content-Type: application/json" \
  -d '{"PoolID":"pool-xxxxx","Address":"192.168.1.1/24"}'
# Expected: {}

# Release the pool
curl -X POST http://127.0.0.1:8080/IpamDriver.ReleasePool \
  -H "Content-Type: application/json" \
  -d '{"PoolID":"pool-xxxxx"}'
# Expected: {}
```

### 3. Verify State File

Check the YAML state file:

```bash
cat test-state.yaml
```

Expected format:
```yaml
pools:
  pool-xxxxx:
    pool_id: pool-xxxxx
    subnet: 192.168.1.0/24
    gateway: null
leases:
  - ip_address: 192.168.1.1
    container_name: test-container
    lease_time: '2025-01-09T10:30:00Z'
```

## Test Scripts

### Automated Test Script

Run the comprehensive test script:

```bash
./test.sh
```

This script runs:
1. All unit tests
2. Clippy linter checks
3. Code formatting checks
4. (Optional) Coverage analysis
5. (Optional) Live endpoint tests

### Build Script

Run the build script to build and test everything:

```bash
./build.sh
```

## Continuous Integration

For CI/CD pipelines, use:

```bash
make ci
```

This runs:
- Code formatting check
- Clippy linter
- All tests

## Performance Testing

### Test IP Allocation Speed

```bash
# Run tests with timing
cargo test --release -- --nocapture test_multiple_address_allocation
```

### Test Concurrent Operations

The storage module uses `RwLock` for thread-safe operations. Test concurrent access:

```bash
cargo test --release -- --test-threads=8
```

## Troubleshooting Tests

### Tests Fail with "Permission Denied"

Ensure you have write permissions in the test directory:

```bash
chmod -R u+w tests/
```

### Docker Build Fails

1. Check Docker is running:
   ```bash
   docker ps
   ```

2. Try building with no cache:
   ```bash
   docker build --no-cache -t docker-ipam-plugin:latest .
   ```

### State File Issues

Clean up test state files:

```bash
rm -f test-state.yaml state.yaml *.tmp
```

## Test Environment

Tests run in isolated temporary directories using the `tempfile` crate, ensuring:
- No conflicts between tests
- Automatic cleanup
- Reproducible results

## Next Steps

After testing, see:
- [README.md](README.md) for installation instructions
- [Makefile](Makefile) for available commands
- [Docker deployment guide](README.md#installation)
