#!/bin/sh
# Usage: build.sh [TARGET] [--upx]
#   TARGET  optional Rust target triple (e.g. x86_64-apple-darwin)
#   --upx   also compress the binary with UPX. Off by default: packed
#           binaries are unsupported on macOS and are often flagged by
#           antivirus tools on Windows.
set -e

TARGET=""
USE_UPX=0
for arg in "$@"; do
    case "$arg" in
        --upx) USE_UPX=1 ;;
        *) TARGET="$arg" ;;
    esac
done

echo "Building optimized release binary (opt-level=z, LTO, single codegen unit)..."
if [ -n "$TARGET" ]; then
    cargo build --release --locked --target "$TARGET"
    BIN_PATH="target/$TARGET/release/py-doc"
else
    cargo build --release --locked
    BIN_PATH="target/release/py-doc"
fi

if [ ! -f "$BIN_PATH" ]; then
    echo "Build failed: binary not found at $BIN_PATH"
    exit 1
fi

echo ""
echo "Binary size:"
du -h "$BIN_PATH"

if [ "$USE_UPX" = "1" ]; then
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
        echo "--upx was given but UPX is not on PATH. Install it, e.g.:"
        echo "  macOS:  brew install upx"
        echo "  Debian/Ubuntu: sudo apt install upx-ucl"
        exit 1
    fi
fi

echo ""
echo "Done: $BIN_PATH"
