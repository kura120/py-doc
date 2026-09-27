#!/bin/sh
set -e

echo "Building optimized release binary (opt-level=z, LTO, single codegen unit)..."
cargo build --release

BIN_PATH="target/release/py-doc"

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
    upx --best --lzma "$BIN_PATH"
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