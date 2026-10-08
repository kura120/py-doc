#!/bin/sh
set -e

echo "Building optimized release binary (opt-level=z, LTO, single codegen unit)..."
# Optional first argument: a Rust target triple (e.g. x86_64-apple-darwin).
TARGET="${1:-}"

if [ -n "$TARGET" ]; then
    cargo build --release --target "$TARGET"
    BIN_PATH="target/$TARGET/release/py-doc"
else
    cargo build --release
    BIN_PATH="target/release/py-doc"
fi

if [ ! -f "$BIN_PATH" ]; then
    echo "Build failed: binary not found at $BIN_PATH"
    exit 1
fi

echo ""
echo "Binary size before UPX:"
du -h "$BIN_PATH"

if command -v upx >/dev/null 2>&1; then
    echo ""
    echo "Compressing with UPX..."
    # macOS requires --force-macos flag
    if [ "$(uname)" = "Darwin" ]; then
        upx --best --lzma --force-macos "$BIN_PATH"
    else
        upx --best --lzma "$BIN_PATH"
    fi
    echo ""
    echo "Binary size after UPX:"
    du -h "$BIN_PATH"
else
    echo ""
    echo "UPX not found on PATH — skipping compression."
    echo "Install it to shrink further, e.g.:"
    echo "  macOS:  brew install upx"
    echo "  Debian/Ubuntu: sudo apt install upx-ucl"
fi

echo ""
echo "Done: $BIN_PATH"