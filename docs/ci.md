# Using swiftdoc in CI

A full build takes well under a second on most projects (a quarter of a second on SQLAlchemy's 254,000 lines; see [Benchmarks](../README.md#benchmarks)), and swiftdoc never imports your code, so CI needs neither your dependencies nor a Python environment. That makes these checks cheap enough to run on every push and pull request.

## Fail on broken docs

```bash
swiftdoc --src src/myproject --out site --strict
```

`--strict` turns problems into a failed build: a `#sd-doc-link` that does not resolve, a missing image, a file that does not parse, an unknown `[tool.swiftdoc]` key, or an `--intersphinx` inventory that cannot be loaded.

## Require documentation coverage

```bash
swiftdoc --src src/myproject --fail-under 80
```

Without `--out` this only checks: it prints the coverage report and writes nothing. `--coverage-format json` prints the report as JSON for other tools. What counts is described in [Documentation coverage](../README.md#documentation-coverage).

## GitHub Actions

The repository is a GitHub Action. The same checks, plus a build of the site:

```yaml
- uses: kura120/swiftdoc@master
  with:
    src: src/myproject
    args: --strict --fail-under 80
```

To publish the site as well, see [Deploying to GitHub Pages](github-pages.md).

## pre-commit

The [pre-commit hook](../README.md#pre-commit) runs the same build before each commit, for example with `args: [--fail-under=80]`.

## Faster rebuilds

CI usually starts from an empty folder, so every build is a full build. If you cache the output folder between runs, swiftdoc's [incremental build](../README.md#incremental-builds) only re-renders what changed.
