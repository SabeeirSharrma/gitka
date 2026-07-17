#!/usr/bin/env bash
set -euo pipefail

REPO="SabeeirSharrma/gitka"
BIN="gitka"
INSTALL_DIR="/usr/local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# Check for Rust
if ! command -v cargo &>/dev/null; then
    info "Rust not found. Installing temporarily..."
    TEMP_RUST=1
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet
    . "$HOME/.cargo/env"
else
    TEMP_RUST=0
fi

info "Building $BIN from source..."
cargo install --git "https://github.com/$REPO" --locked

# Check if we need sudo
if [ -w "$INSTALL_DIR" ]; then
    cp "$(which "$BIN")" "$INSTALL_DIR/$BIN"
else
    info "Installing to $INSTALL_DIR (requires sudo)..."
    sudo cp "$(which "$BIN")" "$INSTALL_DIR/$BIN"
fi

info "$BIN installed to $INSTALL_DIR/$BIN"

# Clean up temporary Rust if we installed it
if [ "${TEMP_RUST:-0}" -eq 1 ]; then
    info "Removing temporary Rust installation..."
    rustup self uninstall -y --quiet
fi

info "Done! Run '$BIN --help' to get started."
