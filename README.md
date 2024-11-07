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
