.PHONY: help build test docker-build docker-test clean install run dev

# Default target
help:
	@echo "Docker IPAM Plugin - Available targets:"
	@echo "  make build         - Build the Rust binary"
	@echo "  make test          - Run all tests"
	@echo "  make docker-build  - Build Docker image"
	@echo "  make docker-test   - Test Docker image"
	@echo "  make docker-run    - Run plugin in Docker with docker-compose"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make install       - Install the plugin to /usr/local/bin"
	@echo "  make run           - Run the plugin locally"
	@echo "  make dev           - Run in development mode with debug logging"
	@echo "  make check         - Run cargo check"
	@echo "  make fmt           - Format code with rustfmt"
	@echo "  make clippy        - Run clippy linter"

# Build the binary
build:
	cargo build --release

# Run all tests
test:
	cargo test --all

# Run tests with verbose output
test-verbose:
	cargo test --all -- --nocapture

# Build Docker image
docker-build:
	docker build -t docker-ipam-plugin:latest .

# Tag Docker image
docker-tag:
	docker tag docker-ipam-plugin:latest docker-ipam-plugin:$$(cargo pkgid | cut -d# -f2)

# Test Docker image by running a simple container
docker-test: docker-build
	@echo "Testing Docker image..."
	docker run --rm docker-ipam-plugin:latest --version || true
	@echo "Docker image test complete"

# Run with docker-compose
docker-run:
	docker-compose up --build

# Run docker-compose in detached mode
docker-up:
	docker-compose up -d --build

# Stop docker-compose
docker-down:
	docker-compose down

# View docker-compose logs
docker-logs:
	docker-compose logs -f ipam-plugin

# Clean build artifacts
clean:
	cargo clean
	rm -f Cargo.lock
	docker-compose down -v 2>/dev/null || true

# Install to system
install: build
	sudo cp target/release/docker-ipam-plugin /usr/local/bin/
	sudo mkdir -p /var/lib/docker-ipam
	sudo mkdir -p /run/docker/plugins
	@echo "Plugin installed to /usr/local/bin/docker-ipam-plugin"

# Run locally for development
run: build
	RUST_LOG=docker_ipam_plugin=info \
	STATE_FILE=./state.yaml \
	DEFAULT_SUBNET=172.18.0.0/16 \
	TCP_ADDR=127.0.0.1:8080 \
	./target/release/docker-ipam-plugin

# Run in development mode with debug logging
dev:
	RUST_LOG=docker_ipam_plugin=debug \
	STATE_FILE=./state.yaml \
	DEFAULT_SUBNET=172.18.0.0/16 \
	TCP_ADDR=127.0.0.1:8080 \
	cargo run

# Check code without building
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Check formatting
fmt-check:
	cargo fmt -- --check

# Run clippy linter
clippy:
	cargo clippy -- -D warnings

# Run all checks (fmt, clippy, test)
ci: fmt-check clippy test
	@echo "All checks passed!"

# Generate documentation
docs:
	cargo doc --no-deps --open

# Update dependencies
update:
	cargo update

# Security audit
audit:
	cargo audit
