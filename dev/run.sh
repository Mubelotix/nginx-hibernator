cargo build

sudo RUST_LOG=trace $CARGO_TARGET_DIR/debug/nginx-site-hibernator dev/config.toml
