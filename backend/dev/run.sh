cargo build

sudo chown root:root dev/config.toml
sudo RUST_LOG=trace ${CARGO_TARGET_DIR:-./target}/debug/nginx-hibernator dev/config.toml
