# py-doc

A lightweight and fast documentation tool for Python codebases, powered by Rust.

py-doc reads your `.py` files, never imports or runs them, and writes a static site: one page per module, a searchable index, and three switchable layouts. The theme is built into the binary, so generation works offline.

<p align="center"><img src="https://raw.githubusercontent.com/kura120/py-doc/master/benchmarks/benchmark.svg" alt="Time to generate HTML API docs. requests: py-doc 20 ms, pydoc 305 ms, pdoc3 868 ms, pdoc 1.50 s, Sphinx 4.27 s. rich: py-doc 44 ms, pydoc 571 ms, pdoc3 1.95 s, pdoc 3.88 s, Sphinx 10.8 s. SQLAlchemy: py-doc 344 ms, pdoc3 15.5 s, pydoc 15.9 s, pdoc 39.7 s, Sphinx 62.6 s." width="820"></p>

py-doc documents all 253,000 lines of SQLAlchemy in about a third of a second: 45x faster than `pydoc` and `pdoc3`, 115x faster than `pdoc`, and 180x faster than Sphinx with autodoc. See [Benchmarks](#benchmarks) for how this was measured.

## Table of Contents

* [Installation](#installation)
* [Usage](#usage)
* [Writing docs](#writing-docs)
* [The generated site](#the-generated-site)
* [Restyling with CSS](#restyling-with-css)
* [Custom templates](#custom-templates)
* [Benchmarks](#benchmarks)
* [Development](#development)
* [Troubleshooting](#troubleshooting)

---

## Installation

Choose the installation method that fits your environment.

### Option A: pip, pipx or uv (Recommended)

From the first release that includes wheels, py-doc is on PyPI as `swiftdoc` (the name `py-doc` was taken). The wheels hold the compiled binary, so no Rust toolchain is needed. The command it installs is still `py-doc`.

```bash
pipx install swiftdoc      # or
uv tool install swiftdoc   # or
pip install swiftdoc
```

To pin it per project, add `swiftdoc` to your dev dependencies, for example `uv add --dev swiftdoc`. Wheels are built for Linux (x86_64 and aarch64, glibc and musl), macOS (Intel and Apple silicon) and Windows (x86_64).

### Option B: One-Line Script Installer

You do not need Cargo or Rust installed for this. The script downloads the pre-compiled binary for your platform, verifies its SHA-256 checksum when the release publishes one, and installs it.

* **macOS / Linux (Bash/Zsh):** installs to `/usr/local/bin`
```bash
curl -fsSL https://raw.githubusercontent.com/kura120/py-doc/master/scripts/install.sh | sh
```

* **Windows (PowerShell):** installs to `%USERPROFILE%\.cargo\bin`
```powershell
irm https://raw.githubusercontent.com/kura120/py-doc/master/scripts/install.ps1 | iex
```

### Option C: Fast Binary Installer (cargo-binstall)

If you have Cargo but want to avoid compile times, install the pre-compiled binary with `cargo-binstall`. py-doc is not published on crates.io, so point it at the repository:

```bash
# Install cargo-binstall if you don't have it
cargo install cargo-binstall

cargo binstall --git https://github.com/kura120/py-doc py-doc
```

### Option D: Build from Source

```bash
git clone https://github.com/kura120/py-doc.git
cd py-doc
cargo install --path .
```

---

## Usage

Run `py-doc` against a Python project directory or a single `.py` file:

```bash
py-doc --src path/to/project --out ./docs --name project-name
```

Settings can also live in `pyproject.toml`; see [Configuring with pyproject.toml](#configuring-with-pyprojecttoml). In a configured project, `py-doc` alone builds the site.

Run `py-doc` with no arguments in a terminal, in a project without a `[tool.py-doc]` table, to use the interactive setup wizard instead. It is prefilled from `pyproject.toml` when there is one. In the wizard, Tab moves between fields, the arrow keys browse folders in path fields, and Enter on the last row generates the site.

### Options

| Flag | Meaning |
| --- | --- |
| `-s`, `--src <PATH>` | Project directory, or a single `.py` file. Required unless set in `pyproject.toml`. |
| `-o`, `--out <DIR>` | Where the site is written. Required unless set in `pyproject.toml`. |
| `-n`, `--name <NAME>` | Package name shown in the docs. Defaults to `[project] name`; required if there is none. |
| `--doc-version <VERSION>` | Your project's version, shown in the sidebar. Defaults to `[project] version`, else `1.0.0`. |
| `--layout <classic\|minimal\|modern>` | Layout the site opens with. Default `classic`. |
| `--theme <auto\|dark\|light\|slate>` | Color theme the site opens with. `auto` (default) follows the reader's system setting. |
| `-e`, `--exclude <NAME>` | Skip a file or directory, by name or by path relative to `--src`. Repeatable. |
| `--source-url <URL>` | Base URL of a browsable copy of the sources. Turns each symbol's `file:line` reference into a link. |
| `--css <FILE>` | Extra stylesheet, linked after the bundled one on every page. Repeatable. See [Restyling with CSS](#restyling-with-css). |
| `-t`, `--templates <DIR>` | Directory with replacement theme files. See [Custom templates](#custom-templates). |
| `--strict` | Exit with an error if a file fails to parse, or a macro, image or doc-link cannot be resolved. Useful in CI. |
| `--clean` | Remove files that an earlier run wrote to `--out` and this run no longer produces, such as pages for deleted modules. |
| `--include-private` | Also document private names and modules. See [Public API](#public-api). |
| `--coverage` | Print a documentation coverage report. Without `--out` on the command line, no site is written. See [Documentation coverage](#documentation-coverage). |
| `--fail-under <PERCENT>` | Exit with an error when coverage is below `PERCENT` (0 to 100). Turns on `--coverage`. |
| `--coverage-format <text\|json>` | Format of the coverage report. Default `text`. |
| `--config <FILE>` | The `pyproject.toml` to read. By default it is found automatically (see below). |
| `-V`, `--version` | Print the py-doc version. |
| `-h`, `--help` | Print help. |

> **Changed in this release:** `-v` / `--version <VERSION>` used to set the version shown in the docs. That is now `--doc-version`, and `--version` reports the tool's own version.

`--strict`, `--clean`, `--include-private` and `--coverage` also take `=true` or `=false`, so a command line can turn off what `pyproject.toml` turns on.

### Configuring with pyproject.toml

Put any option in a `[tool.py-doc]` table, using the flag's name as the key:

```toml
[project]
name = "myproject"
version = "2.1.0"

[tool.py-doc]
src = "src/myproject"
out = "docs"
layout = "modern"
theme = "dark"
exclude = ["tests", "_vendor"]
source-url = "https://github.com/me/myproject/blob/main"
strict = true
```

Then run `py-doc` with no arguments. The name and version come from `[project]`; set `name` or `doc-version` in `[tool.py-doc]` to show something else. If the version is `dynamic`, py-doc shows `1.0.0` and prints a hint; pass `--doc-version` to set it.

Each setting is taken from the first place that has it:

1. the command-line flag;
2. `[tool.py-doc]`;
3. `[project]` (for `name` and `doc-version` only);
4. the built-in default.

A flag replaces the config value, it does not add to it: `--exclude` or `--css` on the command line replaces the whole `exclude` or `css` list.

Paths in `pyproject.toml` (`src`, `out`, `templates`, `css`) are relative to the directory that holds it. Paths on the command line are relative to the current directory.

py-doc looks for `pyproject.toml` in `--src` (or the current directory, without `--src`) and then in each parent directory, stopping at the repository root (the first directory with a `.git`). `--config <FILE>` names the file instead.

An unknown key in `[tool.py-doc]` prints a warning naming it. With `strict`, it is an error.

### What is scanned

Every `.py` file under `--src` is documented, except:

* `__main__.py` files;
* hidden directories (names starting with `.`, such as `.venv` and `.git`);
* `__pycache__`, `venv`, `node_modules`, `site-packages`, `build`, `dist`, `htmlcov`, `__pypackages__` and `*.egg-info` directories;
* anything named with `--exclude`.

Files with syntax errors are reported and skipped; the rest of the site is still generated.

### Public API

By default only the public API is documented, the way pdoc does it:

* If a module defines `__all__`, only the names listed there are shown. A listed `_name` is shown too. `__all__` must be a literal list or tuple of strings; `+=`, `.extend(...)` and `.append(...)` with literals are followed. If it is built any other way, it is ignored.
* Otherwise, names starting with `_` are hidden. Dunder methods such as `__init__` and `__call__` are always shown.
* A module whose name starts with `_` is hidden, and so is everything inside a package whose name starts with `_`.

Hidden items get no page and do not appear in the navigation or search. A `#pd-doc-link` to one is reported as a warning instead of being linked. Pass `--include-private` to document everything.

### Documentation coverage

`--coverage` counts how much of the public API has a docstring:

```console
$ py-doc --src src/myproject --coverage
Module               Documented    Cover
----------------------------------------
myproject.cli               1/6    16.7%
myproject.core             9/12    75.0%
myproject                   1/1   100.0%
----------------------------------------
Total                     11/19    57.9%
```

Modules, classes, functions and methods are counted. "Public" follows the [Public API](#public-api) rules, so with `--include-private` everything counts. An item is documented when its docstring is not empty after whitespace is stripped. Doc-only `#pd-write` pages are not counted, and files that fail to parse are left out (they are reported as usual). Rows are sorted worst first.

`--fail-under 80` turns the report on and exits with an error when the total is below 80%:

```text
Error: documentation coverage is 57.89%, below --fail-under 80%
```

**When is the site written?** `--coverage` or `--fail-under` on the command line without `--out` there only checks: nothing is written, even when `pyproject.toml` sets `out`. Add `--out` on the command line to build the site too. Set in `[tool.py-doc]` instead (`coverage = true`, `fail-under = 80`, `coverage-format = "json"`), coverage is added to every normal build.

In CI, the check needs no output folder:

```yaml
- uses: kura120/py-doc@master
  with:
    args: --fail-under 80
```

The action always passes `--out`, so this also builds the site. To only check, run `py-doc --src src/myproject --fail-under 80` in a step instead. The [pre-commit](#pre-commit) hook takes the same flags, for example `args: [--fail-under=80]`.

`--coverage-format json` prints only JSON on stdout (progress goes to stderr), so it can be piped or saved. Shortened to one module:

```json
{
  "schema_version": 1,
  "include_private": false,
  "fail_under": 80.0,
  "passed": false,
  "totals": { "documented": 11, "total": 19, "percent": 57.89 },
  "kinds": {
    "module": { "documented": 3, "total": 3 },
    "class": { "documented": 2, "total": 3 },
    "function": { "documented": 4, "total": 8 },
    "method": { "documented": 2, "total": 5 }
  },
  "modules": [
    {
      "module": "myproject.cli",
      "path": "myproject/cli.py",
      "documented": 1,
      "total": 6,
      "percent": 16.67,
      "undocumented": ["myproject.cli.main", "myproject.cli.Parser"]
    }
  ],
  "undocumented": ["myproject.cli.main", "myproject.cli.Parser", "..."]
}
```

`modules` is sorted by module name. `module` is `""` for a root `__init__.py`, which has no dotted name; its `path` and its entry in `undocumented` use the file name. `undocumented` lists qualified names in source order. Percentages are rounded to two decimals, and `percent` is 100 when there is nothing to count. `schema_version` changes only when a field is renamed or removed.

### Keeping the output folder tidy

Each run records the files it wrote in `.py-doc-manifest` inside `--out`. `--clean` deletes files listed there that the current run did not produce, and removes folders left empty. Anything py-doc did not write, such as a `CNAME` file or hand-written pages, is never touched. An output folder from before this feature has no manifest, so its old files are not removed.

### Linking to source

```bash
py-doc --src . --out docs --name myproject \
  --source-url https://github.com/me/myproject/blob/main
```

### GitHub Actions

The repository is also a GitHub Action. It downloads the release binary for the runner (Linux x86_64 or arm64, macOS Intel or Apple silicon, Windows), checks its SHA-256 checksum, and runs py-doc. From v0.5.1 on, a missing or wrong checksum fails the step. A full build takes well under a second, so it can run on every push.

This workflow builds the docs and publishes them to GitHub Pages. In the repository settings, set Pages to deploy from GitHub Actions.

```yaml
name: Docs

on:
  push:
    branches: [main]

permissions:
  contents: read
  pages: write
  id-token: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: kura120/py-doc@master
        with:
          src: src/myproject
          out: site
          name: myproject
          args: --strict --source-url https://github.com/${{ github.repository }}/blob/${{ github.sha }}
          upload-pages-artifact: true

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

| Input | Meaning |
| --- | --- |
| `version` | Release to install, such as `0.5.1`. Default `latest`. |
| `install` | `release` (default) downloads a binary. `source` builds the action's own checkout with cargo and needs a Rust toolchain on the runner. Releases up to v0.5.1 have no Linux arm64 binary, so use `source` there for those versions. |
| `src` | `--src`. Default `.`, or `src` from `[tool.py-doc]` when `pyproject.toml` configures py-doc. |
| `out` | `--out`. Default `site`. |
| `name` | `--name`. Defaults to the repository name, or the name from `pyproject.toml` when it configures py-doc. |
| `args` | Any other flags, passed through. Quotes group words. |
| `upload-pages-artifact` | `true` uploads `out` with `actions/upload-pages-artifact`. Default `false`. |

The action's outputs are `version` (the py-doc version installed) and `out`. Once a release includes the action and the hook, pin its tag instead of `master` here and in the pre-commit config below.

### pre-commit

The hook runs `py-doc --strict` into a temporary directory, so a file that fails to parse, or a doc-link, macro or image that cannot be resolved, fails the commit. It does not install py-doc: install it first (see [Installation](#installation)) so `py-doc` is on `PATH`.

```yaml
repos:
  - repo: https://github.com/kura120/py-doc
    rev: master
    hooks:
      - id: py-doc
        args: [--src, src/myproject, --name, myproject]
```

`--src` defaults to `.` and `--name` to the folder name, unless `pyproject.toml` has a `[tool.py-doc]` table, in which case both come from it. Any other args are passed to py-doc.

---

## Writing docs

Docstrings are rendered as Markdown, including tables, strikethrough and task lists. Python code blocks are syntax-highlighted at build time.

### Variables and attributes

Module variables, type aliases (`type X = ...`) and class attributes are documented with their annotation and value. Values longer than 80 characters are cut short and end in `…`. Enum members get an `enum` badge, and fields of a `@dataclass` (or attrs class) get a `field` badge.

Document a variable with a string right after it, or with `#:` comments above it or at the end of its line:

```python
TIMEOUT: float = 2.5
"""Seconds to wait for a reply."""

#: How often a request is retried.
MAX_RETRIES = 3

DEFAULT_NAME = "anonymous"  #: Used when no name is given.
```

Class attributes starting with `_` are private, dunders included. `__slots__`, `__match_args__`, `__annotations__`, `__doc__`, `__module__` and `__all__` are never documented.

### Nested and conditional definitions

Classes defined inside a class are shown inside its card, under qualified names such as `Outer.Inner`. Link to them the same way: `#pd-doc-link: engine.Outer.Inner`.

Definitions inside `if`, `try` and `with` blocks are documented too, at module and class level. Function bodies and `if __name__ == "__main__":` blocks are not.

When a name is defined in several branches, such as `if sys.version_info >= (3, 11):` and `else:`, or `try:` and `except ImportError:`, only the first definition is shown. Definitions in a branch get a `conditional` badge. Definitions that only exist under `if TYPE_CHECKING:` get a `type-checking` badge, and a runtime definition of the same name always wins over them. Several definitions in the same block, such as `@overload` signatures or a property and its setter, are all kept.

### Macros

Put a macro on its own line inside any docstring.

| Macro | Effect |
| --- | --- |
| `#pd-note: text` | A highlighted note box. The text may use inline Markdown. |
| `#pd-warning: text` | A highlighted warning box. The text may use inline Markdown. |
| `#pd-doc-link: target` | A "See Reference" link to a module, class, function, method, variable or attribute. |
| `#pd-image: path` | An image. The path is relative to `--src` and must stay inside it. |
| `#pd-code` | Starts a Python code block that ends at the next line containing only ```` ``` ````. For another language, name it: `#pd-code json`. |
| `#pd-write` | As the first line of a module docstring, in a module with no classes or functions: renders the module as a plain document page. |
| `#pd-z-index: N` | In a module docstring: sort position in the navigation. Lower numbers come first. |

`#pd-doc-link` targets are dotted paths. All of these work, as long as they identify exactly one thing:

```text
#pd-doc-link: core.engine                 a module
#pd-doc-link: core.engine.Engine          a class
#pd-doc-link: core.engine.Engine.run      a method
#pd-doc-link: helpers.slugify             a unique trailing path is enough
#pd-doc-link: Engine                      a bare name, if only one module defines it
```

A target that cannot be found is shown as "(not found)" and reported as a warning. So are a misspelled macro, a missing image and a `#pd-code` block that is never closed. Macro-looking lines inside an ordinary fenced code block are left as text.

### A documentation-only page

```python
"""#pd-write
#pd-z-index: 1
# Getting Started

Welcome to the project.

#pd-note: Requires Python 3.10 or newer.
#pd-doc-link: core.engine.Engine
"""
```

### Ordering folders

Prefix a folder name with digits and an underscore to control its position. The prefix sets the order and is not shown: `01_getting_started/` appears first, as "getting_started". Folders without a prefix follow, largest first.

A folder's `__init__.py` docstring becomes the folder's description, and the root `__init__.py` docstring is shown on the landing page.

---

## The generated site

* **Layouts:** Classic, Minimal and Modern, switchable from the sidebar. Each has a light and a dark palette; Classic also has Slate.
* **Theme:** follows the reader's system setting by default. The button at the top of the sidebar flips between light and dark, and the choice is remembered.
* **Search:** press `/` or `Ctrl+K`, type, move with the arrow keys, open with Enter, close with Esc. Search covers names and the first line of each docstring.
* **Module pages:** signatures with defaults and annotations as written, class bases, `property` / `static` / `classmethod` / `abstract` badges, a `file:line` reference per symbol, and an "On this page" outline on wide screens.

The output is plain static files. Open `index.html` directly or host the folder anywhere.

---

## Restyling with CSS

To change colors or fonts, keep the bundled theme and add a stylesheet with `--css`:

```css
/* brand.css */
:root {
    --accent-blue: #2563eb;
    --font-body: "IBM Plex Sans", sans-serif;
}
```

```bash
py-doc --src ./myproject --out ./docs --css brand.css
```

In `pyproject.toml`: `css = ["docs/brand.css"]`.

Each file is copied to `custom/` in the output and linked after `style.css` on every page, in the order given, so it can also add or override rules. A missing file is an error. `--clean` removes copies that a later run no longer writes.

A plain `:root` rule like the one above applies to every layout and color theme; `!important` is not needed. To set a value for one theme or layout only, use its attribute: `:root[data-theme="light"]`, `:root[data-layout="modern"]`, or both.

The bundled theme reads these properties. The colors are set per layout and per color theme; the rest are set per layout.

| Property | Controls |
| --- | --- |
| `--bg-color` | Page background, and the search box, search results and selects |
| `--sidebar-bg` | Sidebar background |
| `--text-color` | Body text |
| `--muted-color` | Secondary text and icons: labels, badges, line references |
| `--code-bg` | Code blocks, signature boxes, table headers, quotes, callouts |
| `--accent-blue` | Main accent: links, entity names, the active page, focus rings |
| `--accent-red` | Keywords in signatures and code, and error callouts |
| `--border-color` | Borders and dividers |
| `--card-border` | Module card borders (Modern layout) |
| `--sidebar-hover` | Hover and active background in the sidebar and search results |
| `--scrollbar-thumb`, `--scrollbar-thumb-hover` | Scrollbar color |
| `--font-body` | Body text and form controls |
| `--font-heading` | Headings, the sidebar title and labels |
| `--font-mono` | Code and signatures |
| `--radius-sm`, `--radius-md` | Corner rounding of small controls, and of cards and the search panel |
| `--shadow-card` | Shadow under module cards (Modern) and images |
| `--card-lift` | How far a module card moves up on hover (Modern) |
| `--content-max-width` | Maximum width of the main column |
| `--show-nav-icons` | `display` of the sidebar's file and folder icons; `none` hides them |
| `--nav-icon-color`, `--nav-icon-size`, `--nav-icon-stroke`, `--nav-icon-opacity` | Sidebar icon color, size, line width and opacity |

Custom templates get the copied files' paths as `extra_css`. The bundled `base.html` links them in its `stylesheets` block, so a template that replaces that block, or a whole page, has to link them itself.

---

## Custom templates

`--templates <DIR>` replaces parts of the bundled theme. Any of these files found in the directory is used instead of the built-in one; the rest fall back to the bundled theme:

```text
base.html  index.html  module.html  document.html  sidebar.html  style.css  app.js
```

The HTML files are [Tera](https://keats.github.io/tera/) templates. The bundled versions in [`assets/`](assets) are the reference for the variables available.

### Overriding one block

You do not have to copy a whole page to change part of it. The bundled templates are also available under the `py-doc/` prefix, so a custom template can extend the one it replaces and override only the blocks it changes. Everything else keeps the bundled default.

This `module.html` gives every module page a new header and leaves the rest of the page alone:

```html
{% extends "py-doc/module.html" %}

{% block header %}
<h1 class="module-title">{{ display_module_name }} <small>{{ package.name }} {{ version }}</small></h1>
{% endblock header %}
```

Use `{{ super() }}` inside a block to keep the bundled content and add to it:

```html
{% block scripts %}{{ super() }}<script src="{{ root_path }}extra.js" defer></script>{% endblock scripts %}
```

`index.html`, `module.html` and `document.html` all extend `base.html`, which holds the page skeleton. Its blocks are:

| Block | Holds |
| --- | --- |
| `head` | Everything inside `<head>`, including the blocks below it |
| `title` | The text of `<title>` |
| `description` | The `content` of the description `<meta>` tag |
| `stylesheets` | The `style.css` link |
| `body_attributes` | Extra attributes on `<body>` |
| `sidebar` | The `sidebar.html` include and the resize handle |
| `header` | The top of the page: breadcrumb and title |
| `content` | The page body |
| `outline` | The "On this page" panel (module pages only) |
| `footer` | Empty by default; shown after the page body |
| `scripts` | The script tags at the end of `<body>` |

`module.html` also splits `content` into `module_docstring`, `classes` and `functions`.

A custom `base.html` that extends `py-doc/base.html` changes every page at once, for example to add a footer to the whole site.

Any other `*.html` file in the directory, including ones in subfolders, can be included or extended by name: `{% include "partials/footer.html" %}`. Whole-file replacements still work: a template that does not use `extends` is rendered as it is.

---

## Benchmarks

Each tool generated HTML API docs for three packages from PyPI. Every run started from an empty output folder, and the figure is the median of 3 to 20 runs measured with [hyperfine](https://github.com/sharkdp/hyperfine) on a 4-core Linux machine.

| Package | Size | py-doc | pydoc | pdoc3 | pdoc | Sphinx + autodoc |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| requests 2.34.2 | 19 files, 6.4k lines | **20 ms** | 305 ms | 868 ms | 1.50 s | 4.27 s |
| rich 15.0.0 | 100 files, 38.5k lines | **44 ms** | 571 ms | 1.95 s | 3.88 s | 10.8 s |
| SQLAlchemy 2.1.6 | 258 files, 254k lines | **344 ms** | 15.9 s | 15.5 s | 39.7 s | 62.6 s |
| SQLAlchemy, peak memory | | **94 MB** | 146 MB | 133 MB | 1,077 MB | 523 MB |

**Why the gap is so large.** pydoc, pdoc, pdoc3 and Sphinx's autodoc all *import* your package and inspect the live objects, so they pay for Python startup, every import side effect and runtime introspection. py-doc parses the source with Ruff's Rust parser instead and renders pages in parallel on every core.

**What that trade-off costs.** Import-based tools see things a parser cannot, such as attributes created at runtime, members inherited from other packages, and docstrings assigned with `__doc__ = ...`. Because py-doc never imports, it also works without your dependencies installed and never runs your code.

**Fairness.** Every tool used the same Python and the same package versions. Sphinx ran with `-j auto`, so it used all cores the same way py-doc does. Import-based tools were given only modules that import cleanly on the test machine; py-doc parsed every file, so it did slightly more work.

To reproduce, run `cargo build --release && python3 benchmarks/bench.py`, then `python3 benchmarks/make_chart.py` to redraw the chart. Raw numbers are in [`benchmarks/results.json`](benchmarks/results.json).

---

## Development

```bash
cargo test                                   # unit tests and end-to-end tests against tests/fixtures
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Release builds are made with `scripts/build.sh [TARGET]` or `scripts/build.ps1 [-Target TARGET]`. Pushing a `v*` tag that matches the version in `Cargo.toml` builds and publishes binaries with SHA-256 checksums, and publishes the `swiftdoc` wheels and sdist to PyPI through trusted publishing. Each wheel is installed and run on its own platform first. The Packaging workflow builds and tests the Linux wheel on pull requests that touch packaging files.

UPX compression is available with `--upx` / `-Upx` for local builds. Published binaries are not packed: the download is already compressed, and packed executables are unsupported on macOS and often flagged by antivirus tools on Windows.

---

## Troubleshooting

### 'py-doc' is not recognized as a command

If you chose Option D (or your script path environment variable is missing), your shell cannot locate the executable. You must add Cargo's binary directory to your PATH.

* **Windows (PowerShell):**
```powershell
[Environment]::SetEnvironmentVariable("Path", "$env:Path;$env:USERPROFILE\.cargo\bin", "User")
```

* **macOS / Linux:**
```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### Dependency Conflicts during Local Compilation

If you see a compiler error stating `the trait bound 'CompactString: GetSize' is not satisfied` during source installations, run:

```powershell
cargo clean
cargo install --path .
```

This forces Cargo to respect the exact version constraint specified in the manifest.
