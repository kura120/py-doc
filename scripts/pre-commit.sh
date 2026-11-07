#!/bin/sh
# pre-commit hook: runs `swiftdoc --strict` and throws the site away, so a
# broken doc-link, macro or image fails the commit. Uses the swiftdoc on PATH.
# Any hook args are passed to swiftdoc. --src defaults to ".", --name to the
# current directory's name, and --out to a temporary directory.
set -e

if ! command -v swiftdoc >/dev/null 2>&1; then
    echo "swiftdoc is not on PATH. Install it first: https://github.com/kura120/swiftdoc#installation" >&2
    exit 1
fi

has_src=0
has_name=0
has_out=0
for arg in "$@"; do
    case "$arg" in
        -s | --src | --src=*) has_src=1 ;;
        -n | --name | --name=*) has_name=1 ;;
        -o | --out | --out=*) has_out=1 ;;
    esac
done

# With [tool.swiftdoc] in pyproject.toml, src and name come from there:
# defaults passed here would override them.
if [ -f pyproject.toml ] && grep -q '^\[tool\.swiftdoc\]' pyproject.toml; then
    has_src=1
    has_name=1
fi

if [ "$has_src" = 0 ]; then
    set -- "$@" --src .
fi
if [ "$has_name" = 0 ]; then
    set -- "$@" --name "$(basename "$PWD")"
fi
if [ "$has_out" = 0 ]; then
    out_dir=$(mktemp -d)
    trap 'rm -rf "$out_dir"' EXIT
    set -- "$@" --out "$out_dir"
fi

swiftdoc --strict "$@"
