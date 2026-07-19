#!/usr/bin/env bash
# Gitka uninstaller — Linux & macOS
# Removes gitka and gitka-gui binaries from the install directory.
#
# Usage:
#   curl -sSf https://sabeeir.qd.je/gitka/uninstall.sh | bash
#   gitka uninstall

set -euo pipefail

INSTALL_DIR="/usr/local/bin"

# Parse flags
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

REMOVED=0

# ── Remove CLI ──────────────────────────────────────────────────────
CLI_PATH="$INSTALL_DIR/gitka"
if [ -f "$CLI_PATH" ] || [ -L "$CLI_PATH" ]; then
  info "Removing $CLI_PATH..."
  if [ -w "$INSTALL_DIR" ]; then
    rm -f "$CLI_PATH"
  else
    sudo rm -f "$CLI_PATH"
  fi
  ok "Removed $CLI_PATH"
  REMOVED=1
fi

# ── Remove GUI ──────────────────────────────────────────────────────
GUI_PATH="$INSTALL_DIR/gitka-gui"
if [ -f "$GUI_PATH" ] || [ -L "$GUI_PATH" ]; then
  info "Removing $GUI_PATH..."
  if [ -w "$INSTALL_DIR" ]; then
    rm -f "$GUI_PATH"
  else
    sudo rm -f "$GUI_PATH"
  fi
  ok "Removed $GUI_PATH"
  REMOVED=1
fi

# ── Result ──────────────────────────────────────────────────────────
if [ "$REMOVED" -eq 0 ]; then
  warn "No gitka binaries found in $INSTALL_DIR"
else
  ok "Gitka uninstalled from $INSTALL_DIR"
fi
