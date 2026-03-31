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

By default, `module/scripts/run.sh` and `module/scripts/test-random-gate.sh` rebuild the module before running.
Set `MODULE_AUTO_BUILD=0` to skip rebuilding.

`module/scripts/run.sh` configures `random_gate_landing_dir` so denied (`503`) requests are served directly from a landing folder by the module.
By default it uses `./landing` from the repository root; override with `LANDING_DIR=/path/to/landing`.
