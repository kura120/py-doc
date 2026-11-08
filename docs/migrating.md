# Migrating from Sphinx or pdoc

swiftdoc reads source files instead of importing them, so a migration is mostly a matter of pointing it at your code. This guide maps the features you use today.

## From Sphinx with autodoc

| In Sphinx | In swiftdoc |
| --- | --- |
| `conf.py` | `[tool.swiftdoc]` in `pyproject.toml`; see [Configuring with pyproject.toml](../README.md#configuring-with-pyprojecttoml) |
| `automodule` / `autoclass` directives | Not needed: every public module, class, function and variable is documented. `__all__` decides what is public. |
| reST field lists (`:param x:`), Napoleon (Google / NumPy) | Read directly; `--docstring-style` picks one style if detection guesses wrong. See [Docstring sections](../README.md#docstring-sections). |
| Narrative `.rst` pages, `toctree` | Markdown pages with `--pages docs`; see [Markdown pages](../README.md#markdown-pages). reST pages need converting to Markdown, for example with `pandoc -f rst -t gfm`. |
| `:py:class:` cross-references | `#pd-doc-link: package.module.Class`, and type annotations link on their own |
| `intersphinx_mapping` | `--intersphinx python=https://docs.python.org/3`; see [Linking to other projects](../README.md#linking-to-other-projects) |
| Other projects linking to you through intersphinx | Keep working: every build writes `objects.inv`. See [Linking from Sphinx](../README.md#linking-from-sphinx). |
| Versions on Read the Docs | `--version-dir`; see [Several versions](../README.md#several-versions) and [Deploying to GitHub Pages](github-pages.md) |
| Themes, `html_static_path` | `--css` to restyle, `--templates` to change markup; see [Restyling with CSS](../README.md#restyling-with-css) and [Custom templates](../README.md#custom-templates) |

What swiftdoc does not do: PDF, ePub or man page output, Sphinx extensions, and anything that needs the live objects (see the limits below).

## From pdoc or pdoc3

| In pdoc / pdoc3 | In swiftdoc |
| --- | --- |
| `pdoc mypackage -o docs` | `swiftdoc --src src/mypackage --out docs` |
| Markdown docstrings | Rendered as Markdown, with Google, NumPy and reST sections on top |
| `__all__` and `_private` names | The same rules; `--include-private` shows everything |
| pdoc3's `__pdoc__` overrides | Not supported: edit the docstring, or use `__all__` to hide a name |
| Jinja2 (pdoc) or Mako (pdoc3) templates | Tera templates, close to Jinja2. Override single blocks; see [Custom templates](../README.md#custom-templates) and [the template context](templates.md). |
| `--docformat` | `--docstring-style` |
| pdoc3 `--pdf` (Markdown output) | `--format markdown`; see [Markdown output](../README.md#markdown-output) |

## The limits of parsing

swiftdoc sees what is written in the source, not what exists at runtime. It does not see:

* attributes and methods created at runtime (by metaclasses, decorators that add members, `setattr`)
* members inherited from classes in other packages
* docstrings assigned with `__doc__ = ...` or built by code
* signatures changed by decorators, such as `functools.wraps` wrappers

In exchange, it works without your dependencies installed (native or GPU libraries, heavy import side effects, machines without network access) and never runs your code, which matters for untrusted or security-sensitive code.
