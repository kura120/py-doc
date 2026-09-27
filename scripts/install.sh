#!/bin/sh
set -e

# Configuration
REPO="kura120/py-doc"
BINARY_NAME="py-doc"
INSTALL_DIR="/usr/local/bin"

# Detect OS and Architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$OS" = "darwin" ]; then
    TARGET="x86_64-apple-darwin"
elif [ "$OS" = "linux" ] && [ "$ARCH" = "x86_64" ]; then
    TARGET="x86_64-unknown-linux-gnu"
else
    echo "Unsupported platform: $OS-$ARCH"
    exit 1
fi

# Fetch the latest release tag
TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$TAG" ]; then
    echo "Failed to fetch the latest release tag."
    exit 1
fi

ASSET_NAME="${BINARY_NAME}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$ASSET_NAME"

echo "Downloading $BINARY_NAME $TAG for $TARGET..."
curl -L "$URL" -o "$ASSET_NAME"

echo "Extracting..."
tar -xzf "$ASSET_NAME"

echo "Installing to $INSTALL_DIR (may require sudo)..."
if [ -w "$INSTALL_DIR" ]; then
    mv "$BINARY_NAME" "$INSTALL_DIR/"
else
    sudo mv "$BINARY_NAME" "$INSTALL_DIR/"
fi

# Cleanup
rm "$ASSET_NAME"
echo "Successfully installed $BINARY_NAME!"