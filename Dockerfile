# Multi-stage build for Docker IPAM Plugin
# Stage 1: Build the Rust binary
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy dependency files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy src/main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release || true && \
    rm -rf src

# Copy the actual source code
COPY src ./src

# Build the actual binary
RUN cargo build --release && \
    strip /build/target/release/docker-ipam-plugin

# Create directory structure for runtime
RUN mkdir -p /runtime/var/lib/docker-ipam && \
    mkdir -p /runtime/run/docker/plugins && \
    mkdir -p /runtime/tmp

# Stage 2: Create the runtime image with distroless
FROM gcr.io/distroless/cc-debian12:nonroot

# Copy the binary from builder
COPY --from=builder /build/target/release/docker-ipam-plugin /usr/local/bin/docker-ipam-plugin

# Copy directory structure
COPY --from=builder --chown=nonroot:nonroot /runtime/var/lib/docker-ipam /var/lib/docker-ipam
COPY --from=builder --chown=nonroot:nonroot /runtime/run/docker/plugins /run/docker/plugins
COPY --from=builder --chown=nonroot:nonroot /runtime/tmp /tmp

# Set environment variables
ENV SOCKET_PATH=/run/docker/plugins/ipam.sock
ENV STATE_FILE=/var/lib/docker-ipam/state.yaml
ENV DEFAULT_SUBNET=172.18.0.0/16
ENV RUST_LOG=docker_ipam_plugin=info

USER nonroot:nonroot

ENTRYPOINT ["/usr/local/bin/docker-ipam-plugin"]
