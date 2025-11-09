#!/bin/bash
set -e

echo "=========================================="
echo "Building Docker IPAM Plugin"
echo "=========================================="

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Cargo is not installed. Please install Rust from https://rustup.rs/${NC}"
    exit 1
fi

echo -e "${BLUE}Step 1: Running tests...${NC}"
cargo test --all
echo -e "${GREEN}✓ All tests passed${NC}"

echo -e "${BLUE}Step 2: Building release binary...${NC}"
cargo build --release
echo -e "${GREEN}✓ Binary built successfully${NC}"

echo -e "${BLUE}Step 3: Checking if Docker is available...${NC}"
if command -v docker &> /dev/null; then
    echo -e "${GREEN}✓ Docker found${NC}"

    echo -e "${BLUE}Step 4: Building Docker image...${NC}"
    docker build -t docker-ipam-plugin:latest .
    echo -e "${GREEN}✓ Docker image built successfully${NC}"

    echo -e "${BLUE}Step 5: Tagging image...${NC}"
    VERSION=$(cargo pkgid | cut -d# -f2 | cut -d: -f2)
    docker tag docker-ipam-plugin:latest docker-ipam-plugin:${VERSION}
    echo -e "${GREEN}✓ Image tagged as docker-ipam-plugin:${VERSION}${NC}"
else
    echo -e "${RED}⚠ Docker not found. Skipping Docker image build.${NC}"
fi

echo ""
echo -e "${GREEN}=========================================="
echo "Build completed successfully!"
echo "==========================================${NC}"
echo ""
echo "Binary location: target/release/docker-ipam-plugin"
echo "Docker image: docker-ipam-plugin:latest"
echo ""
echo "Next steps:"
echo "  - Run locally:        make run"
echo "  - Run in Docker:      make docker-run"
echo "  - Install to system:  make install"
echo "  - View help:          make help"
