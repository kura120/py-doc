# Changelog

## v0.6.1

The first release on PyPI: `pip install swiftdoc`. The v0.6.0 release run published the binaries but stopped before the PyPI upload, because the Linux arm64 wheel failed to build.

- **Macros start with `#sd-`**: `#sd-write`, `#sd-doc-link`, `#sd-note`, `#sd-warning`, `#sd-code`, `#sd-image` and `#sd-z-index`. Update existing docstrings, for example with `sed -i 's/#pd-/#sd-/g'` (#61).
- `--intersphinx` downloads inventories with `curl` (included in Linux, macOS and Windows 10 and later) instead of a bundled HTTP client. Every target builds again, and the binary is about 1.6 MB smaller (#61).

## v0.6.0

A large release. swiftdoc is much faster on big projects, reads its settings from `pyproject.toml`, documents more of your code, and can now write JSON and Markdown as well as HTML. It also publishes Markdown guides, links to and from Sphinx sites, and keeps docs for several releases side by side.

### Speed

- **47x faster on large projects.** v0.5.1 serialized the whole package once per page. On SQLAlchemy that took 16 s; it now takes about 0.24 s (#29).
- **Incremental builds.** Files that have not changed since the last run are not parsed again (#45).
- **Smaller pages.** The search index loads on the first search, not on every page (#43). The no-JS module list is written once, on the index page (#35).
- Release binaries are built with `opt-level = 3` (#33). They are a little larger.
- `benchmarks/` compares swiftdoc with pydoc, pdoc, pdoc3 and Sphinx. The README has the chart (#29, #51).

### Configuration

- **`[tool.swiftdoc]` in `pyproject.toml`.** Every flag can be set there. The project name and version come from `[project]` (#31). Use `--config` to point at another file.

### What gets documented

- **Public API only, by default.** Names in `__all__` are documented. Without `__all__`, names starting with `_` are left out. Pass `--include-private` for the old behavior (#34).
- Module variables, class attributes, dataclass fields and enum members (#39).
- Nested classes, and definitions inside `if`, `try`, `with` and `if TYPE_CHECKING:` blocks (#46).
- **Docstring sections.** Google, NumPy and reST sections (Args, Returns, Raises, ...) render as structured lists. `--docstring-style` picks a style or turns this off (#49).
- **Annotation links.** Type annotations and base classes link to their pages (#50).

### Output

- `--format json` writes the parsed API as `api.json` (#42).
- `--format markdown` writes Markdown pages for MkDocs, Docusaurus or LLM context (#44).
- `swiftdoc serve` builds the site, serves it on 127.0.0.1 and reloads the page when files change (#48).
- The generated site meets WCAG 2.2 AA. CI checks it with axe and a keyboard test (#47).

### Guides, cross-project links and versions

- **Markdown pages.** `--pages <DIR>` publishes `.md` files as guides above the API reference, with front-matter titles and ordering, links between pages, macros and search. A top-level `index.md` replaces the landing page's intro (#27).
- **Sphinx inventories.** Every HTML build writes `objects.inv`, so Sphinx projects can link to your API with intersphinx (#25).
- **Links to other projects.** `--intersphinx NAME=URL` links annotations and `#pd-doc-link` to other projects' Sphinx docs, such as `pathlib.Path` to the Python docs. Inventories are cached, so builds work offline (#25).
- **Several versions.** `--version-dir ROOT` builds each release into `ROOT/<version>/`, with a version switcher, `versions.json` and a redirect to the latest (#24).

### Themes and templates

- **Block inheritance.** A custom template can extend `swiftdoc/<name>.html` and override only the blocks it changes (#32).
- `--css` adds a stylesheet. The theme's colors and spacing are CSS custom properties you can override (#38).
- `swiftdoc theme eject` copies the bundled theme into your project as a starting point (#41).
- `docs/templates.md` documents every template variable. Templates get `context_version`, bumped on breaking changes (#11).

### CI and distribution

- `--coverage` prints a documentation coverage report. `--fail-under N` fails the run below N percent. `--coverage-format json` is for scripts (#40). Dunder methods (`__init__`, `__repr__`, ...) are not counted.
- A GitHub Action and a pre-commit hook (#36).
- Packaged for PyPI as `swiftdoc` (#37). The v0.6.0 upload did not run; v0.6.1 is the first release on PyPI.
- Release binaries now include Linux arm64.

### Documentation

- The README compares swiftdoc with pydoc, pdoc, pdoc3 and Sphinx. New guides in `docs/`: deploying to GitHub Pages, using swiftdoc in CI, and migrating from Sphinx or pdoc (#28).

### Upgrading from v0.5.1

- Private names are no longer documented by default. Add `--include-private` to keep them.
- Docstrings with Google, NumPy or reST sections render differently. Use `--docstring-style markdown` for the old output.
- The command is `swiftdoc`, and settings live in `[tool.swiftdoc]`. Custom templates extend `swiftdoc/<name>.html`.
- The command line has subcommands (`serve`, `theme eject`). Running with flags works as before.
- `--strict` and `--clean` also accept `=false`, so a config file setting can be turned off on the command line.
- Custom templates written for v0.5.1 keep rendering. A custom `sidebar.html` still renders the full no-JS module list on every page.
