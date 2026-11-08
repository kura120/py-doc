# Changelog

## Unreleased

- `--pages <DIR>` publishes Markdown guides next to the API reference, with front-matter titles and ordering, links between pages, macros and search. A top-level `index.md` replaces the landing page's intro (#27).
- `docs/templates.md` documents every template variable, and templates get `context_version` (#11).
- HTML builds write `objects.inv`, so Sphinx projects can link to the API with intersphinx (#25).
- `--intersphinx NAME=URL` links annotations and `#pd-doc-link` to other projects' Sphinx docs, with an offline cache (#25).
- `--version-dir ROOT` builds each release into `ROOT/<version>/` with a version switcher, `versions.json` and a redirect to the latest (#24).
- README: a "Why swiftdoc" comparison, and guides for GitHub Pages, CI and migrating from Sphinx or pdoc in `docs/` (#28).

## v0.6.0

A large release. swiftdoc is much faster on big projects, reads its settings from `pyproject.toml`, documents more of your code, and can now write JSON and Markdown as well as HTML.

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

### Themes and templates

- **Block inheritance.** A custom template can extend `swiftdoc/<name>.html` and override only the blocks it changes (#32).
- `--css` adds a stylesheet. The theme's colors and spacing are CSS custom properties you can override (#38).
- `swiftdoc theme eject` copies the bundled theme into your project as a starting point (#41).

### CI and distribution

- `--coverage` prints a documentation coverage report. `--fail-under N` fails the run below N percent. `--coverage-format json` is for scripts (#40). Dunder methods (`__init__`, `__repr__`, ...) are not counted.
- A GitHub Action and a pre-commit hook (#36).
- Published to PyPI: `pip install swiftdoc` (#37).
- Release binaries now include Linux arm64.

### Upgrading from v0.5.1

- Private names are no longer documented by default. Add `--include-private` to keep them.
- Docstrings with Google, NumPy or reST sections render differently. Use `--docstring-style markdown` for the old output.
- The command is `swiftdoc`, and settings live in `[tool.swiftdoc]`. Custom templates extend `swiftdoc/<name>.html`.
- The command line has subcommands (`serve`, `theme eject`). Running with flags works as before.
- `--strict` and `--clean` also accept `=false`, so a config file setting can be turned off on the command line.
- Custom templates written for v0.5.1 keep rendering. A custom `sidebar.html` still renders the full no-JS module list on every page.
