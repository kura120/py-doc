#!/usr/bin/env bash
# Installs py-doc for the GitHub Action in action.yml. Not meant to be run
# by hand. Settings come from the environment:
#   PY_DOC_INSTALL  "release" (download a published binary) or "source"
#                   (cargo install from the action's own checkout)
#   PY_DOC_VERSION  "latest", "v0.5.1" or "0.5.1" (release installs only)
#   PY_DOC_TOKEN    token for the GitHub API, to avoid anonymous rate limits
# The binary goes to $RUNNER_TEMP/py-doc/bin, which is added to $GITHUB_PATH.
set -eo pipefail

REPO="kura120/py-doc"
ROOT="$RUNNER_TEMP/py-doc"
BIN_DIR="$ROOT/bin"
WORK=""
mkdir -p "$BIN_DIR"

cleanup() {
    if [ -n "$WORK" ]; then
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT

install_source() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "::error::install: source needs a Rust toolchain on the runner (for example dtolnay/rust-toolchain@stable)."
        exit 1
    fi
    if [ -n "$PY_DOC_VERSION" ] && [ "$PY_DOC_VERSION" != "latest" ]; then
        echo "::notice::install: source builds the action's own checkout; the version input ($PY_DOC_VERSION) is ignored."
    fi
    cargo install --locked --path "$GITHUB_ACTION_PATH" --root "$ROOT"
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

# fetch URL FILE
fetch() {
    curl -fsSL --retry 3 -o "$2" "$1"
}

install_release() {
    # Same asset names as .github/workflows/release.yaml and scripts/install.sh.
    # Intel builds also run on Apple silicon (Rosetta) and on Windows on Arm
    # (emulation), so they are the fallback where no arm64 asset exists.
    local target fallback="" ext="tar.gz" exe="py-doc"
    case "$RUNNER_OS/$RUNNER_ARCH" in
        Linux/X64) target="x86_64-unknown-linux-gnu" ;;
        macOS/ARM64)
            target="aarch64-apple-darwin"
            fallback="x86_64-apple-darwin"
            ;;
        macOS/X64) target="x86_64-apple-darwin" ;;
        Windows/X64 | Windows/ARM64)
            target="x86_64-pc-windows-msvc"
            ext="zip"
            exe="py-doc.exe"
            ;;
        *)
            echo "::error::No py-doc release binary for $RUNNER_OS/$RUNNER_ARCH. Use 'install: source' with a Rust toolchain instead."
            exit 1
            ;;
    esac

    local tag
    if [ -z "$PY_DOC_VERSION" ] || [ "$PY_DOC_VERSION" = "latest" ]; then
        local auth=()
        if [ -n "$PY_DOC_TOKEN" ]; then
            auth=(-H "Authorization: Bearer $PY_DOC_TOKEN")
        fi
        tag=$(curl -fsSL --retry 3 "${auth[@]}" "https://api.github.com/repos/$REPO/releases/latest" |
            grep '"tag_name":' | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/')
        if [ -z "$tag" ]; then
            echo "::error::Could not look up the latest py-doc release."
            exit 1
        fi
    else
        tag="v${PY_DOC_VERSION#v}"
    fi

    WORK=$(mktemp -d)
    local asset url
    asset="py-doc-$target.$ext"
    url="https://github.com/$REPO/releases/download/$tag/$asset"
    echo "Downloading py-doc $tag for $target..."
    if ! fetch "$url" "$WORK/$asset"; then
        if [ -z "$fallback" ]; then
            echo "::error::Release $tag has no download for $target."
            exit 1
        fi
        target="$fallback"
        asset="py-doc-$target.$ext"
        url="https://github.com/$REPO/releases/download/$tag/$asset"
        echo "No $tag build for this runner; trying $target..."
        if ! fetch "$url" "$WORK/$asset"; then
            echo "::error::Release $tag has no download for $RUNNER_OS/$RUNNER_ARCH."
            exit 1
        fi
    fi

    # Verify against the checksum published with the release. Releases from
    # before checksums were added have none; those get a warning, as in
    # scripts/install.sh. A mismatch always fails.
    if fetch "$url.sha256" "$WORK/$asset.sha256"; then
        local expected actual
        expected=$(cut -d ' ' -f 1 <"$WORK/$asset.sha256" | tr 'A-F' 'a-f')
        actual=$(sha256_of "$WORK/$asset" | tr 'A-F' 'a-f')
        if [ "$expected" != "$actual" ]; then
            echo "::error::Checksum mismatch for $asset: expected $expected, got $actual."
            exit 1
        fi
        echo "Checksum verified."
    else
        echo "::warning::Release $tag publishes no checksum for $asset; skipping verification."
    fi

    if [ "$ext" = "zip" ]; then
        if command -v unzip >/dev/null 2>&1; then
            unzip -q -o "$WORK/$asset" -d "$WORK"
        else
            pwsh -NoProfile -Command "Expand-Archive -Force -Path '$WORK/$asset' -DestinationPath '$WORK'"
        fi
    else
        tar -xzf "$WORK/$asset" -C "$WORK"
    fi
    mv -f "$WORK/$exe" "$BIN_DIR/$exe"
    chmod +x "$BIN_DIR/$exe"
}

case "$PY_DOC_INSTALL" in
    release) install_release ;;
    source) install_source ;;
    *)
        echo "::error::Unknown install value '$PY_DOC_INSTALL'; use 'release' or 'source'."
        exit 1
        ;;
esac

echo "$BIN_DIR" >>"$GITHUB_PATH"
exe_path="$BIN_DIR/py-doc"
if [ -f "$exe_path.exe" ]; then
    exe_path="$exe_path.exe"
fi
version=$("$exe_path" --version | sed -E 's/^py-doc //')
echo "Installed py-doc $version"
echo "version=$version" >>"$GITHUB_OUTPUT"
