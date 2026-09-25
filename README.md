# nginx-hibernator

Automatically hibernate and wake up nginx-proxied sites based on activity, reducing resource usage for idle services.

This project is an NGINX module that monitors incoming traffic and manages the lifecycle of upstream systemd services.

## Features

- **Automatic Hibernation**: Services are automatically stopped after a configurable period of inactivity.
- **Seamless Wake-up**: Incoming requests trigger service startup.
- **Asynchronous Landing Page**: Customizable landing page displayed while the service is starting, served without blocking NGINX worker threads.
- **Smart ETA Calculation**: Provides startup time estimates based on historical data, injected into the landing page.
- **Systemd Integration**: Directly controls services using `systemd` via dbus or CLI.
- **Non-blocking Operations**: Health checks and landing page file I/O are performed asynchronously.

## Quick Start

The project is currently in development. To use it, you need to build it from source and load it as an NGINX dynamic module.

```bash
# Clone the repository
git clone https://github.com/Mubelotix/nginx-hibernator
cd nginx-hibernator

# Build the module (requires Rust)
cargo build --release

# Build NGINX with the module (requires helper scripts)
./scripts/build-nginx.sh
```

## Configuration

Enable hibernation for a specific location in your `nginx.conf` and configure the module using the following directives:

```nginx
http {
  upstream app_backend {
    server 127.0.0.1:18081;
  }

  server {
    listen 80;
    server_name example.com;

    location / {
      # Enable hibernation logic for this location.
      hibernator on;

      # Name of the systemd service to manage.
      hibernator_service_name my-app-service;

      # TCP port of the upstream app used for readiness checks.
      hibernator_check_port 18081;

      # Readiness check mode. Options: http (default) | tcp.
      hibernator_check_mode http;

      # HTTP endpoint used for readiness checks (only if mode is http).
      # Default: /ready. Any valid HTTP response means the upstream is reachable.
      hibernator_check_endpoint /ready;

      # Max time allowed for a single readiness check.
      # Default: 100ms
      hibernator_check_timeout 100ms;

      # Background health check intervals.
      hibernator_up_check_interval 10s;       # While service is up
      hibernator_starting_check_interval 100ms; # While service is starting
      hibernator_down_check_interval 60s;      # While service is down (hibernated)

      # Keep backend alive after the last request for this long.
      # Default: 5m
      hibernator_keep_alive 5m;

      # Max time to wait for startup before returning a 503 response.
      # Default: 5m
      hibernator_start_timeout 5m;

      # Poll interval while waiting for service startup during a request.
      # Default: 100ms
      hibernator_start_check_interval 100ms;

      # Folder containing landing page files (index.html + assets).
      # Default: /usr/share/nginx-hibernator/landing
      hibernator_landing_dir /var/www/landing;

      # Require an interaction before a down service is started.
      # Default: off
      hibernator_checkpoint on;

      # --- Startup ETA Options ---

      # Enable estimation and tracking of startup durations.
      # Default: on
      hibernator_eta on;

      # File to store historical startup times for the service.
      # Default: /var/lib/nginx/hibernator/startup-times-{service}.txt
      hibernator_history_file /var/lib/nginx/hibernator/startup-times-myapp.txt;

      # Number of recent samples to use for calculating the ETA.
      # Default: 40
      hibernator_history_samples_count 40;

      # Percentile of samples to use as the expected startup time.
      # Default: 95
      hibernator_history_percentile 95;

      # Regular proxy settings.
      proxy_pass http://app_backend;
    }
  }
}
```

## Landing Page

When a service is hibernated, an incoming request triggers its startup. During this time, NGINX returns a `503 Service Unavailable` response with the content of `index.html` from the `hibernator_landing_dir`. When installed from the deb package, the default landing page is located at `/usr/share/nginx-hibernator/landing/` and is used automatically.

The following placeholders are automatically replaced in the HTML:
- `{{ETA_SECONDS}}`: Estimated seconds remaining.
- `DURATION_MS`: Expected total duration in milliseconds.
- `DONE_MS`: Elapsed time in milliseconds.
- `KEEP_ALIVE`: The configured keep-alive duration in seconds.

Assets (images, CSS, JS) from the `hibernator_landing_dir` are served under the `/hibernator-landing/` URI prefix.

### Checkpoint Page

Set `hibernator_checkpoint on;` to return `checkpoint.html` from the landing directory before starting a down service. The bundled page displays an `Enter Site` control and sends a JavaScript confirmation request when that control is clicked. That confirmation follows the normal startup and landing-page path. Automated `GET` requests remain on the checkpoint page and do not start the service.

Custom landing directories need both `index.html` and `checkpoint.html` when this option is enabled.

## Development

To run a development environment with a sample service:

```bash
./scripts/run.sh start
```

This script builds NGINX, the module, and sets up a test environment.

### Docker build

If you want a reproducible Debian package build environment that compiles against the Debian nginx source package, use the Docker helper script:

```bash
./scripts/debian-build.sh
```

This builds a Debian trixie image, downloads the nginx source package inside the container, and exports a `.deb` package artifact to:

```bash
target/docker/nginx-hibernator-module_<version>-<release>_<arch>.deb
```

### APT Repository

Prebuilt packages are available on GitHub Pages. To install from the APT repository:

```bash
# Install the repository public key
curl -fsSL https://mubelotix.github.io/nginx-hibernator/dists/trixie/repo-public.key | sudo gpg --dearmor -o /usr/share/keyrings/nginx-hibernator.gpg

# Add the repository
echo "deb [signed-by=/usr/share/keyrings/nginx-hibernator.gpg] https://mubelotix.github.io/nginx-hibernator trixie main" | sudo tee /etc/apt/sources.list.d/nginx-hibernator.list

# Update package lists
sudo apt-get update

# Install the module
sudo apt-get install nginx-hibernator-module

# Reload nginx
sudo systemctl reload nginx
```

The module is automatically loaded by nginx through `/etc/nginx/modules-enabled/`.

The APT repository is signed, and the public key is published at `https://mubelotix.github.io/nginx-hibernator/dists/trixie/repo-public.key`.

## Alternatives

- [GoDoxy](https://github.com/yusing/go-proxy): A Go-based proxy with similar features but requires replacing NGINX and only supports Docker.
