# nginx-site-hibernator

A simple program that detects when your sites are not used by anyone and shuts them down, and then starts them up again when they are needed.

It reads nginx access logs to determine when a site is being used, and systemd to start and stop the site.

```toml
[site]
name = "example" # The name of the nginx site
access_log = "/var/log/nginx/example.access.log" # The path to the access log
access_log_filter = "example.com" # Optional filter to match lines in the access log
service_name = "webserver" # The name of the service that runs the site
keep_alive = "5m" # Time to keep the site running after the last access
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
