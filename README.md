# nginx-site-hibernator

A simple program that detects when your sites are not used by anyone and shuts them down, and then starts them up again when they are needed.

It reads nginx access logs to determine when a site is being used, and systemd to start and stop the site.

## Configuration

<!--
Generate the following sample using this chatgpt prompt:

> Generate a sample config toml including all comments and all fields
-->

```toml
# Top-level configuration settings for the hibernator service

# Where the nginx hibernator config file is located.
# Defaults to "/etc/nginx/sites-available/hibernator".
nginx_hibernator_config = "/etc/nginx/sites-available/hibernator"

# The port the hibernator listens to.
# This port should never be exposed to the internet.
# Defaults to 7878.
hibernator_port = 7878

# List of sites to be managed by the hibernator.
[[sites]]

# The name of the site. Must be unique.
name = "example_site"

# Path to the nginx available config file.
# Defaults to "/etc/nginx/sites-available/{name}".
nginx_available_config = "/etc/nginx/sites-available/example_site"

# Path to the nginx enabled config file.
# Defaults to "/etc/nginx/sites-enabled/{name}".
nginx_enabled_config = "/etc/nginx/sites-enabled/example_site"

# The port the service listens to. Used to determine if the service is up.
port = 8080

# The path to the access log file.
# Your nginx configuration must log the requests to this file.
access_log = "/var/log/nginx/example_site_access.log"

# Optional filter to match lines in the access log.
# Only lines containing this string will be considered.
# Example: "GET /api/"
# access_log_filter = "GET /api/"  # Uncomment to set a filter

# The name of the systemctl service that runs the site.
# Commands like `systemctl start` and `systemctl stop` will be run with this name.
service_name = "example_site_service"

# The hostnames that the service listens to.
# Used by the hibernator to know which site to start upon receiving a request.
hosts = ["example.com", "www.example.com"]

# The proxy mode.
#   - "all": Proxies all requests.
#   - "nonbrowser": Only waits for API calls and shows a waiting page for browser users.
#   - "none": Disables the proxy feature.
# Defaults to "nonbrowser".
proxy_mode = "nonbrowser"

# Maximum time to wait before giving up on the proxy, in milliseconds.
# Defaults to 28000 ms (28 seconds).
proxy_timeout_ms = 28000

# Interval time to check if the proxy is up, in milliseconds.
# Defaults to 500 ms (0.5 seconds).
proxy_check_interval_ms = 500

# The time in seconds to keep the service running after the last request.
# The service will be stopped after this time. Example: "10m" for 10 minutes.
keep_alive = "10m"  # Can be specified with units: s (seconds), m (minutes), h (hours), d (days)
```

## Security considerations

<details>
<summary>Information to take into account before deploying</summary>

### Access violations

If you are using nginx to restrict access to pages, please note that unless you set `proxy_mode=none` in each site configuration, some requests might bypass nginx and be proxied directly by the hibernator.

If your service handles authentication by itself, you are fine keeping the default.

### Malware

This program needs to run as root. Hence, I have kept the dependencies to a minimum. Here is the dependency tree :


```
nginx-hibernator v0.1.0 (/home/mubelotix/projects/nginx-site-hibernator)
├── anyhow v1.0.93
├── chrono v0.4.38
│   ├── iana-time-zone v0.1.61
│   └── num-traits v0.2.19
│       [build-dependencies]
│       └── autocfg v1.4.0
├── env_logger v0.11.5
│   ├── anstream v0.6.18
│   │   ├── anstyle v1.0.10
│   │   ├── anstyle-parse v0.2.6
│   │   │   └── utf8parse v0.2.2
│   │   ├── anstyle-query v1.1.2
│   │   ├── colorchoice v1.0.3
│   │   ├── is_terminal_polyfill v1.70.1
│   │   └── utf8parse v0.2.2
│   ├── anstyle v1.0.10
│   ├── env_filter v0.1.2
│   │   ├── log v0.4.22
│   │   └── regex v1.11.1
│   │       ├── aho-corasick v1.1.3
│   │       │   └── memchr v2.7.4
│   │       ├── memchr v2.7.4
│   │       ├── regex-automata v0.4.8
│   │       │   ├── aho-corasick v1.1.3 (*)
│   │       │   ├── memchr v2.7.4
│   │       │   └── regex-syntax v0.8.5
│   │       └── regex-syntax v0.8.5
│   ├── humantime v2.1.0
│   └── log v0.4.22
├── log v0.4.22
├── rev_lines v0.3.0
│   └── thiserror v1.0.68
│       └── thiserror-impl v1.0.68 (proc-macro)
│           ├── proc-macro2 v1.0.89
│           │   └── unicode-ident v1.0.13
│           ├── quote v1.0.37
│           │   └── proc-macro2 v1.0.89 (*)
│           └── syn v2.0.87
│               ├── proc-macro2 v1.0.89 (*)
│               ├── quote v1.0.37 (*)
│               └── unicode-ident v1.0.13
├── serde v1.0.214
│   └── serde_derive v1.0.214 (proc-macro)
│       ├── proc-macro2 v1.0.89 (*)
│       ├── quote v1.0.37 (*)
│       └── syn v2.0.87 (*)
└── toml v0.8.19
    ├── serde v1.0.214 (*)
    ├── serde_spanned v0.6.8
    │   └── serde v1.0.214 (*)
    ├── toml_datetime v0.6.8
    │   └── serde v1.0.214 (*)
    └── toml_edit v0.22.22
        ├── indexmap v2.6.0
        │   ├── equivalent v1.0.1
        │   └── hashbrown v0.15.1
        ├── serde v1.0.214 (*)
        ├── serde_spanned v0.6.8 (*)
        ├── toml_datetime v0.6.8 (*)
        └── winnow v0.6.20
```

You might get the hibernator to run as non-root using sudo's command whitelist feature. This might require forking the project to add "sudo" in front of the commands.
</details>
