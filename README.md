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
##############################################
# Hibernator Configuration File
# This file controls the global hibernator behavior and per-site settings.
# All durations support suffixes: s (seconds), m (minutes), h (hours), d or j (days)
##############################################


##############################################
# Top-level Configuration
##############################################

# The port that the hibernator listens on.
# This port should **never** be exposed to the internet.
# Defaults to 7878.
hibernator_port = 7878

# Path to the database file where runtime data is stored.
# Defaults to "./data.mdb".
database_path = "./data.mdb"



##############################################
# Per-site Configuration
##############################################

[[sites]]
# The unique name of the site.
name = "example-site"

# Path to nginx "available" configuration file.
# Default: /etc/nginx/sites-available/{name}
# nginx_available_config = "/etc/nginx/sites-available/example-site"

# Path to nginx "enabled" configuration file.
# Default: /etc/nginx/sites-enabled/{name}
# nginx_enabled_config = "/etc/nginx/sites-enabled/example-site"

# Path to nginx hibernator configuration.
# Default: /etc/nginx/sites-available/nginx-hibernator
# nginx_hibernator_config = "/etc/nginx/sites-available/nginx-hibernator"

# Number of recent start durations to store for ETA computation.
# Default: 100
eta_sample_size = 100

# Percentile used to compute ETA from samples.
# Should be between 0 and 100. Default: 95
eta_percentile = 95

# Port that the backend service listens to.
# Used to determine if the service is up.
port = 8080

# Path to the nginx access log file.
# Must be the same file where nginx logs requests.
access_log = "/var/log/nginx/example-site.access.log"

# Optional substring filter for access log entries.
# Only lines containing this string will be considered.
# access_log_filter = "GET /api/"

# Systemd service name for this site.
# Commands like `systemctl start <service_name>` will be executed.
service_name = "example-site.service"

# Hostnames that this site responds to.
# Used to decide which service to start when requests come in.
hosts = ["example.com", "www.example.com"]

# Proxy behavior mode. Options:
#   - "always"     → always proxy requests
#   - "when_ready" → proxy only when upstream is ready
#   - "never"      → disable proxy
# Default: "always"
proxy_mode = "always"

# Proxy mode for browser requests.
# Default: "when_ready"
browser_proxy_mode = "when_ready"

# Maximum time to wait for the backend proxy to respond (milliseconds).
# Default: 28000
proxy_timeout_ms = 28000

# Interval to check if proxy is up (milliseconds).
# Default: 500
proxy_check_interval_ms = 500

# List of glob patterns for paths that should NOT count as activity.
# Requests to these paths do not reset the keep-alive timer.
# Example: ["/static/*", "/health", "/favicon.ico"]
# path_blacklist = ["/static/*", "/health"]

# List of IPs or prefixes that should NOT count as activity.
# Example: ["127.0.0.1", "192.168.0.0/16"]
# ip_blacklist = ["127.0.0.1"]

# List of IPs or prefixes allowed to wake up the service.
# If set, requests from other IPs are ignored.
# Example: ["203.0.113.0/24"]
# ip_whitelist = ["203.0.113.0/24"]

# How long to keep the service running after last request.
# Accepts units: s (seconds), m (minutes), h (hours), d (days)
# Example: "10m" means 10 minutes.
keep_alive = "10m"

# Maximum time to wait for service startup (milliseconds).
# Default: 300000 (5 minutes)
start_timeout_ms = 300000

# Interval to check if the service has started (milliseconds).
# Default: 100
start_check_interval_ms = 100
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
