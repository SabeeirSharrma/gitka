#!/usr/bin/env bash
# Gitka installer — Linux & macOS
# Builds from source using cargo, installs gitka to /usr/local/bin (default).
#
# Usage:
#   curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash
#   curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash -s -- --prefix ~/.local

set -euo pipefail

REPO="SabeeirSharrma/gitka"
BIN="gitka"
INSTALL_DIR="/usr/local/bin"
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
    # pkg-config may be needed for vendored openssl on some macOS versions
    if ! command -v pkg-config >/dev/null 2>&1 && command -v brew >/dev/null 2>&1; then
      brew install pkg-config 2>/dev/null || true
    fi
    ;;
  *)
    err "Unsupported OS: $OS. On Windows, use the PowerShell installer:"
    err "  irm https://sabeeir.qd.je/gitka/install-windows.ps1 | iex"
    ;;
esac

# ── Install Rust if missing ──────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
  if [ -z "$SKIP_RUST_INSTALL" ]; then
    info "Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    . "$HOME/.cargo/env"
  else
    err "Rust/Cargo not found. Install it first: https://rustup.rs"
  fi
fi

command -v cargo >/dev/null 2>&1 || err "Cargo still not available."
ok "Dependencies OK"

# ── Build & install ──────────────────────────────────────────────────
info "Building $BIN from source (this may take a few minutes)..."
cargo install --git "https://github.com/$REPO" --locked 2>&1

BIN_PATH="$(which "$BIN" 2>/dev/null || echo "$HOME/.cargo/bin/$BIN")"
if [ ! -f "$BIN_PATH" ]; then
  err "Build completed but $BIN binary not found. Check your cargo bin directory."
fi

info "Installing to $INSTALL_DIR/$BIN..."
mkdir -p "$INSTALL_DIR" 2>/dev/null || true
if [ -w "$INSTALL_DIR" ]; then
  cp "$BIN_PATH" "$INSTALL_DIR/$BIN"
else
  sudo cp "$BIN_PATH" "$INSTALL_DIR/$BIN"
fi
chmod +x "$INSTALL_DIR/$BIN"

ok "$BIN installed to $INSTALL_DIR/$BIN"

# ── Verify ───────────────────────────────────────────────────────────
if command -v "$BIN" >/dev/null 2>&1; then
  VERSION=$("$BIN" --version 2>/dev/null || echo "unknown")
  ok "$BIN $VERSION is ready!"
else
  warn "Installed but '$BIN' not found in PATH."
  warn "Make sure $INSTALL_DIR is in your PATH."
fi

echo ""
echo "  Quick start:"
echo "    gitka init --target /mnt/usb --username <user> --token <pat>"
echo "    gitka scan && gitka sync"
echo "    gitka status"
echo "    gitka --help"
echo ""
