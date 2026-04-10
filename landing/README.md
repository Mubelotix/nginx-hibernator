# Landing Page Folder

This folder contains the static landing page shown while the NGINX module is starting or waking a hibernated upstream.

## How It Is Served

The module reads `index.html` from the configured landing directory and serves it directly when the upstream is unavailable.

It also serves any other files in that directory under the `/hibernator-landing/` prefix. For example, if `servers.json` lives next to `index.html`, the page can load it from `/hibernator-landing/servers.json`.

## Template Placeholders

The NGINX module performs simple string replacement on `index.html` (and other `.html` files) to provide runtime information:

- `{{ETA_SECONDS}}`: The estimated time remaining until the service is ready, in seconds.
- `DURATION_MS`: The total expected startup duration in milliseconds.
- `DONE_MS`: The time elapsed since the startup sequence began, in milliseconds.
- `KEEP_ALIVE`: The configured keep-alive duration in seconds.

These can be used in your HTML or JavaScript to show progress bars or countdowns.

## Folder Layout

- `index.html` - Primary page returned while the service is starting.
- `assets/`, `style.css`, etc. - Static assets used by the page.

## Configuration

Point the NGINX module at this directory with `hibernator_landing_dir`:

```nginx
location / {
    hibernator on;
    hibernator_service_name my-app;
    hibernator_check_port 18081;
    hibernator_landing_dir /var/www/nginx-hibernator/landing;
    proxy_pass http://backend;
}
```

If `hibernator_landing_dir` is not set or the directory cannot be read, the module falls back to a built-in HTML page.

## Authoring Notes

- Keep all landing assets in the same directory (or subdirectories) so they can be served under `/hibernator-landing/`.
- Use relative URLs or the `/hibernator-landing/` prefix for local assets.
- If you use `{{ETA_SECONDS}}` in a script, make sure to handle it correctly as a number once it is replaced by the server.
