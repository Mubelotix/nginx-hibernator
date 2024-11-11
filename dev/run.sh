cargo build

sudo RUST_LOG=trace ./target/debug/nginx-site-hibernator dev/config.toml
