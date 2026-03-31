#!/usr/bin/env bash
set -euo pipefail

MODULE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(cd "$MODULE_DIR/.." && pwd)"
NGINX_VERSION="${NGINX_VERSION:-1.28.3}"
BUILD_DIR="${BUILD_DIR:-$REPO_DIR/.local/build}"
NGINX_SRC_DIR="$BUILD_DIR/nginx-$NGINX_VERSION"
NGINX_PREFIX="${NGINX_PREFIX:-$REPO_DIR/.local/nginx}"
NGINX_TARBALL="$BUILD_DIR/nginx-$NGINX_VERSION.tar.gz"
TARGET_DIR="${CARGO_TARGET_DIR:-$MODULE_DIR/target}"
MODULE_SO="$TARGET_DIR/release/librandom_gate.so"

mkdir -p "$BUILD_DIR"

if [[ ! -d "$NGINX_SRC_DIR" ]]; then
  if [[ ! -f "$NGINX_TARBALL" ]]; then
    curl -fsSL "https://nginx.org/download/nginx-$NGINX_VERSION.tar.gz" -o "$NGINX_TARBALL"
  fi
  tar -xzf "$NGINX_TARBALL" -C "$BUILD_DIR"
fi

pushd "$NGINX_SRC_DIR" >/dev/null

if [[ ! -f objs/Makefile ]]; then
  ./configure \
    --prefix="$NGINX_PREFIX" \
    --with-compat \
    --with-http_ssl_module \
    --with-http_v2_module
fi

make -j"$(nproc)"
make install

popd >/dev/null

export NGINX_SOURCE_DIR="$NGINX_SRC_DIR"
export NGINX_BUILD_DIR="$NGINX_SRC_DIR/objs"

pushd "$MODULE_DIR" >/dev/null
cargo build --release
popd >/dev/null

mkdir -p "$NGINX_PREFIX/modules"
cp "$MODULE_SO" "$NGINX_PREFIX/modules/"

echo "Built nginx binary: $NGINX_PREFIX/sbin/nginx"
echo "Built module: $MODULE_SO"
echo "Installed module copy: $NGINX_PREFIX/modules/librandom_gate.so"
