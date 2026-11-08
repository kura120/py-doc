# Deploying to GitHub Pages

The [README](../README.md#github-actions) has the basic workflow: the swiftdoc action builds the site and `actions/deploy-pages` publishes it on every push. This guide adds versioned docs.

## One version (the latest)

Use the README workflow as is. Each push replaces the site. `--strict` fails the build on a broken `#sd-doc-link` or a missing image, so a broken page never ships.

## Every release, with a version switcher

`--version-dir` needs the earlier versions next to the new one, so keep the site on a `gh-pages` branch instead of rebuilding it from scratch. In the repository settings, set Pages to deploy from the `gh-pages` branch.

```yaml
name: Docs

on:
  release:
    types: [published]

permissions:
  contents: write

jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      # The published site, with every earlier version.
      - uses: actions/checkout@v4
        with:
          ref: gh-pages
          path: site
        continue-on-error: true # the first release has no gh-pages branch yet
      - uses: kura120/swiftdoc@master
        with:
          src: src/myproject
          out: _unused
          args: --version-dir site --doc-version ${{ github.event.release.tag_name }} --strict
      - name: Publish
        run: |
          cd site
          git init -q -b gh-pages 2>/dev/null || true
          touch .nojekyll # Jekyll would drop files starting with _, such as __init__.html
          git add -A
          git -c user.name=docs -c user.email=docs@users.noreply.github.com commit -qm "Docs for ${{ github.event.release.tag_name }}"
          git push -f "https://x-access-token:${{ github.token }}@github.com/${{ github.repository }}" HEAD:gh-pages
```

Each release adds a folder, `site/versions.json` lists them all, and `site/index.html` redirects to the newest. Older versions pick up the new one in their switcher without being rebuilt, because the switcher reads `versions.json` when a page loads.

The action always passes `--out`; with `--version-dir` the site goes to `site/<version>/` instead, so `_unused` stays empty.

Every link is relative, so the site also works from a project page (`https://you.github.io/project/`) and on any other static host.
