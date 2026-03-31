#!/usr/bin/env bash
set -euo pipefail

MODULE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(cd "$MODULE_DIR/.." && pwd)"
NGINX_PREFIX="${NGINX_PREFIX:-$REPO_DIR/.local/nginx}"
NGINX_BIN="${NGINX_BIN:-$NGINX_PREFIX/sbin/nginx}"
TARGET_DIR="${CARGO_TARGET_DIR:-$MODULE_DIR/target}"
MODULE_SO="${MODULE_SO:-$TARGET_DIR/release/librandom_gate.so}"
MODULE_AUTO_BUILD="${MODULE_AUTO_BUILD:-1}"
REQUESTS="${REQUESTS:-100}"
WORK_DIR="${WORK_DIR:-$REPO_DIR/.local/test-run}"
BACKEND_PORT="${BACKEND_PORT:-18081}"
PROXY_PORT="${PROXY_PORT:-18080}"

if [[ ! -x "$NGINX_BIN" ]]; then
  echo "nginx binary not found at $NGINX_BIN"
  echo "Run scripts/build-nginx.sh first or set NGINX_BIN."
  exit 1
fi

if [[ "$MODULE_AUTO_BUILD" = "1" || ! -f "$MODULE_SO" ]]; then
  echo "Building release module."
  (cd "$MODULE_DIR" && cargo build --release)
fi

mkdir -p "$WORK_DIR/html"
mkdir -p "$WORK_DIR/logs"
echo "ok-from-upstream" > "$WORK_DIR/html/index.html"

BACKEND_PID=""
NGINX_PID=""

cleanup() {
  set +e
  if [[ -n "$NGINX_PID" ]]; then
    kill "$NGINX_PID" >/dev/null 2>&1 || true
    wait "$NGINX_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
    wait "$BACKEND_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

python3 -m http.server "$BACKEND_PORT" --bind 127.0.0.1 --directory "$WORK_DIR/html" >/dev/null 2>&1 &
BACKEND_PID="$!"

cat > "$WORK_DIR/nginx.conf" <<EOF
daemon off;
master_process off;
error_log $WORK_DIR/error.log debug;
pid $WORK_DIR/nginx.pid;

load_module $MODULE_SO;

events {}

http {
    access_log $WORK_DIR/access.log;

    upstream backend {
        server 127.0.0.1:$BACKEND_PORT;
    }

    server {
        listen 127.0.0.1:$PROXY_PORT;
        server_name localhost;

        location / {
            random_gate on;
            proxy_pass http://backend;
        }
    }
}
EOF

"$NGINX_BIN" -t -c "$WORK_DIR/nginx.conf" -p "$WORK_DIR"
"$NGINX_BIN" -c "$WORK_DIR/nginx.conf" -p "$WORK_DIR" > "$WORK_DIR/nginx.out" 2>&1 &
NGINX_PID="$!"

sleep 0.5

allowed=0
denied=0
other=0

for _ in $(seq 1 "$REQUESTS"); do
  code="$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PROXY_PORT/")"
  case "$code" in
    200)
      allowed=$((allowed + 1))
      ;;
    503)
      denied=$((denied + 1))
      ;;
    *)
      other=$((other + 1))
      ;;
  esac
done

echo "requests=$REQUESTS allowed_200=$allowed denied_503=$denied other=$other"

if [[ "$allowed" -eq 0 || "$denied" -eq 0 || "$other" -ne 0 ]]; then
  echo "Unexpected result distribution. Check $WORK_DIR/error.log"
  exit 2
fi

echo "Smoke test passed: both upstream and denied responses observed."
