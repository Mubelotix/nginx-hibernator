#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APT_REPO="${APT_REPO:-$REPO_DIR/apt-repo}"
DEB_FILE="${1:-}"

log() {
  printf '[apt-repo.sh] %s\n' "$*"
}

if [[ -z "$DEB_FILE" ]]; then
  log "Usage: $0 <path-to-.deb-file>"
  log "Example: $0 target/docker/nginx-hibernator-module_0.1.0-1_amd64.deb"
  exit 1
fi

if [[ ! -f "$DEB_FILE" ]]; then
  log "Error: $DEB_FILE not found"
  exit 1
fi

# Extract package info
PKG_NAME=$(dpkg -I "$DEB_FILE" | grep 'Package:' | awk '{print $2}')
PKG_ARCH=$(dpkg -I "$DEB_FILE" | grep 'Architecture:' | awk '{print $2}')

log "Adding $PKG_NAME to APT repository"
log "Package: $PKG_NAME"
log "Architecture: $PKG_ARCH"

# Create repository structure
mkdir -p "$APT_REPO/pool/main/n/$PKG_NAME"
mkdir -p "$APT_REPO/dists/trixie/main/binary-$PKG_ARCH"

# Copy package
cp "$DEB_FILE" "$APT_REPO/pool/main/n/$PKG_NAME/"
log "Copied package to $APT_REPO/pool/main/n/$PKG_NAME/"

# Generate Packages file
log "Generating Packages file..."
cd "$APT_REPO"
dpkg-scanpackages --multiversion pool/main/ > dists/trixie/main/binary-$PKG_ARCH/Packages
gzip -k dists/trixie/main/binary-$PKG_ARCH/Packages

# Generate Release file
log "Generating Release file..."
cat > dists/trixie/Release <<'EOF'
Origin: nginx-hibernator
Label: nginx-hibernator
Suite: trixie
Codename: trixie
Version: 13.0
Architectures: amd64 arm64
Components: main
Description: nginx-hibernator module packages
EOF

if ! command -v gpg >/dev/null 2>&1; then
  log "Error: gpg is required to sign the repository metadata"
  exit 1
fi

log "Signing Release and generating InRelease/Release.gpg..."
GPG_ARGS=(--batch --yes)
if [[ -n "${GPG_PASSPHRASE:-}" ]]; then
  GPG_ARGS+=(--pinentry-mode loopback --passphrase "$GPG_PASSPHRASE")
fi

gpg "${GPG_ARGS[@]}" --clearsign -o dists/trixie/InRelease dists/trixie/Release
gpg "${GPG_ARGS[@]}" --detach-sign -o dists/trixie/Release.gpg dists/trixie/Release

log "Exporting public key for apt clients..."
gpg --armor --export > dists/trixie/repo-public.key

cd - >/dev/null

log "✅ APT repository updated"
log ""
log "To use this repository locally:"
log "  curl -fsSL file://$APT_REPO/dists/trixie/repo-public.key | sudo gpg --dearmor -o /usr/share/keyrings/nginx-hibernator.gpg"
log "  echo 'deb [signed-by=/usr/share/keyrings/nginx-hibernator.gpg] file://$APT_REPO trixie main' | sudo tee /etc/apt/sources.list.d/nginx-hibernator-local.list"
log "  sudo apt-get update"
log "  sudo apt-get install nginx-hibernator-module"
