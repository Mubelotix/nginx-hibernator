#!/usr/bin/env bash
set -euo pipefail

MODULE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$(cd "$MODULE_DIR/.." && pwd)"
RUNTIME_DIR="${RUNTIME_DIR:-$REPO_DIR/.local/manual-run}"
NGINX_PREFIX="${NGINX_PREFIX:-$REPO_DIR/.local/nginx}"
NGINX_BIN="${NGINX_BIN:-$NGINX_PREFIX/sbin/nginx}"
TARGET_DIR="${CARGO_TARGET_DIR:-$MODULE_DIR/target}"
MODULE_SO="${MODULE_SO:-$TARGET_DIR/release/librandom_gate.so}"
MODULE_AUTO_BUILD="${MODULE_AUTO_BUILD:-1}"
LANDING_DIR="${LANDING_DIR:-$REPO_DIR/landing}"
BACKEND_PORT="${BACKEND_PORT:-18081}"
PROXY_PORT="${PROXY_PORT:-18080}"
SERVICE_NAME="${SERVICE_NAME:-hibernator-demo-backend}"
KEEPALIVE_SECS="${KEEPALIVE_SECS:-20}"
STARTUP_DELAY_SECS="${STARTUP_DELAY_SECS:-5}"

NGINX_PID_FILE="$RUNTIME_DIR/nginx.pid"
CONF_FILE="$RUNTIME_DIR/nginx.conf"
BACKEND_ROOT="$RUNTIME_DIR/backend-root"
SERVICE_FILE="/etc/systemd/system/$SERVICE_NAME.service"
ACCESS_LOG_FILE="$RUNTIME_DIR/logs/access.log"
PYTHON_BIN="${PYTHON_BIN:-$(command -v python3 || true)}"

cmd="${1:-up}"

if [[ "${EUID:-$(id -u)}" -ne 0 && "${HIBERNATOR_RUN_AS_ROOT:-0}" != "1" ]]; then
  if [[ "$MODULE_AUTO_BUILD" = "1" ]]; then
    printf '[run.sh] building module (MODULE_AUTO_BUILD=1) as user session\n'
    (cd "$MODULE_DIR" && cargo build --release)
    export MODULE_AUTO_BUILD=0
  elif [[ ! -f "$MODULE_SO" ]]; then
    printf '[run.sh] module not found, building release binary as user session\n'
    (cd "$MODULE_DIR" && cargo build --release)
    export MODULE_AUTO_BUILD=0
  fi

  exec sudo --preserve-env=PATH,CARGO_TARGET_DIR,MODULE_AUTO_BUILD,NGINX_BIN,NGINX_PREFIX,MODULE_SO,LANDING_DIR,BACKEND_PORT,PROXY_PORT,SERVICE_NAME,KEEPALIVE_SECS,STARTUP_DELAY_SECS,RUNTIME_DIR,PYTHON_BIN,HIBERNATOR_RUN_AS_ROOT \
    HIBERNATOR_RUN_AS_ROOT=1 "$0" "$@"
fi

log() {
  printf '[run.sh] %s\n' "$*"
}

pid_alive() {
  local pid="$1"
  kill -0 "$pid" >/dev/null 2>&1
}

read_pid() {
  local file="$1"
  if [[ -f "$file" ]]; then
    cat "$file"
  fi
}

ensure_prereqs() {
  if [[ ! -x "$NGINX_BIN" ]]; then
    log "nginx binary not found, running module/scripts/build-nginx.sh"
    "$MODULE_DIR/scripts/build-nginx.sh"
    return
  fi

  if [[ "$MODULE_AUTO_BUILD" = "1" ]]; then
    log "building module (MODULE_AUTO_BUILD=1)"
    (cd "$MODULE_DIR" && cargo build --release)
  elif [[ ! -f "$MODULE_SO" ]]; then
    log "module not found, building release binary"
    (cd "$MODULE_DIR" && cargo build --release)
  fi

  if [[ ! -d "$LANDING_DIR" ]]; then
    log "landing directory not found: $LANDING_DIR"
    return 1
  fi

  if [[ -z "$PYTHON_BIN" ]] || [[ ! -x "$PYTHON_BIN" ]]; then
    log "python3 not found; set PYTHON_BIN to a valid interpreter path"
    return 1
  fi

  if ! command -v systemctl >/dev/null 2>&1; then
    log "systemctl not found"
    return 1
  fi

  if ! [[ "$KEEPALIVE_SECS" =~ ^[0-9]+$ ]] || [[ "$KEEPALIVE_SECS" -lt 1 ]]; then
    log "KEEPALIVE_SECS must be a positive integer (got: $KEEPALIVE_SECS)"
    return 1
  fi

  if ! [[ "$STARTUP_DELAY_SECS" =~ ^[0-9]+$ ]]; then
    log "STARTUP_DELAY_SECS must be a non-negative integer (got: $STARTUP_DELAY_SECS)"
    return 1
  fi
}

write_conf() {
  mkdir -p "$RUNTIME_DIR" "$RUNTIME_DIR/logs" "$BACKEND_ROOT"

  cat > "$BACKEND_ROOT/index.html" <<EOF
ok-from-upstream
EOF

  cat > "$CONF_FILE" <<EOF
daemon off;
master_process off;
error_log $RUNTIME_DIR/logs/error.log debug;
pid $RUNTIME_DIR/logs/nginx.pid;

load_module $MODULE_SO;

events {}

http {
  access_log $ACCESS_LOG_FILE;

    upstream backend {
        server 127.0.0.1:$BACKEND_PORT;
    }

    server {
        listen 127.0.0.1:$PROXY_PORT;
        server_name localhost;

        location / {
            hibernator on;
            hibernator_service_name $SERVICE_NAME;
            hibernator_target_port $BACKEND_PORT;
            hibernator_keep_alive ${KEEPALIVE_SECS}s;
          hibernator_proxy_mode never;
            hibernator_landing_dir $LANDING_DIR;
            proxy_pass http://backend;
        }
    }
}
EOF
}

write_service_unit() {
  mkdir -p /etc/systemd/system

  tee "$SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Nginx hibernator demo backend ($SERVICE_NAME)
After=network.target

[Service]
Type=simple
ExecStartPre=/usr/bin/sleep $STARTUP_DELAY_SECS
ExecStart=$PYTHON_BIN -m http.server $BACKEND_PORT --bind 127.0.0.1 --directory $BACKEND_ROOT
WorkingDirectory=$BACKEND_ROOT
Restart=no
User=$(id -un)
Group=$(id -gn)

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload >/dev/null
}

service_active() {
  systemctl is-active --quiet "$SERVICE_NAME"
}

provision_backend_service() {
  write_service_unit
  systemctl enable "$SERVICE_NAME" >/dev/null || true
  log "backend service installed: $SERVICE_NAME"
}

stop_backend_service() {
  if service_active; then
    systemctl stop "$SERVICE_NAME" || true
    log "backend service stopped: $SERVICE_NAME"
  else
    log "backend service already stopped: $SERVICE_NAME"
  fi
}

start_nginx() {
  local existing
  existing="$(read_pid "$NGINX_PID_FILE" || true)"
  if [[ -n "$existing" ]] && pid_alive "$existing"; then
    log "nginx already running with pid $existing"
    return
  fi

  "$NGINX_BIN" -t -c "$CONF_FILE" -p "$RUNTIME_DIR" >/dev/null
  "$NGINX_BIN" -c "$CONF_FILE" -p "$RUNTIME_DIR" >"$RUNTIME_DIR/nginx.out" 2>&1 &
  echo "$!" > "$NGINX_PID_FILE"
  log "nginx started on 127.0.0.1:$PROXY_PORT (pid $!)"
}

stop_one() {
  local name="$1"
  local pid_file="$2"
  local pid

  pid="$(read_pid "$pid_file" || true)"
  if [[ -z "$pid" ]]; then
    log "$name not running (no pid file)"
    return
  fi

  if pid_alive "$pid"; then
    kill "$pid" >/dev/null 2>&1 || true
    sleep 0.2
    if pid_alive "$pid"; then
      kill -9 "$pid" >/dev/null 2>&1 || true
    fi
    log "$name stopped (pid $pid)"
  else
    log "$name pid file existed but process was not alive"
  fi

  rm -f "$pid_file"
}

status() {
  local npid
  npid="$(read_pid "$NGINX_PID_FILE" || true)"

  if service_active; then
    log "backend service: running ($SERVICE_NAME)"
  else
    log "backend service: stopped ($SERVICE_NAME)"
  fi

  if [[ -n "$npid" ]] && pid_alive "$npid"; then
    log "nginx: running (pid $npid)"
  else
    log "nginx: stopped"
  fi

  log "test url: http://127.0.0.1:$PROXY_PORT/"
}

up() {
  ensure_prereqs
  write_conf
  provision_backend_service
  stop_backend_service
  start_nginx

  log "manual test is ready"
  log "try: curl -i http://127.0.0.1:$PROXY_PORT/"
  log "backend service starts/stops from module logic"
  log "press Ctrl+C to stop nginx"

  cleanup() {
    stop_one "nginx" "$NGINX_PID_FILE"
  }
  trap cleanup EXIT INT TERM

  local npid
  npid="$(read_pid "$NGINX_PID_FILE")"
  wait "$npid"
}

start_detached() {
  ensure_prereqs
  write_conf
  provision_backend_service
  stop_backend_service
  start_nginx
  status
  log "tail logs: tail -f $RUNTIME_DIR/logs/error.log"
}

stop_all() {
  stop_one "nginx" "$NGINX_PID_FILE"
  stop_backend_service
}

usage() {
  cat <<EOF
Usage: $0 [up|start|stop|status|restart]

  up       Install backend service, start nginx, stay attached (default)
  start    Install backend service, start nginx detached
  stop     Stop nginx + backend service
  status   Show current status
  restart  Stop then start detached

Environment overrides:
  NGINX_BIN, NGINX_PREFIX, CARGO_TARGET_DIR, MODULE_SO
  MODULE_AUTO_BUILD=1|0
  LANDING_DIR, BACKEND_PORT, PROXY_PORT, RUNTIME_DIR
  SERVICE_NAME, KEEPALIVE_SECS, STARTUP_DELAY_SECS
EOF
}

case "$cmd" in
  up)
    up
    ;;
  start)
    start_detached
    ;;
  stop)
    stop_all
    ;;
  status)
    status
    ;;
  restart)
    stop_all
    start_detached
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage
    exit 2
    ;;
esac
