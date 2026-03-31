#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_DIR="${RUNTIME_DIR:-$ROOT_DIR/.local/manual-run}"
NGINX_PREFIX="${NGINX_PREFIX:-$ROOT_DIR/.local/nginx}"
NGINX_BIN="${NGINX_BIN:-$NGINX_PREFIX/sbin/nginx}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
MODULE_SO="${MODULE_SO:-$TARGET_DIR/release/librandom_gate.so}"
BACKEND_PORT="${BACKEND_PORT:-18081}"
PROXY_PORT="${PROXY_PORT:-18080}"

BACKEND_PID_FILE="$RUNTIME_DIR/backend.pid"
NGINX_PID_FILE="$RUNTIME_DIR/nginx.pid"
CONF_FILE="$RUNTIME_DIR/nginx.conf"
BACKEND_ROOT="$RUNTIME_DIR/backend-root"

cmd="${1:-up}"

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
  if [[ ! -x "$NGINX_BIN" || ! -f "$MODULE_SO" ]]; then
    log "nginx or module not found, running scripts/build-nginx.sh"
    "$ROOT_DIR/scripts/build-nginx.sh"
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
    access_log $RUNTIME_DIR/logs/access.log;

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
}

start_backend() {
  local existing
  existing="$(read_pid "$BACKEND_PID_FILE" || true)"
  if [[ -n "$existing" ]] && pid_alive "$existing"; then
    log "backend already running with pid $existing"
    return
  fi

  python3 -m http.server "$BACKEND_PORT" --bind 127.0.0.1 --directory "$BACKEND_ROOT" \
    >"$RUNTIME_DIR/backend.out" 2>&1 &
  echo "$!" > "$BACKEND_PID_FILE"
  log "backend started on 127.0.0.1:$BACKEND_PORT (pid $!)"
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
  local bpid npid
  bpid="$(read_pid "$BACKEND_PID_FILE" || true)"
  npid="$(read_pid "$NGINX_PID_FILE" || true)"

  if [[ -n "$bpid" ]] && pid_alive "$bpid"; then
    log "backend: running (pid $bpid)"
  else
    log "backend: stopped"
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
  start_backend
  start_nginx

  log "manual test is ready"
  log "try: curl -i http://127.0.0.1:$PROXY_PORT/"
  log "press Ctrl+C to stop both services"

  cleanup() {
    stop_one "nginx" "$NGINX_PID_FILE"
    stop_one "backend" "$BACKEND_PID_FILE"
  }
  trap cleanup EXIT INT TERM

  local npid
  npid="$(read_pid "$NGINX_PID_FILE")"
  wait "$npid"
}

start_detached() {
  ensure_prereqs
  write_conf
  start_backend
  start_nginx
  status
  log "tail logs: tail -f $RUNTIME_DIR/logs/error.log"
}

stop_all() {
  stop_one "nginx" "$NGINX_PID_FILE"
  stop_one "backend" "$BACKEND_PID_FILE"
}

usage() {
  cat <<EOF
Usage: $0 [up|start|stop|status|restart]

  up       Start backend+nginx and stay attached (default)
  start    Start backend+nginx detached
  stop     Stop backend+nginx
  status   Show current status
  restart  Stop then start detached

Environment overrides:
  NGINX_BIN, NGINX_PREFIX, CARGO_TARGET_DIR, MODULE_SO
  BACKEND_PORT, PROXY_PORT, RUNTIME_DIR
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
