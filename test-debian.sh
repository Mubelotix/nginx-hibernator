#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR"
DEB_FILE="$(find "$REPO_DIR/target/docker" -maxdepth 1 -name '*.deb' | head -n 1)"

if [[ -z "$DEB_FILE" ]]; then
  printf 'Error: No .deb package found in target/docker\n' >&2
  exit 1
fi

log() {
  printf '[test-debian.sh] %s\n' "$*"
}

# Create a temporary test container name
TEST_CONTAINER="nginx-hibernator-test-$$"
TEST_IMAGE="nginx-hibernator-test:$$"

cleanup() {
  docker rm -f "$TEST_CONTAINER" >/dev/null 2>&1 || true
  docker rmi -f "$TEST_IMAGE" >/dev/null 2>&1 || true
}
trap cleanup EXIT

cat > /tmp/Dockerfile.test <<'TESTEOF'
FROM debian:trixie

RUN apt-get update && apt-get install -y --no-install-recommends \
    nginx \
    curl \
    python3 \
    libdbus-1-3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY tests/test-service.py /opt/test-service/server.py
RUN chmod +x /opt/test-service/server.py
TESTEOF

log "building test image"
docker build -f /tmp/Dockerfile.test -t "$TEST_IMAGE" "$REPO_DIR"

log "creating test container"
docker create --name "$TEST_CONTAINER" \
  -p 8080:80 \
  -v "$DEB_FILE:/tmp/package.deb:ro" \
  "$TEST_IMAGE" \
  /bin/bash -c "tail -f /dev/null"

log "starting test container"
docker start "$TEST_CONTAINER"
sleep 1

log "installing the hibernator module package"
docker exec "$TEST_CONTAINER" bash -c 'apt-get update && apt-get install -y /tmp/package.deb && nginx -s stop 2>/dev/null || true'

sleep 1

log "configuring hibernator module for testing"
docker exec "$TEST_CONTAINER" bash -c '
cat > /etc/nginx/nginx.conf <<'\''EOF'\''
load_module /usr/lib/nginx/modules/libhibernator.so;

user www-data;
worker_processes auto;
pid /run/nginx.pid;

events {
  worker_connections 1024;
}

http {
  sendfile on;
  tcp_nopush on;
  types_hash_max_size 2048;

  include /etc/nginx/mime.types;
  default_type application/octet-stream;

  access_log /var/log/nginx/access.log;
  error_log /var/log/nginx/error.log;

  upstream test_backend {
    server 127.0.0.1:18081;
  }

  server {
    listen 80;
    server_name _;

    location / {
      hibernator on;
      hibernator_service_name test-service;
      hibernator_check_port 18081;
      hibernator_check_mode http;
      hibernator_check_timeout 100ms;
      hibernator_down_check_interval 100ms;
      hibernator_keep_alive 30s;
      hibernator_start_timeout 30s;
      hibernator_checkpoint on;

      proxy_pass http://test_backend;
    }
  }
}
EOF
'

log "starting nginx"
docker exec -d "$TEST_CONTAINER" sh -c 'nginx -g "daemon off;" &'

# Wait for nginx to start
sleep 3

log "test 1: first request should get checkpoint page (503) without starting the service"
http_code=$(docker exec "$TEST_CONTAINER" curl -s -o /tmp/response.html -w '%{http_code}' http://localhost:80/ 2>&1 || echo "FAIL")

if [[ "$http_code" == "503" ]]; then
  log "✓ Got 503 response as expected"
  body=$(docker exec "$TEST_CONTAINER" cat /tmp/response.html)
  if echo "$body" | grep -q "Enter Site"; then
    log "✓ Response contains checkpoint page content"
  else
    log "✗ Response does not contain checkpoint page content"
    exit 1
  fi
else
  log "✗ Expected 503, got $http_code"
  exit 1
fi

log "test 2: confirmation request should get landing page and start the service"
http_code=$(docker exec "$TEST_CONTAINER" curl -s -X POST -o /tmp/response.html -w '%{http_code}' http://localhost:80/ 2>&1 || echo "FAIL")

if [[ "$http_code" == "503" ]] && docker exec "$TEST_CONTAINER" grep -q "Server is Waking Up" /tmp/response.html; then
  log "✓ Confirmation received the landing page"
else
  log "✗ Confirmation did not receive the landing page"
  exit 1
fi

log "starting the test service in background now"
docker exec -d "$TEST_CONTAINER" /opt/test-service/server.py

log "test 3: waiting for service to start (5 seconds)"
sleep 5

log "test 4: second request should reach actual service"
http_code=$(docker exec "$TEST_CONTAINER" curl -s -o /tmp/response2.html -w '%{http_code}' http://localhost:80/ 2>&1 || echo "FAIL")

if [[ "$http_code" == "200" ]]; then
  log "✓ Got 200 response"
  body=$(docker exec "$TEST_CONTAINER" cat /tmp/response2.html)
  if echo "$body" | grep -q "Hello from test service"; then
    log "✓ Got response from actual service"
  else
    log "⚠ Response doesn't match expected text: $body"
  fi
else
  log "✗ Expected 200, got $http_code"
  docker exec "$TEST_CONTAINER" cat /tmp/response2.html 2>&1
  exit 1
fi

log "all tests passed!"
