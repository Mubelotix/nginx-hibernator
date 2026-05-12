#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-$REPO_DIR/target/docker}"
DEB_PACKAGE_VERSION="${DEB_PACKAGE_VERSION:-$(sed -n 's/^version = "\(.*\)"$/\1/p' "$REPO_DIR/Cargo.toml" | head -n 1)}"

log() {
  printf '[debian-build.sh] %s\n' "$*"
}

if ! command -v docker >/dev/null 2>&1; then
  log "docker is required but was not found in PATH"
  exit 1
fi

dockerfile_path="${DOCKERFILE_PATH:-$REPO_DIR/Dockerfile}"
image_tag="${IMAGE_TAG:-nginx-hibernator-deb:local}"
container_name="nginx-hibernator-deb-$(date +%s)"

if [[ ! -f "$dockerfile_path" ]]; then
  log "Dockerfile not found at $dockerfile_path"
  exit 1
fi

if [[ -z "$DEB_PACKAGE_VERSION" ]]; then
  log "could not determine package version from Cargo.toml"
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

log "building image $image_tag from $dockerfile_path"
docker build \
  -f "$dockerfile_path" \
  --build-arg "DEB_PACKAGE_NAME=${DEB_PACKAGE_NAME:-nginx-hibernator-module}" \
  --build-arg "DEB_PACKAGE_VERSION=$DEB_PACKAGE_VERSION" \
  --build-arg "DEB_PACKAGE_RELEASE=${DEB_PACKAGE_RELEASE:-1}" \
  --build-arg "DEB_MAINTAINER=${DEB_MAINTAINER:-nginx-hibernator maintainers <maintainers@example.com>}" \
  --build-arg "DEB_DESCRIPTION=${DEB_DESCRIPTION:-NGINX dynamic module for automatic hibernation and wake-up of upstream services}" \
  -t "$image_tag" \
  "$REPO_DIR"

cleanup() {
  docker rm -f "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT

log "creating temporary container $container_name"
docker create --name "$container_name" "$image_tag" /bin/true >/dev/null

log "copying package artifacts into $OUTPUT_DIR"
docker cp "$container_name:/artifacts/." "$OUTPUT_DIR"

DEB_PATH="$(find "$OUTPUT_DIR" -maxdepth 1 -type f -name '*.deb' -printf '%T@ %p\n' | sort -nr | head -n 1 | cut -d' ' -f2- || true)"

if [[ -z "$DEB_PATH" ]]; then
  log "build completed but no .deb package was found"
  log "available files in $OUTPUT_DIR:"
  ls -1 "$OUTPUT_DIR"
  exit 1
fi

log "package ready: $DEB_PATH"
