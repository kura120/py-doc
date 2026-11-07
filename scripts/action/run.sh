#!/usr/bin/env bash
# Runs swiftdoc for the GitHub Action in action.yml. Not meant to be run by
# hand. Settings come from the environment:
#   SWIFTDOC_SRC, SWIFTDOC_OUT, SWIFTDOC_NAME  --src, --out and --name
#   SWIFTDOC_ARGS  extra flags, split like a shell command line (quotes are
#                honored, nothing is expanded or executed)
set -eo pipefail

# A repository that configures swiftdoc in pyproject.toml supplies its own
# src and name; passing defaults here would override them, because flags
# take precedence over [tool.swiftdoc].
configured=0
if [ -f pyproject.toml ] && grep -q '^\[tool\.swiftdoc\]' pyproject.toml; then
    configured=1
fi

src="$SWIFTDOC_SRC"
name="$SWIFTDOC_NAME"
if [ "$configured" = 0 ]; then
    src="${src:-.}"
    name="${name:-${GITHUB_REPOSITORY#*/}}"
fi

args=(--out "$SWIFTDOC_OUT")
if [ -n "$src" ]; then
    args+=(--src "$src")
fi
if [ -n "$name" ]; then
    args+=(--name "$name")
fi
if [ -n "${SWIFTDOC_ARGS//[[:space:]]/}" ]; then
    # xargs splits on whitespace and honors quotes without running anything.
    while IFS= read -r -d '' arg; do
        args+=("$arg")
    done < <(printf '%s' "$SWIFTDOC_ARGS" | xargs printf '%s\0')
fi

echo "swiftdoc ${args[*]}"
swiftdoc "${args[@]}"
echo "out=$SWIFTDOC_OUT" >>"$GITHUB_OUTPUT"
