#!/bin/sh
set -e

# Configuration
REPO="kura120/py-doc"
BINARY_NAME="py-doc"
INSTALL_DIR="/usr/local/bin"

# Detect OS and Architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Intel builds also run on Apple silicon through Rosetta, so they are the
# fallback when a release has no arm64 asset.
FALLBACK_TARGET=""
if [ "$OS" = "darwin" ] && [ "$ARCH" = "arm64" ]; then
    TARGET="aarch64-apple-darwin"
    FALLBACK_TARGET="x86_64-apple-darwin"
elif [ "$OS" = "darwin" ]; then
    TARGET="x86_64-apple-darwin"
elif [ "$OS" = "linux" ] && [ "$ARCH" = "x86_64" ]; then
    TARGET="x86_64-unknown-linux-gnu"
else
    echo "Unsupported platform: $OS-$ARCH"
    exit 1
fi

# Fetch the latest release tag
TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$TAG" ]; then
    echo "Failed to fetch the latest release tag."
    exit 1
fi

# Work in a private temporary directory, removed on any exit.
WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

download() {
    ASSET_NAME="${BINARY_NAME}-$1.tar.gz"
    URL="https://github.com/$REPO/releases/download/$TAG/$ASSET_NAME"
    echo "Downloading $BINARY_NAME $TAG for $1..."
    curl -fsSL "$URL" -o "$WORK_DIR/$ASSET_NAME"
}

if ! download "$TARGET"; then
    if [ -n "$FALLBACK_TARGET" ] && download "$FALLBACK_TARGET"; then
        TARGET="$FALLBACK_TARGET"
    else
        echo "Release $TAG has no download for $TARGET."
        exit 1
    fi
fi

# Verify the download against the checksum published with the release.
if curl -fsSL "$URL.sha256" -o "$WORK_DIR/$ASSET_NAME.sha256"; then
    EXPECTED=$(cut -d ' ' -f 1 < "$WORK_DIR/$ASSET_NAME.sha256")
    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL=$(sha256sum "$WORK_DIR/$ASSET_NAME" | cut -d ' ' -f 1)
    else
        ACTUAL=$(shasum -a 256 "$WORK_DIR/$ASSET_NAME" | cut -d ' ' -f 1)
    fi
    if [ "$EXPECTED" != "$ACTUAL" ]; then
        echo "Checksum mismatch for $ASSET_NAME: expected $EXPECTED, got $ACTUAL."
        exit 1
    fi
    echo "Checksum verified."
else
    echo "Warning: release $TAG publishes no checksum for $ASSET_NAME; skipping verification."
fi

echo "Extracting..."
tar -xzf "$WORK_DIR/$ASSET_NAME" -C "$WORK_DIR"

echo "Installing to $INSTALL_DIR (may require sudo)..."
if [ -w "$INSTALL_DIR" ]; then
    mv "$WORK_DIR/$BINARY_NAME" "$INSTALL_DIR/"
else
    sudo mv "$WORK_DIR/$BINARY_NAME" "$INSTALL_DIR/"
fi

echo "Successfully installed $BINARY_NAME $TAG!"
