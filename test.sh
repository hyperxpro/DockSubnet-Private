#!/bin/bash
set -e

echo "=========================================="
echo "Testing Docker IPAM Plugin"
echo "=========================================="

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Run unit tests
echo -e "${BLUE}Running unit tests...${NC}"
cargo test --all -- --test-threads=1
echo -e "${GREEN}✓ Unit tests passed${NC}"

# Run tests with coverage if available
if cargo tarpaulin --version &> /dev/null; then
    echo -e "${BLUE}Running tests with coverage...${NC}"
    cargo tarpaulin --out Html --output-dir coverage
    echo -e "${GREEN}✓ Coverage report generated in coverage/index.html${NC}"
else
    echo -e "${YELLOW}⚠ cargo-tarpaulin not installed. Skipping coverage.${NC}"
    echo "  Install with: cargo install cargo-tarpaulin"
fi

# Run clippy
echo -e "${BLUE}Running clippy linter...${NC}"
cargo clippy --all-targets --all-features -- -D warnings
echo -e "${GREEN}✓ Clippy checks passed${NC}"

# Check formatting
echo -e "${BLUE}Checking code formatting...${NC}"
cargo fmt -- --check
echo -e "${GREEN}✓ Code formatting is correct${NC}"

# Run the binary in test mode if TCP_ADDR is set
if [ ! -z "$TCP_ADDR" ]; then
    echo -e "${BLUE}Starting plugin in test mode...${NC}"

    # Start the plugin in background
    STATE_FILE=/tmp/test-state.yaml \
    DEFAULT_SUBNET=10.99.0.0/24 \
    RUST_LOG=docker_ipam_plugin=debug \
    cargo run &
    PLUGIN_PID=$!

    # Wait for plugin to start
    sleep 2

    # Test endpoints
    echo -e "${BLUE}Testing plugin endpoints...${NC}"

    # Test activation
    echo -e "${YELLOW}Testing /Plugin.Activate...${NC}"
    curl -s -X POST http://${TCP_ADDR}/Plugin.Activate | grep -q "IpamDriver" && \
        echo -e "${GREEN}✓ Activation endpoint works${NC}" || \
        echo -e "${RED}✗ Activation endpoint failed${NC}"

    # Test capabilities
    echo -e "${YELLOW}Testing /IpamDriver.GetCapabilities...${NC}"
    curl -s -X POST http://${TCP_ADDR}/IpamDriver.GetCapabilities | grep -q "RequiresMACAddress" && \
        echo -e "${GREEN}✓ Capabilities endpoint works${NC}" || \
        echo -e "${RED}✗ Capabilities endpoint failed${NC}"

    # Test default address spaces
    echo -e "${YELLOW}Testing /IpamDriver.GetDefaultAddressSpaces...${NC}"
    curl -s -X POST http://${TCP_ADDR}/IpamDriver.GetDefaultAddressSpaces | grep -q "LocalDefaultAddressSpace" && \
        echo -e "${GREEN}✓ Default address spaces endpoint works${NC}" || \
        echo -e "${RED}✗ Default address spaces endpoint failed${NC}"

    # Stop the plugin
    kill $PLUGIN_PID 2>/dev/null || true
    rm -f /tmp/test-state.yaml
fi

echo ""
echo -e "${GREEN}=========================================="
echo "All tests passed!"
echo "==========================================${NC}"
