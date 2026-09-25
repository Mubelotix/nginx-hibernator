#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR"
CONTAINER_CLI="${CONTAINER_CLI:-docker}"
DEB_FILE="${DEB_FILE:-$(find "$REPO_DIR/target/docker" -maxdepth 1 -name '*.deb' | head -n 1)}"

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
  "$CONTAINER_CLI" rm -f "$TEST_CONTAINER" >/dev/null 2>&1 || true
  "$CONTAINER_CLI" rmi -f "$TEST_IMAGE" >/dev/null 2>&1 || true
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
"$CONTAINER_CLI" build -f /tmp/Dockerfile.test -t "$TEST_IMAGE" "$REPO_DIR"

log "creating test container"
"$CONTAINER_CLI" create --name "$TEST_CONTAINER" \
  -p 8080:80 \
  -v "$DEB_FILE:/tmp/package.deb:ro" \
  "$TEST_IMAGE" \
  /bin/bash -c "tail -f /dev/null"

log "starting test container"
"$CONTAINER_CLI" start "$TEST_CONTAINER"
sleep 1

log "installing the hibernator module package"
"$CONTAINER_CLI" exec "$TEST_CONTAINER" bash -c 'apt-get update && apt-get install -y /tmp/package.deb && nginx -s stop 2>/dev/null || true'

sleep 1

log "configuring hibernator module for testing"
"$CONTAINER_CLI" exec "$TEST_CONTAINER" bash -c '
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

  upstream test_backend_one {
    server 127.0.0.1:18081;
  }

  upstream test_backend_two {
    server 127.0.0.1:18082;
  }

  server {
    listen 80;
    server_name _;

    location /one/ {
      hibernator on;
      hibernator_service_name test-service-one;
      hibernator_check_port 18081;
      hibernator_check_mode http;
      hibernator_check_timeout 100ms;
      hibernator_down_check_interval 100ms;
      hibernator_keep_alive 30s;
      hibernator_start_timeout 30s;
      hibernator_checkpoint on;

      proxy_pass http://test_backend_one/;
    }

    location /two/ {
      hibernator on;
      hibernator_service_name test-service-two;
      hibernator_check_port 18082;
      hibernator_check_mode http;
      hibernator_check_timeout 100ms;
      hibernator_down_check_interval 100ms;
      hibernator_keep_alive 30s;
      hibernator_start_timeout 30s;
      hibernator_checkpoint on;

      proxy_pass http://test_backend_two/;
    }
  }
}
EOF
'

log "starting nginx"
"$CONTAINER_CLI" exec -d "$TEST_CONTAINER" sh -c 'nginx -g "daemon off;" &'

# Wait for nginx to start
sleep 3

log "test 1: first request should get checkpoint page (503) without starting the service"
http_code=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" curl -s -o /tmp/response.html -w '%{http_code}' http://localhost:80/one/ 2>&1 || echo "FAIL")

if [[ "$http_code" == "503" ]]; then
  log "✓ Got 503 response as expected"
  body=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" cat /tmp/response.html)
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
http_code=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" curl -s -X POST -o /tmp/response.html -w '%{http_code}' http://localhost:80/one/ 2>&1 || echo "FAIL")

if [[ "$http_code" == "503" ]] && "$CONTAINER_CLI" exec "$TEST_CONTAINER" grep -q "Server is Waking Up" /tmp/response.html; then
  log "✓ Confirmation received the landing page"
else
  log "✗ Confirmation did not receive the landing page"
  exit 1
fi

log "test 3: second service should also show its checkpoint page"
http_code=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" curl -s -o /tmp/response.html -w '%{http_code}' http://localhost:80/two/ 2>&1 || echo "FAIL")
if [[ "$http_code" == "503" ]] && "$CONTAINER_CLI" exec "$TEST_CONTAINER" grep -q "Enter Site" /tmp/response.html; then
  log "✓ Second service returned its checkpoint page"
else
  log "✗ Second service did not return its checkpoint page (HTTP $http_code)"
  exit 1
fi

log "test 4: confirmation request for second service should get landing page"
http_code=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" curl -s -X POST -o /tmp/response.html -w '%{http_code}' http://localhost:80/two/ 2>&1 || echo "FAIL")
if [[ "$http_code" == "503" ]] && "$CONTAINER_CLI" exec "$TEST_CONTAINER" grep -q "Server is Waking Up" /tmp/response.html; then
  log "✓ Second service confirmation received the landing page"
else
  log "✗ Second service confirmation did not receive the landing page"
  exit 1
fi

log "starting both test services in background"
"$CONTAINER_CLI" exec -d "$TEST_CONTAINER" /opt/test-service/server.py 18081 "test service one"
"$CONTAINER_CLI" exec -d "$TEST_CONTAINER" /opt/test-service/server.py 18082 "test service two"

log "test 5: waiting for services to start (5 seconds)"
sleep 5

log "test 6: both requests should reach their actual services"
http_code=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" curl -s -o /tmp/response1.html -w '%{http_code}' http://localhost:80/one/ 2>&1 || echo "FAIL")

if [[ "$http_code" == "200" ]]; then
  log "✓ Got 200 response"
  body=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" cat /tmp/response1.html)
  if echo "$body" | grep -q "Hello from test service one"; then
    log "✓ First request reached its actual service"
  else
    log "✗ First response doesn't match expected text: $body"
    exit 1
  fi
else
  log "✗ Expected 200, got $http_code"
  "$CONTAINER_CLI" exec "$TEST_CONTAINER" cat /tmp/response1.html 2>&1
  exit 1
fi

http_code=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" curl -s -o /tmp/response2.html -w '%{http_code}' http://localhost:80/two/ 2>&1 || echo "FAIL")
if [[ "$http_code" == "200" ]]; then
  body=$("$CONTAINER_CLI" exec "$TEST_CONTAINER" cat /tmp/response2.html)
  if echo "$body" | grep -q "Hello from test service two"; then
    log "✓ Second request reached its actual service"
  else
    log "✗ Second response doesn't match expected text: $body"
    exit 1
  fi
else
  log "✗ Expected second service to return 200, got $http_code"
  "$CONTAINER_CLI" exec "$TEST_CONTAINER" cat /tmp/response2.html 2>&1
  exit 1
fi

log "all tests passed!"
