#!/bin/bash
set -euo pipefail

# gana installer
# Usage: curl -fsSL https://raw.githubusercontent.com/daern91/gana/master/install.sh | bash

REPO="daern91/gana"
INSTALL_DIR="${GANA_INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS="unknown-linux-gnu" ;;
    Darwin) OS="apple-darwin" ;;
    *)      echo "Error: Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64)  ARCH="x86_64" ;;
    aarch64) ARCH="aarch64" ;;
    arm64)   ARCH="aarch64" ;;
    *)       echo "Error: Unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"

# Get latest release tag
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST" ]; then
    echo "Error: Could not determine latest release"
    exit 1
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/gana-${TARGET}.tar.gz"

echo "Installing gana ${LATEST} for ${TARGET}..."

# Download and extract
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/gana.tar.gz"
tar -xzf "$TMP_DIR/gana.tar.gz" -C "$TMP_DIR"

# Install binary
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_DIR/gana" "$INSTALL_DIR/gana"
else
    sudo mv "$TMP_DIR/gana" "$INSTALL_DIR/gana"
fi

chmod +x "$INSTALL_DIR/gana"

echo ""
echo "  ☸ gana ${LATEST} installed to ${INSTALL_DIR}/gana"
echo ""
echo "  Run 'gana' to get started."
echo ""
