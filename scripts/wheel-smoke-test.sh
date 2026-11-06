#!/bin/sh
# Checks an installed swiftdoc wheel: `py-doc` is on PATH, reports the
# Cargo.toml version, and builds the sample fixture under --strict.
# Run from the repository root after `pip install swiftdoc`. POSIX sh, so it
# also runs in Alpine (musllinux) containers.
set -e

expected=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
actual=$(py-doc --version)
echo "$actual"
if [ "$actual" != "py-doc $expected" ]; then
    echo "expected 'py-doc $expected', got '$actual'" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# An empty --config keeps the repository's own pyproject.toml out of the run.
: >"$work/empty.toml"
py-doc --config "$work/empty.toml" --src tests/fixtures/sample --out "$work/site" --name sample --strict
test -f "$work/site/index.html"
echo "Wheel smoke test passed."
