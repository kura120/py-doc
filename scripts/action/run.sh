#!/usr/bin/env bash
# Runs py-doc for the GitHub Action in action.yml. Not meant to be run by
# hand. Settings come from the environment:
#   PY_DOC_SRC, PY_DOC_OUT, PY_DOC_NAME  --src, --out and --name
#   PY_DOC_ARGS  extra flags, split like a shell command line (quotes are
#                honored, nothing is expanded or executed)
set -eo pipefail

name="$PY_DOC_NAME"
if [ -z "$name" ]; then
    name="${GITHUB_REPOSITORY#*/}"
fi

args=(--src "$PY_DOC_SRC" --out "$PY_DOC_OUT" --name "$name")
if [ -n "${PY_DOC_ARGS//[[:space:]]/}" ]; then
    # xargs splits on whitespace and honors quotes without running anything.
    while IFS= read -r -d '' arg; do
        args+=("$arg")
    done < <(printf '%s' "$PY_DOC_ARGS" | xargs printf '%s\0')
fi

echo "py-doc ${args[*]}"
py-doc "${args[@]}"
echo "out=$PY_DOC_OUT" >>"$GITHUB_OUTPUT"
