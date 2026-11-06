#!/usr/bin/env bash
# Runs py-doc for the GitHub Action in action.yml. Not meant to be run by
# hand. Settings come from the environment:
#   PY_DOC_SRC, PY_DOC_OUT, PY_DOC_NAME  --src, --out and --name
#   PY_DOC_ARGS  extra flags, split like a shell command line (quotes are
#                honored, nothing is expanded or executed)
set -eo pipefail

# A repository that configures py-doc in pyproject.toml supplies its own
# src and name; passing defaults here would override them, because flags
# take precedence over [tool.py-doc].
configured=0
if [ -f pyproject.toml ] && grep -q '^\[tool\.py-doc\]' pyproject.toml; then
    configured=1
fi

src="$PY_DOC_SRC"
name="$PY_DOC_NAME"
if [ "$configured" = 0 ]; then
    src="${src:-.}"
    name="${name:-${GITHUB_REPOSITORY#*/}}"
fi

args=(--out "$PY_DOC_OUT")
if [ -n "$src" ]; then
    args+=(--src "$src")
fi
if [ -n "$name" ]; then
    args+=(--name "$name")
fi
if [ -n "${PY_DOC_ARGS//[[:space:]]/}" ]; then
    # xargs splits on whitespace and honors quotes without running anything.
    while IFS= read -r -d '' arg; do
        args+=("$arg")
    done < <(printf '%s' "$PY_DOC_ARGS" | xargs printf '%s\0')
fi

echo "py-doc ${args[*]}"
py-doc "${args[@]}"
echo "out=$PY_DOC_OUT" >>"$GITHUB_OUTPUT"
