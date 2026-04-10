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
cd module
cargo build --release

# Build NGINX with the module (requires helper scripts)
./module/scripts/build-nginx.sh
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
      # Default: /ready
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
      # If not set, the built-in default page is used.
      hibernator_landing_dir /var/www/landing;

      # --- Startup ETA Options ---

      # Enable estimation and tracking of startup durations.
      # Default: on
      hibernator_eta on;

      # File to store historical startup times for the service.
      # Default: /var/log/nginx/startup-times-{service}.txt
      hibernator_history_file /var/log/nginx/startup-times-myapp.txt;

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

When a service is hibernated, an incoming request triggers its startup. During this time, NGINX returns a `503 Service Unavailable` response with the content of `index.html` from the `hibernator_landing_dir`.

The following placeholders are automatically replaced in the HTML:
- `{{ETA_SECONDS}}`: Estimated seconds remaining.
- `DURATION_MS`: Expected total duration in milliseconds.
- `DONE_MS`: Elapsed time in milliseconds.
- `KEEP_ALIVE`: The configured keep-alive duration in seconds.

Assets (images, CSS, JS) from the `hibernator_landing_dir` are served under the `/hibernator-landing/` URI prefix.

## Development

To run a development environment with a sample service:

```bash
cd module
./scripts/run.sh start
```

This script builds NGINX, the module, and sets up a test environment.

## Alternatives

- [GoDoxy](https://github.com/yusing/go-proxy): A Go-based proxy with similar features but requires replacing NGINX and only supports Docker.
