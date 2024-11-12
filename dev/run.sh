cargo build

sudo RUST_LOG=trace $CARGO_TARGET_DIR/debug/nginx-hibernator dev/config.toml
