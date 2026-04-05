> [!IMPORTANT]  
> This project is undergoing a massive rewrite to transition from a backend running on the side to a module running inside the nginx process itself.

# nginx-hibernator

Automatically hibernate and wake up nginx-proxied sites based on activity, reducing resource usage for idle services.

You can hibernate any service that:
- Provides an HTTP API
- Is proxied by nginx
- Can be started and stopped by systemd

## Features

- **Automatic Hibernation**: Services are automatically stopped after a configurable period of inactivity
- **Seamless Wake-up**: Incoming requests trigger service startup
- **Landing Page**: Customizable landing page displayed while the service is starting; see [landing/README.md](landing/README.md) for how to build one
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

## Development

### Dependencies

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh      # Rust
sudo apt update && sudo apt install build-essential libdbus-1-dev pkg-config -y
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

## NGINX Module Configuration Goals

The old backend (`./backend`) had a rich per-site config model. The target is to expose equivalent behavior as clear, optional nginx directives with sane defaults.

### Sample `nginx.conf` (target design)

```nginx
http {
  upstream app_backend {
    server 127.0.0.1:18081;
  }

  server {
    listen 127.0.0.1:18080;
    server_name localhost;

    location / {
      # Enable hibernation logic for this location.
      hibernator on;

      # Service to wake/suspend.
      # Required when service control is enabled.
      hibernator_service_name simple_python_http;

      # TCP port of the upstream app used by the readiness checks.
      hibernator_check_port 18081;

      # Service readiness check mode.
      # Values: http | tcp
      # Default: http
      hibernator_check_mode http;

      # HTTP endpoint used by the http readiness checker.
      # Default: /ready
      hibernator_check_endpoint /ready;

      # Max time allowed for the service to answer readiness checks.
      # Used by both http and port check modes.
      # Default: 100ms
      hibernator_check_timeout 100ms;

      # Background health monitor interval while the service is up.
      # Default: 10s
      hibernator_up_check_interval 10s;

      # Background health monitor interval while the service is in starting state.
      # Default: 100ms
      hibernator_starting_check_interval 100ms;

      # Background health monitor interval while the service is down.
      # Default: 60s
      hibernator_down_check_interval 60s;

      # Keep backend alive after the last qualifying request.
      hibernator_keep_alive 5m;

      # Max wait for startup before returning fallback response.
      hibernator_start_timeout 5m;

      # Poll interval while waiting for service startup.
      hibernator_start_check_interval 100ms;

      # Folder containing landing page files (index + assets).
      hibernator_landing_dir /var/www/nginx-hibernator/landing;

      # Startup ETA model tuning.
      hibernator_eta_sample_size 100;
      hibernator_eta_percentile 95;

      proxy_pass http://app_backend;
    }
  }
}
```

Process-level legacy settings like `hibernator_port`, `database_path`, `api_key_sha256`, and deployment path fields are intentionally not planned as nginx directives.

Current implementation uses the `hibernator*` directives shown in the sample config above.

### Build and run helpers

```bash
cd module
cargo build --release
cd ..
scripts/build-nginx.sh
scripts/test-hibernator.sh
scripts/run.sh start
```
