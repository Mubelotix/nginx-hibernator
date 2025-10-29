# Landing Page Folder

This folder contains the landing page shown to users when a hibernated site is starting up.

## Structure

- `index.html` - The main landing page template served when a site is booting
- Any additional assets (CSS, JS, images, fonts, etc.) should be served by nginx

## Template Variables

The `index.html` file supports the following template variables that are replaced at runtime:

- `DONE_MS` - Milliseconds of boot time completed
- `DURATION_MS` - Estimated total boot time in milliseconds  
- `KEEP_ALIVE` - Keep-alive duration in seconds

## Serving Assets

The hibernator only serves `index.html` with template variable replacement. All other static assets (CSS, JS, images, etc.) should be served by nginx for better performance.

Configure nginx to serve the landing folder:

```nginx
location /landing/ {
    alias /path/to/landing/;
    expires 1h;
}
```

Then reference assets in your `index.html`:
```html
<link rel="stylesheet" href="/landing/style.css">
<script src="/landing/script.js"></script>
<img src="/landing/logo.png">
```

## Configuration

The landing folder path can be configured:

**Global (in config.toml):**
```toml
landing_folder = "./landing"
```

**Per-site override:**
```toml
[[sites]]
name = "my-site"
landing_folder = "/custom/path/to/landing"
# ... other config
```
