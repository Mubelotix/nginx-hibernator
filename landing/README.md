# Landing Page Folder

This folder contains the static landing page shown while the nginx module is
starting or waking a hibernated upstream.

## How It Is Served

The module reads `index.html` from the configured landing directory and serves
it directly when the upstream is unavailable.

It also serves any other files in that directory under the
`/hibernator-landing/` prefix. For example, if `servers.json` lives next to
`index.html`, the page can load it from `/hibernator-landing/servers.json`.

The module does not perform backend-style template substitution. `index.html`
should be written as a plain static page and any dynamic behavior should come
from client-side JavaScript.

## Folder Layout

- `index.html` - fallback page returned while the service is starting
- `servers.json`, `star.json`, and other static assets - served as files from
    the same directory

## Configuration

Point the nginx module at this directory with `hibernator_landing_dir`:

```nginx
location / {
    hibernator on;
    hibernator_service_name simple_python_http;
    hibernator_check_port 18081;
    hibernator_landing_dir /opt/nginx-hibernator/landing;
    proxy_pass http://backend;
}
```

If `hibernator_landing_dir` is not set or the directory cannot be read, the
module falls back to a built-in HTML page.

## Authoring Notes

- Keep all landing assets in the same directory so they can be served under
    `/hibernator-landing/`.
- Use relative URLs or the `/hibernator-landing/` prefix for local assets.
- Avoid relying on runtime variables injected by the server.
