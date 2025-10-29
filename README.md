# nginx-hibernator

Automatically hibernate and wake up nginx-proxied sites based on activity, reducing resource usage for idle services.

You can hibernate any service that:
- Provides an HTTP API
- Is proxied by nginx
- Can be started and stopped by systemd

## Features

- **Automatic Hibernation**: Services are automatically stopped after a configurable period of inactivity
- **Seamless Wake-up**: Incoming requests trigger service startup
- **Landing Page**: Customizable landing page displayed while the service is starting
- **Web Dashboard**: Monitor service states, view metrics, and analyze activity patterns through a minimalistic frontend
- **Persistent Storage**: Request history and state transitions stored in LMDB for efficient querying
- **Smart ETA Calculation**: Provides startup time estimates based on historical data
- **Flexible Configuration**: Per-service settings for timeouts, proxy modes, IP filtering, and more

## Dashboard

The hibernator includes a modern web-based dashboard for monitoring and managing your services:

- **Services Overview**: Real-time view of all services and their current states (up/down/starting)
- **Service Metrics**: Uptime percentage, hibernation count, and startup time distribution
- **State History**: Timeline of service state transitions
- **Request Logs**: Detailed access logs with request metadata and results

Access the dashboard at `http://localhost:7878` (or your configured `hibernator_port`).

## Installing

```bash
curl -fsSL https://raw.githubusercontent.com/Mubelotix/nginx-hibernator/master/install.sh | sh
```

This program cannot be installed as a docker container because it needs to interact with the host's systemd and nginx.

## Configuration

<!--
Generate the following sample using this chatgpt prompt:

> Generate a sample config toml including all comments and all fields
-->

```toml
#########################################
# HIBERNATOR GLOBAL CONFIGURATION FILE  #
#########################################
# This file configures both the hibernator service itself
# and the individual sites it manages.
# Each site can override most of the global settings.

#########################################
# [GLOBAL SETTINGS]
#########################################
# These values apply to the hibernator process as a whole.

# The port the hibernator listens to.
# This port should NEVER be exposed to the internet.
# Defaults to 7878
hibernator_port = 7878

# Path to the embedded database.
# Defaults to "./data.mdb"
database_path = "./data.mdb"

# Path to the folder containing the default landing page (index.html and assets).
# Defaults to "./landing"
landing_folder = "./landing"

# SHA-256 hash of the API key used for hibernator API authentication.
# If not set, API authentication is disabled.
# Generate with:
#   echo -n "your-api-key" | sha256sum
api_key_sha256 = "5e884898da28047151d0e56f8dc6292773603d0d6aabbdd62a11ef721d1542d8"  # example for "password"

#########################################
# [SITE CONFIGURATIONS]
#########################################
# Each [[sites]] section describes one managed site.
# You can define multiple sites by repeating [[sites]] blocks.

[[sites]]
# Unique name for the site
name = "example-site"

# Optional: Path to the nginx available config file.
# Defaults to /etc/nginx/sites-available/{name}
nginx_available_config = "/etc/nginx/sites-available/example-site"

# Optional: Path to the nginx enabled config file.
# Defaults to /etc/nginx/sites-enabled/{name}
nginx_enabled_config = "/etc/nginx/sites-enabled/example-site"

# Optional: Path to the nginx hibernator config file.
# Defaults to /etc/nginx/sites-available/nginx-hibernator
nginx_hibernator_config = "/etc/nginx/sites-available/nginx-hibernator"

# Number of start durations stored (used for ETA calculations)
# Default: 100
eta_sample_size = 100

# Percentile used for ETA computation (0–100)
# Default: 95
eta_percentile = 95

# The TCP port the service listens to (used to detect if it's up)
port = 8080

# Path to the nginx access log file.
# The nginx config must log to this file.
access_log = "/var/log/nginx/example-site.access.log"

# Optional string to filter log lines.
# Only matching lines are considered for activity tracking.
access_log_filter = "GET /"

# The name of the systemd service used to start/stop this site
service_name = "example-site.service"

# Hostnames that this site responds to.
# Used by hibernator to determine which site to start on incoming requests.
hosts = ["example.com", "www.example.com"]

# Proxy behavior for requests:
#   - "always"     → proxy all requests
#   - "when_ready" → proxy only when service is already up
#   - "never"      → disable proxy feature
proxy_mode = "always"

# Proxy mode for browser-issued requests (same options as above)
browser_proxy_mode = "when_ready"

# Maximum time (ms) to wait for proxy to succeed
# Default: 28000
proxy_timeout_ms = 28000

# Interval (ms) between checks to see if proxy is ready
# Default: 500
proxy_check_interval_ms = 500

# Optional: Glob patterns for paths that should NOT count as activity.
# Requests to these paths will NOT wake the service.
# Example: static assets, health checks, etc.
# Patterns follow standard glob syntax.
path_blacklist = ["*/static/*", "*/healthcheck"]

# Optional: IP addresses or prefixes that should NOT count as activity.
# Requests from these IPs will NOT wake the service.
ip_blacklist = ["192.168.1.0/24", "10.0.0.0/8"]

# Optional: IP addresses or prefixes that ARE allowed to wake the service.
# If set, requests from other IPs will be ignored.
ip_whitelist = ["203.0.113.0/24"]

# How long to keep the service running after last request (in seconds or with suffixes)
# Supports suffixes: s=seconds, m=minutes, h=hours, d=days
# Example: "300s" or "5m"
keep_alive = "5m"

# Timeout (ms) for waiting for service startup before giving up
# Default: 300000 (5 minutes)
start_timeout_ms = 300000

# Interval (ms) to check whether the service has started
# Default: 100
start_check_interval_ms = 100

# Optional: Site-specific landing page folder.
# If not set, uses the global landing_folder.
landing_folder = "/var/www/example-landing"
```

### Dashboard Setup

The frontend is built with Vue 3, TypeScript, and Vite. To run it in development mode:

```bash
cd frontend
bun install
bun run dev
```

For production, build the frontend and serve the static files:

```bash
cd frontend
bun run build
# Serve the dist/ directory with your preferred web server
```

## Development

### Dependencies

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh      # Rust
sudo apt update && sudo apt install build-essential -y              # GCC
curl -fsSL https://bun.sh/install | bash                            # Bun (for frontend)
```

### Building

Backend:
```bash
cd backend
cargo build
```

Frontend:
```bash
cd frontend
bun install
bun run build
```

### Running

Setup a dev environment (one-time only):

```bash
cd backend/dev
sh setup.sh
```

Run the hibernator (it will also build it):

```bash
cd backend/dev
sh run.sh
```

Run the frontend dashboard in development mode:

```bash
cd frontend
bun run dev
```

Check the backend behavior on `http://localhost:80` and the dashboard on `http://localhost:5173`.

## Security considerations

<details>
<summary>Information to take into account before deploying</summary>

### Access violations

If you are using nginx to restrict access to pages, please note that unless you set `proxy_mode=none` in each site configuration, some requests might bypass nginx and be proxied directly by the hibernator.

If your service handles authentication by itself, you are fine keeping the default.

### Code execution and XSS

The content of the config file is not sanitized.
**Do not rely on user input to generate the config file.**

Malicious configurations could trigger code execution as root, and XSS injections in waiting pages.

</details>

## Architecture

- **Backend**: Rust-based proxy server and service controller
  - Monitors nginx access logs for activity
  - Controls systemd services (start/stop)
  - Serves API endpoints for the dashboard
  - Stores data in LMDB (Lightning Memory-Mapped Database)
  
- **Frontend**: Vue 3 + TypeScript SPA
  - Real-time service monitoring
  - Historical data visualization
  - Responsive UI built with Tailwind CSS and shadcn-vue components

- **Database**: LMDB-based persistent storage
  - Connection history with request metadata
  - Service state transitions with timestamps
  - Startup duration samples for ETA calculation
  - Efficient append-only design with range queries

## Alternatives

The only known alternative is [GoDoxy](https://github.com/yusing/go-proxy?tab=readme-ov-file#idlesleeper). Unfortunately, this requires you to ditch nginx entirely for a less-mature proxy, and only supports docker containers rather than any systemd service.
