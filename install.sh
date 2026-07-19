#!/usr/bin/env bash
# Gitka installer — Linux & macOS
# Builds CLI and GUI from source, installs to /usr/local/bin (default).
#
# Usage:
#   curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash
#   curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash -s -- --prefix ~/.local

set -euo pipefail

REPO="SabeeirSharrma/gitka"
INSTALL_DIR="/usr/local/bin"
SKIP_RUST_INSTALL="${SKIP_RUST_INSTALL:-}"
CLI_ONLY="${CLI_ONLY:-}"

# Parse flags
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) INSTALL_DIR="$2"; shift 2 ;;
    --prefix=*) INSTALL_DIR="${1#*=}"; shift ;;
    --cli-only) CLI_ONLY=1; shift ;;
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

# ── Clone & build ────────────────────────────────────────────────────
BUILD_DIR="$HOME/.cache/gitka-install-$$"
trap 'rm -rf "$BUILD_DIR"' EXIT

info "Cloning Gitka..."
git clone --depth 1 "https://github.com/$REPO" "$BUILD_DIR" 2>/dev/null

# ── Build CLI ────────────────────────────────────────────────────────
info "Building CLI (this may take a few minutes)..."
cargo build --release --bin gitka --manifest-path "$BUILD_DIR/Cargo.toml" 2>&1

CLI_BIN="$BUILD_DIR/target/release/gitka"
if [ ! -f "$CLI_BIN" ]; then
  err "CLI build completed but binary not found at $CLI_BIN"
fi

info "Installing CLI to $INSTALL_DIR/gitka..."
mkdir -p "$INSTALL_DIR" 2>/dev/null || true
if [ -w "$INSTALL_DIR" ]; then
  install -m 755 "$CLI_BIN" "$INSTALL_DIR/gitka"
else
  sudo install -m 755 "$CLI_BIN" "$INSTALL_DIR/gitka"
fi
ok "CLI installed to $INSTALL_DIR/gitka"

# ── Build GUI (optional) ────────────────────────────────────────────
if [ -z "$CLI_ONLY" ] && [ -d "$BUILD_DIR/src-tauri" ]; then
  info "Building GUI..."

  # Install tauri-cli if not present
  if ! cargo tauri --version >/dev/null 2>&1; then
    info "Installing tauri-cli..."
    cargo install tauri-cli --locked 2>&1
  fi

  # cargo tauri build may fail at bundling (missing linuxdeploy) but the binary is built
  (cd "$BUILD_DIR/src-tauri" && cargo tauri build) 2>&1 || true

  # Find the built binary (location varies by platform)
  GUI_BIN=""
  if [ "$OS" = "Darwin" ]; then
    GUI_BIN="$BUILD_DIR/src-tauri/target/release/bundle/macos/Gitka.app/Contents/MacOS/gitka-gui"
  else
    GUI_BIN="$BUILD_DIR/src-tauri/target/release/gitka-gui"
  fi

  if [ -f "$GUI_BIN" ]; then
    if [ -w "$INSTALL_DIR" ]; then
      install -m 755 "$GUI_BIN" "$INSTALL_DIR/gitka-gui"
    else
      sudo install -m 755 "$GUI_BIN" "$INSTALL_DIR/gitka-gui"
    fi
    ok "GUI installed to $INSTALL_DIR/gitka-gui"
  else
    warn "GUI binary not found. CLI was installed successfully."
  fi
elif [ -n "$CLI_ONLY" ]; then
  info "Skipping GUI (--cli-only flag)"
fi

# ── Verify ───────────────────────────────────────────────────────────
if command -v gitka >/dev/null 2>&1; then
  VERSION=$(gitka --version 2>/dev/null || echo "unknown")
  ok "gitka $VERSION is ready!"
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
