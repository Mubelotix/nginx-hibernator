# Random Gate Module

This folder contains the standalone Rust NGINX dynamic module.

## Build

```bash
cargo build --release
```

## Helper scripts

From repository root:

```bash
scripts/build-nginx.sh
scripts/test-random-gate.sh
scripts/run.sh start
```

The `scripts/` entries at repository root are wrappers that call `module/scripts/`.
