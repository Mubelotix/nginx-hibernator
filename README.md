# NGINX Random Gate Module

This repository contains a Rust NGINX dynamic module that flips a random bool in the HTTP access phase.

- If the bool is true, request handling continues to upstream with `NGX_DECLINED`.
- If the bool is false, the module denies the request with `503 Service Unavailable`.

## Directive

Use inside an `http` `location` block:

- `random_gate on;`
- `random_gate off;`

## Build Module

```bash
cargo build --release
```

Output artifact on Linux:

- `target/release/librandom_gate.so`

## Build NGINX + Module

```bash
scripts/build-nginx.sh
```

This script:

1. downloads and builds NGINX,
2. installs it to `.local/nginx`,
3. builds the Rust module against the same NGINX source,
4. copies module to `.local/nginx/modules/librandom_gate.so`.

## Smoke Test

```bash
scripts/test-random-gate.sh
```

The smoke test starts a local upstream backend and an NGINX instance, sends repeated requests, and expects both response classes in one run:

- `200` (request reached upstream)
- `503` (request denied by module)

## Sample Configuration

A sample config is available at `nginx.conf`.
