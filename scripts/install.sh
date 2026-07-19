#!/usr/bin/env bash
# Gitka installer — universal Unix (Linux + macOS)
# Builds from source, installs to /usr/local/bin.
# Usage: curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash
#        curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash -s -- --prefix ~/.local

set -euo pipefail

REPO="https://github.com/SabeeirSharrma/gitka.git"
INSTALL_DIR="${1:-/usr/local/bin}"
BUILD_DIR="${TMPDIR:-/tmp}/gitka-build-$$"
SKIP_RUST_INSTALL="${SKIP_RUST_INSTALL:-}"

# ── Colors ──────────────────────────────────────────────────────────
info()  { printf "\033[1;34m▸\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m✓\033[0m %s\n" "$*"; }
warn()  { printf "\033[1;33m⚠\033[0m %s\n" "$*"; }
err()   { printf "\033[1;31m✗\033[0m %s\n" "$*" >&2; exit 1; }

cleanup() { rm -rf "$BUILD_DIR"; }
trap cleanup EXIT

# ── Detect OS ────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

# ── Preflight checks ────────────────────────────────────────────────
command -v git >/dev/null 2>&1 || err "git is required but not installed."

case "$OS" in
  Linux)
    PACKAGE_MANAGER=""
    if command -v apt-get >/dev/null 2>&1; then
      PACKAGE_MANAGER="apt-get"
    elif command -v dnf >/dev/null 2>&1; then
      PACKAGE_MANAGER="dnf"
    elif command -v yum >/dev/null 2>&1; then
      PACKAGE_MANAGER="yum"
    elif command -v pacman >/dev/null 2>&1; then
      PACKAGE_MANAGER="pacman"
    elif command -v zypper >/dev/null 2>&1; then
      PACKAGE_MANAGER="zypper"
    fi

    if ! command -v cargo >/dev/null 2>&1; then
      if [ -z "$SKIP_RUST_INSTALL" ]; then
        warn "Rust/Cargo not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
        export PATH="$CARGO_HOME/bin:$PATH"
      else
        err "Rust/Cargo not found. Install Rust first: https://rustup.rs"
      fi
    fi
    ;;

  Darwin)
    if ! command -v xcode-select >/dev/null 2>&1; then
      warn "Xcode Command Line Tools not found. Installing..."
      xcode-select --install 2>/dev/null || true
      echo ""
      echo "  ⏳ Please complete the Xcode CLT installation dialog, then re-run this script."
      echo ""
      exit 1
    fi

    if ! command -v cargo >/dev/null 2>&1; then
      if [ -z "$SKIP_RUST_INSTALL" ]; then
        warn "Rust/Cargo not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
        export PATH="$CARGO_HOME/bin:$PATH"
      else
        err "Rust/Cargo not found. Install Rust first: https://rustup.rs"
      fi
    fi

    # macOS needs OpenSSL for the vendored feature
    if ! command -v pkg-config >/dev/null 2>&1; then
      warn "pkg-config not found. Installing via Homebrew..."
      if ! command -v brew >/dev/null 2>&1; then
        err "Homebrew is required for pkg-config. Install it: https://brew.sh"
      fi
      brew install pkg-config
    fi
    ;;

  *)
    err "Unsupported operating system: $OS"
    ;;
esac

command -v cargo >/dev/null 2>&1 || err "Cargo still not available after install."
ok "Dependencies OK (${OS}/${ARCH})"

# ── Clone & build ────────────────────────────────────────────────────
info "Cloning Gitka..."
git clone --depth 1 "$REPO" "$BUILD_DIR"

info "Building release binary (this may take a few minutes)..."
(cd "$BUILD_DIR" && cargo build --release 2>/dev/null)

BINARY="$BUILD_DIR/target/release/gitka"
[ -f "$BINARY" ] || err "Build succeeded but binary not found at $BINARY"

# ── Install ──────────────────────────────────────────────────────────
info "Installing to $INSTALL_DIR/gitka..."
mkdir -p "$INSTALL_DIR" 2>/dev/null || true
if [ -w "$INSTALL_DIR" ]; then
  cp "$BINARY" "$INSTALL_DIR/gitka"
else
  sudo cp "$BINARY" "$INSTALL_DIR/gitka"
fi
chmod +x "$INSTALL_DIR/gitka"

ok "Installed to $INSTALL_DIR/gitka"

# ── Verify ───────────────────────────────────────────────────────────
if command -v gitka >/dev/null 2>&1; then
  VERSION=$(gitka --version 2>/dev/null || echo "unknown")
  ok "Gitka $VERSION is ready!"
else
  warn "Installed but 'gitka' not found in PATH."
  warn "Make sure $INSTALL_DIR is in your PATH."
fi

echo ""
echo "  Quick start:"
echo "    gitka init --target /mnt/usb --username <github-user> --token <pat>"
echo "    gitka scan && gitka sync"
echo "    gitka status"
echo ""
