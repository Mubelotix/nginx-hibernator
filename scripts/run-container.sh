#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PODMAN="${PODMAN:-podman}"
PROXY_PORT="${PROXY_PORT:-18080}"
SERVICE_NAME="hibernator-demo-backend"
CONTAINER_NAME="nginx-hibernator-run-$$"
SYSTEMD_IMAGE="nginx-hibernator-systemd:local"
PACKAGE_IMAGE="nginx-hibernator-deb:local"

log() {
  printf '[run-container.sh] %s\n' "$*"
}

require_podman() {
  if ! command -v "$PODMAN" >/dev/null 2>&1; then
    log "Podman is required; set PODMAN to its executable path"
    exit 1
  fi
}

build_package() {
  # debian-build.sh uses the Docker CLI spelling; forward it to Podman.
  docker() {
    "$PODMAN" "$@"
  }
  export -f docker
  export PODMAN

  IMAGE_TAG="$PACKAGE_IMAGE" "$SCRIPT_DIR/debian-build.sh"
}

latest_package() {
  find "$REPO_DIR/target/docker" -maxdepth 1 -type f -name '*.deb' -printf '%T@ %p\n' |
    sort -nr |
    head -n 1 |
    cut -d' ' -f2-
}

wait_for_systemd() {
  local attempt

  for attempt in {1..20}; do
    if "$PODMAN" exec "$CONTAINER_NAME" systemctl is-active --quiet dbus; then
      return
    fi
    sleep 1
  done

  log "systemd did not start in the container"
  "$PODMAN" logs "$CONTAINER_NAME" >&2 || true
  return 1
}

write_nginx_config() {
  "$PODMAN" exec -i "$CONTAINER_NAME" tee /etc/nginx/nginx.conf >/dev/null <<'EOF'
load_module /usr/lib/nginx/modules/libhibernator.so;

# The module needs system-bus permission to start the demo service.
user root;

error_log /dev/stderr info;

events {}

http {
    access_log /dev/stdout;

    upstream backend {
        server 127.0.0.1:18081;
    }

    server {
        listen 80;
        server_name localhost;

        location / {
            hibernator on;
            hibernator_service_name hibernator-demo-backend;
            hibernator_check_port 18081;
            hibernator_keep_alive 20s;
            hibernator_landing_dir /opt/nginx-hibernator/landing;
            hibernator_checkpoint on;
            proxy_pass http://backend;
        }
    }
}
EOF
}

write_service_unit() {
  "$PODMAN" exec -i "$CONTAINER_NAME" tee "/etc/systemd/system/$SERVICE_NAME.service" >/dev/null <<'EOF'
[Unit]
Description=Nginx hibernator demo backend
After=network.target

[Service]
Type=simple
ExecStartPre=/usr/bin/sleep 10
ExecStart=/usr/bin/python3 /opt/nginx-hibernator/test-service.py
Restart=no
EOF
}

provision_container() {
  local package

  package="$(latest_package)"
  if [[ -z "$package" ]]; then
    log "package build completed without a .deb artifact"
    return 1
  fi

  log "building the systemd development image"
  "$PODMAN" build --tag "$SYSTEMD_IMAGE" --file "$REPO_DIR/trixie-systemd.Dockerfile" "$REPO_DIR"

  log "starting disposable systemd container $CONTAINER_NAME"
  "$PODMAN" run --detach --name "$CONTAINER_NAME" --systemd=always \
    --cgroupns=host \
    --tmpfs /run \
    --tmpfs /run/lock \
    --volume /sys/fs/cgroup:/sys/fs/cgroup:rw \
    --publish "127.0.0.1:$PROXY_PORT:80" \
    "$SYSTEMD_IMAGE" >/dev/null
  wait_for_systemd

  "$PODMAN" exec "$CONTAINER_NAME" mkdir --parents /opt/nginx-hibernator
  "$PODMAN" cp "$package" "$CONTAINER_NAME:/tmp/hibernator.deb"
  "$PODMAN" cp "$REPO_DIR/tests/test-service.py" "$CONTAINER_NAME:/opt/nginx-hibernator/test-service.py"
  "$PODMAN" cp "$REPO_DIR/landing" "$CONTAINER_NAME:/opt/nginx-hibernator/landing"
  "$PODMAN" exec "$CONTAINER_NAME" apt-get install --yes /tmp/hibernator.deb
  "$PODMAN" exec "$CONTAINER_NAME" systemctl stop nginx

  write_nginx_config
  write_service_unit
  "$PODMAN" exec "$CONTAINER_NAME" systemctl daemon-reload
  "$PODMAN" exec "$CONTAINER_NAME" nginx -t
  "$PODMAN" exec "$CONTAINER_NAME" nginx
}

LOG_PID=""

cleanup() {
  if [[ -n "$LOG_PID" ]] && kill -0 "$LOG_PID" >/dev/null 2>&1; then
    kill "$LOG_PID" >/dev/null 2>&1 || true
  fi
  "$PODMAN" rm --force "$CONTAINER_NAME" >/dev/null 2>&1 || true
}

require_podman
trap cleanup EXIT INT TERM

log "building the Debian module package with Podman"
build_package
provision_container

"$PODMAN" logs --follow "$CONTAINER_NAME" &
LOG_PID="$!"

log "checkpoint demo is ready at http://127.0.0.1:$PROXY_PORT/"
log "press Ctrl+C to remove the container"
"$PODMAN" wait "$CONTAINER_NAME" >/dev/null
