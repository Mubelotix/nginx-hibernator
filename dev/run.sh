cargo build

sudo RUST_LOG=trace ${CARGO_TARGET_DIR:-./target}/debug/nginx-hibernator dev/config.toml
