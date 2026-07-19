#!/usr/bin/env bash
set -euo pipefail

# Gitka installer — builds from source and installs to /usr/local/bin
# Usage: curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash

REPO="https://github.com/SabeeirSharrma/gitka.git"
INSTALL_DIR="/usr/local/bin"
BUILD_DIR="${TMPDIR:-/tmp}/gitka-build-$$"

info()  { printf "\033[1;34m▸\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m✓\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33m⚠\033[0m %s\n" "$*"; }
err()   { printf "\033[1;31m✗\033[0m %s\n" "$*" >&2; exit 1; }

cleanup() { rm -rf "$BUILD_DIR"; }
trap cleanup EXIT

# ── Preflight checks ──────────────────────────────────────────────

command -v git  >/dev/null 2>&1 || err "git is required but not installed."
command -v cargo >/dev/null 2>&1 || {
  warn "Rust/Cargo not found."
  if command -v rustup >/dev/null 2>&1; then
    info "Installing Rust toolchain via rustup..."
    rustup default stable
  else
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # Source cargo env for this session
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export PATH="$CARGO_HOME/bin:$PATH"
  fi
  command -v cargo >/dev/null 2>&1 || err "Cargo still not available after install."
}
ok "Dependencies OK"

# ── Clone & build ─────────────────────────────────────────────────

info "Cloning Gitka..."
git clone --depth 1 "$REPO" "$BUILD_DIR"

info "Building release binary (this may take a few minutes)..."
cd "$BUILD_DIR"
cargo build --release 2>/dev/null

# ── Install ───────────────────────────────────────────────────────

BINARY="$BUILD_DIR/target/release/gitka"
[ -f "$BINARY" ] || err "Build succeeded but binary not found at $BINARY"

info "Installing to $INSTALL_DIR/gitka..."
if [ -w "$INSTALL_DIR" ]; then
  cp "$BINARY" "$INSTALL_DIR/gitka"
else
  sudo cp "$BINARY" "$INSTALL_DIR/gitka"
fi
chmod +x "$INSTALL_DIR/gitka"

ok "Installed to $INSTALL_DIR/gitka"

# ── Verify ────────────────────────────────────────────────────────

if command -v gitka >/dev/null 2>&1; then
  VERSION=$(gitka --version 2>/dev/null || echo "unknown")
  ok "Gitka $VERSION is ready!"
else
  warn "Installed but 'gitka' not found in PATH."
  warn "Make sure $INSTALL_DIR is in your PATH."
fi

echo ""
echo "  Quick start:"
echo "    gitka wipe --target /dev/sdb1 --username <github-user>"
echo "    gitka init --target /mnt/usb --username <github-user>"
echo "    gitka scan && gitka sync"
echo ""
