#!/usr/bin/env bash
set -euo pipefail

# Gitka installer — Linux & macOS
# Builds from source, installs gitka to /usr/local/bin.
# Usage: curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash
#        curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash -s -- --prefix ~/.local

REPO="https://github.com/SabeeirSharrma/gitka.git"
INSTALL_DIR="/usr/local/bin"
BUILD_DIR="${TMPDIR:-/tmp}/gitka-build-$$"
SKIP_RUST_INSTALL="${SKIP_RUST_INSTALL:-}"

# Parse --prefix
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) INSTALL_DIR="$2"; shift 2 ;;
    --prefix=*) INSTALL_DIR="${1#*=}"; shift ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ── Colors ──────────────────────────────────────────────────────────
info()  { printf "\033[1;34m▸\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m✓\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33m⚠\033[0m %s\n" "$*"; }
err()   { printf "\033[1;31m✗\033[0m %s\n" "$*" >&2; exit 1; }

cleanup() { rm -rf "$BUILD_DIR"; }
trap cleanup EXIT

# ── Detect OS ────────────────────────────────────────────────────────
OS="$(uname -s)"

case "$OS" in
  Linux)
    ;;
  Darwin)
    # macOS: ensure Xcode CLT and pkg-config are present
    if ! command -v xcode-select >/dev/null 2>&1; then
      warn "Xcode Command Line Tools not found."
      xcode-select --install 2>/dev/null || true
      echo ""
      echo "  ⏳ Complete the installation dialog, then re-run this script."
      echo ""
      exit 1
    fi
    if ! command -v pkg-config >/dev/null 2>&1 && command -v brew >/dev/null 2>&1; then
      brew install pkg-config 2>/dev/null || true
    fi
    ;;
  *)
    err "Unsupported OS: $OS. On Windows, use the PowerShell installer:"
    err "  irm https://sabeeir.qd.je/gitka/install-windows.ps1 | iex"
    ;;
esac

# ── Preflight checks ──────────────────────────────────────────────

command -v git >/dev/null 2>&1 || err "git is required but not installed."

if ! command -v cargo >/dev/null 2>&1; then
  if [ -z "$SKIP_RUST_INSTALL" ]; then
    warn "Rust/Cargo not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export PATH="$CARGO_HOME/bin:$PATH"
  else
    err "Rust/Cargo is required. Install it first: https://rustup.rs"
  fi
fi
command -v cargo >/dev/null 2>&1 || err "Cargo still not available after install."
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
mkdir -p "$INSTALL_DIR" 2>/dev/null || true
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
echo "    gitka init --target /mnt/usb --username <user> --token <pat>"
echo "    gitka scan && gitka sync"
echo "    gitka status"
echo "    gitka --help"
echo ""
