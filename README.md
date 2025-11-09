# Docker IPAM Plugin in Rust

A Docker IPAM (IP Address Management) plugin written in Rust using Tokio. This plugin manages static IP address assignments for containers within a subnet, persisting all allocations to a YAML file.

## Features

- Static IP to container name mapping
- Persistent storage in YAML format
- Tracks IP address, container name, and lease time
- Built with async Rust using Tokio
- Supports custom subnet configuration
- Automatic IP allocation from available pool
- Thread-safe concurrent operations

## Architecture

The plugin implements the Docker IPAM Driver API and consists of:

- **IPAM Plugin** (`src/ipam.rs`): Core logic for IP address management
- **HTTP Server** (`src/server.rs`): Unix socket server handling Docker API requests
- **Storage** (`src/storage.rs`): YAML-based persistence layer
- **Types** (`src/types.rs`): Data structures for requests/responses and state

## Building

### Prerequisites

- Rust 1.70 or later
- Docker (for testing)

### Build from source

```bash
cargo build --release
```

The binary will be located at `target/release/docker-ipam-plugin`.

## Installation

### Manual Installation

1. Build the plugin:
```bash
cargo build --release
```

2. Copy the binary to a system location:
```bash
sudo cp target/release/docker-ipam-plugin /usr/local/bin/
```

3. Create the state directory:
```bash
sudo mkdir -p /var/lib/docker-ipam
```

4. Create the plugin directory:
```bash
sudo mkdir -p /run/docker/plugins
```

5. Install the systemd service:
```bash
sudo cp docker-ipam-plugin.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable docker-ipam-plugin
sudo systemctl start docker-ipam-plugin
```

6. Create the Docker plugin spec:
```bash
sudo mkdir -p /etc/docker/plugins
sudo cp ipam-plugin.json /etc/docker/plugins/
```

## Configuration

Configure the plugin using environment variables:

- `SOCKET_PATH`: Path to Unix socket (default: `/run/docker/plugins/ipam.sock`)
- `STATE_FILE`: Path to YAML state file (default: `/var/lib/docker-ipam/state.yaml`)
- `DEFAULT_SUBNET`: Default subnet for IP allocation (default: `172.18.0.0/16`)
- `RUST_LOG`: Logging level (default: `docker_ipam_plugin=info`)

For TCP mode (testing only):
- `TCP_ADDR`: TCP address to bind (e.g., `127.0.0.1:8080`)

## Usage

### Create a network using the IPAM plugin

```bash
docker network create \
  --driver=bridge \
  --ipam-driver=ipam \
  --subnet=172.18.0.0/16 \
  mynetwork
```

### Run a container with automatic IP assignment

```bash
docker run -d --network=mynetwork --name mycontainer nginx
```

### Run a container with specific IP

```bash
docker run -d --network=mynetwork --ip=172.18.0.10 --name mycontainer nginx
```

### View allocated IPs

The state is stored in `/var/lib/docker-ipam/state.yaml`:

```yaml
pools:
  pool-xxxxx:
    pool_id: pool-xxxxx
    subnet: 172.18.0.0/16
    gateway: null
leases:
  - ip_address: 172.18.0.2
    container_name: mycontainer
    lease_time: 2025-01-09T10:30:00Z
```

## Development

### Run in development mode

```bash
# TCP mode for easier testing
TCP_ADDR=127.0.0.1:8080 \
STATE_FILE=./state.yaml \
DEFAULT_SUBNET=172.18.0.0/16 \
RUST_LOG=docker_ipam_plugin=debug \
cargo run
```

### Run tests

```bash
cargo test
```

### Check the logs

```bash
sudo journalctl -u docker-ipam-plugin -f
```

## API Endpoints

The plugin implements the Docker IPAM Driver API:

- `POST /Plugin.Activate` - Plugin activation
- `POST /IpamDriver.GetCapabilities` - Get driver capabilities
- `POST /IpamDriver.GetDefaultAddressSpaces` - Get default address spaces
- `POST /IpamDriver.RequestPool` - Request an IP pool
- `POST /IpamDriver.ReleasePool` - Release an IP pool
- `POST /IpamDriver.RequestAddress` - Request an IP address
- `POST /IpamDriver.ReleaseAddress` - Release an IP address

## State File Format

The YAML state file stores all IP allocations:

```yaml
pools:
  <pool_id>:
    pool_id: <pool_id>
    subnet: <CIDR>
    gateway: <optional>

leases:
  - ip_address: <IP>
    container_name: <name>
    lease_time: <timestamp>
```

## Troubleshooting

### Plugin not detected by Docker

1. Check the plugin socket exists:
```bash
ls -l /run/docker/plugins/ipam.sock
```

2. Verify the service is running:
```bash
sudo systemctl status docker-ipam-plugin
```

3. Check Docker can see the plugin:
```bash
docker plugin ls
```

### IP allocation issues

Check the state file:
```bash
sudo cat /var/lib/docker-ipam/state.yaml
```

View logs:
```bash
sudo journalctl -u docker-ipam-plugin -n 100
```

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.