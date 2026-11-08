# Template context

Every variable a swiftdoc template can use. Custom templates (`--templates`) and ejected themes (`swiftdoc theme eject`) get exactly this context.

`context_version` is bumped whenever a variable listed here is renamed, removed or changes type. New variables can be added without a bump. Check it in a custom theme to fail loudly instead of rendering a broken page:

```jinja
{% if context_version != 1 %}{{ throw(message="This theme needs template context version 1") }}{% endif %}
```

Optional values are `none` when not set; test them with `{% if x %}`. HTML values are already escaped and safe to print with `| safe`; plain-text values are escaped by Tera's autoescape.

Tests check that each object below lists exactly the fields the code has, and that every variable exists (`template_context_matches_the_docs` in `src/generator.rs`, `custom_templates_see_every_page_variable` in `tests/theme.rs`).

## Every page

### Context

| Variable | Type | Description |
|---|---|---|
| `context_version` | integer | Version of this contract. Currently `1`. |
| `package` | [Package](#package) | The documented package: every page sees every module. |
| `nav_groups` | list of [NavGroup](#navgroup) | Modules grouped by folder, in sidebar order. |
| `version` | string | The project version (`--doc-version`). |
| `version_switcher` | boolean | Built with `--version-dir`: show a version `<select>` (`app.js` fills it from `{{ root_path }}../versions.json`). |
| `root_path` | string | Relative path from this page to the site root: `"./"` or `"../../"`. Prefix every site link with it. |
| `layout` | string | The layout the site opens with: `"classic"`, `"minimal"` or `"modern"`. |
| `theme` | string, optional | The color theme the site opens with: `"dark"`, `"light"` or `"slate"`. `none` for `auto`. |
| `source_url` | string, optional | `--source-url`, without a trailing slash. Link a symbol with `{{ source_url }}/{{ module.source_path }}#L{{ item.line }}`. |
| `extra_css` | list of strings | Paths of the `--css` stylesheets, relative to the site root. |

## index.html only

| Variable | Type | Description |
|---|---|---|
| `total_modules` | integer | Number of documented modules. |
| `total_classes` | integer | Number of documented classes. |
| `total_functions` | integer | Number of documented functions and methods. |
| `package_docstring` | HTML, optional | The root `__init__.py` docstring. |
| `package_link_path` | string, optional | Page of the root `__init__.py`, when it defines anything. |

## module.html and document.html only

`document.html` renders `#pd-write` pages; `module.html` renders everything else.

| Variable | Type | Description |
|---|---|---|
| `module` | [Module](#module) | The module this page is for. |
| `display_module_name` | string | The page title: the module name, or for an `__init__.py` its folder (the package name at the root). |

## Objects

### Package

| Field | Type | Description |
|---|---|---|
| `name` | string | The project name (`--name`). |
| `modules` | list of [Module](#module) | Every documented module, in sidebar order. |

### NavGroup

| Field | Type | Description |
|---|---|---|
| `name` | string, optional | Display path of the folder (`"core/sub"`); `none` for modules at the root. |
| `link_path` | string, optional | Page of the folder's own `__init__.py`, if it has one. |
| `docstring` | HTML, optional | The folder's `__init__.py` docstring. |
| `classes` | list of [Class](#class) | Classes defined in the folder's `__init__.py`. |
| `functions` | list of [Function](#function) | Functions defined in the folder's `__init__.py`. |
| `modules` | list of [Module](#module) | The folder's other modules. |
| `pages` | boolean | The group of `--pages` Markdown pages (named "Guides"), always first. |

### Module

| Field | Type | Description |
|---|---|---|
| `name` | string | File name without `.py` (`"engine"`, `"__init__"`). |
| `qualname` | string | Dotted path, ordering prefixes stripped (`"core.engine"`). |
| `source_path` | string | Path from the source root, with forward slashes (`"01_core/engine.py"`). |
| `link_path` | string | Path of the module's page from the site root (`"01_core/engine.html"`). |
| `folder` | string, optional | Display path of the containing folder (`"core/sub"`). |
| `docstring` | HTML, optional | The module docstring. |
| `summary` | string, optional | First line of the docstring, as plain text. |
| `classes` | list of [Class](#class) | Classes, in source order. |
| `functions` | list of [Function](#function) | Functions, in source order. |
| `variables` | list of [Variable](#variable) | Module variables, constants and type aliases, in source order. |
| `all` | list of strings, optional | Names in a literal `__all__`; `none` without one. |
| `is_document` | boolean | A `#pd-write` page, rendered with `document.html`. |
| `is_private` | boolean | The module or one of its packages starts with `_`. Only shown with `--include-private`. |
| `z_index` | integer | Sort key from `#pd-z-index`. |

### Class

| Field | Type | Description |
|---|---|---|
| `name` | string | The class name. |
| `qualname` | string | Dotted name inside the module (`"Outer.Inner"` for a nested class). Used in anchors: `id="class.{{ cls.qualname }}"`. |
| `bases` | list of strings | Base classes as written. |
| `line` | integer | Line of the `class` statement. |
| `docstring` | HTML, optional | The docstring, without its sections. |
| `summary` | string, optional | First line of the docstring, as plain text. |
| `sections` | [DocSections](#docsections), optional | Sections taken out of the docstring. |
| `badges` | list of strings | `"conditional"` (defined under `if`/`try`) or `"type-checking"` (only under `if TYPE_CHECKING:`). |
| `functions` | list of [Function](#function) | Methods. |
| `classes` | list of [Class](#class) | Classes defined in the class body. |
| `variables` | list of [Variable](#variable) | Class attributes, enum members and dataclass fields. |
| `linked` | [LinkedTypes](#linkedtypes), optional | `bases` with links to documented classes; `none` when nothing links. |
| `is_private` | boolean | The name starts with `_`, or is left out of `__all__`. |

### Function

| Field | Type | Description |
|---|---|---|
| `name` | string | The function name. |
| `args` | list of strings | Parameters as written, with annotations and defaults (`"timeout: float = 1.0"`). |
| `return_type` | string, optional | Return annotation as written. |
| `decorators` | list of strings | Decorators as written, without `@`. |
| `badges` | list of strings | From well-known decorators: `"property"`, `"setter"`, `"static"`, `"classmethod"`, `"abstract"`, `"overload"`, plus `"conditional"` or `"type-checking"`. |
| `is_async` | boolean | Defined with `async def`. |
| `line` | integer | Line of the `def` statement. |
| `docstring` | HTML, optional | The docstring, without its sections. |
| `summary` | string, optional | First line of the docstring, as plain text. |
| `sections` | [DocSections](#docsections), optional | Sections taken out of the docstring. |
| `linked` | [LinkedTypes](#linkedtypes), optional | `args` and `return_type` with links to documented types; `none` when nothing links. |
| `is_private` | boolean | The name starts with `_`, or is left out of `__all__`. |

### Variable

| Field | Type | Description |
|---|---|---|
| `name` | string | The variable name. |
| `annotation` | string, optional | The annotation as written. |
| `value` | string, optional | The value as written, whitespace collapsed. |
| `value_truncated` | boolean | `value` was cut short; show an ellipsis. |
| `is_type_alias` | boolean | Declared with the `type` statement. |
| `badges` | list of strings | `"enum"` for enum members, `"field"` for dataclass fields, plus `"conditional"` or `"type-checking"`. |
| `line` | integer | Line of the assignment. |
| `docstring` | HTML, optional | From a string right after the assignment, or `#:` comments. |
| `summary` | string, optional | First line of the docstring, as plain text. |
| `linked` | [LinkedTypes](#linkedtypes), optional | `annotation` with links to documented types; `none` when nothing links. |
| `is_private` | boolean | The name starts with `_`, or is left out of `__all__`. |

### LinkedTypes

| Field | Type | Description |
|---|---|---|
| `html` | HTML | A function's parameters joined with `", "`, a class's bases joined the same way, or a variable's annotation, with links. |
| `return_type` | HTML, optional | A function's return annotation, with links. |

### DocSections

Every list is empty when the docstring has no such section.

| Field | Type | Description |
|---|---|---|
| `style` | string | `"google"`, `"numpy"` or `"rest"`. |
| `params` | list of [DocItem](#docitem) | `Args:` / `Parameters`. |
| `keyword_params` | list of [DocItem](#docitem) | `Keyword Args:`. |
| `other_params` | list of [DocItem](#docitem) | `Other Parameters`. |
| `attributes` | list of [DocItem](#docitem) | `Attributes:`. |
| `returns` | list of [DocItem](#docitem) | `Returns:`. |
| `yields` | list of [DocItem](#docitem) | `Yields:`. |
| `raises` | list of [DocItem](#docitem) | `Raises:`. |
| `examples` | list of [DocBlock](#docblock) | `Example:` / `Examples:`. |
| `notes` | list of [DocBlock](#docblock) | `Note:` / `Notes:`. |
| `warnings` | list of [DocBlock](#docblock) | `Warning:` / `Warnings:`. |
| `see_also` | list of [DocBlock](#docblock) | `See Also:`. |

### DocItem

| Field | Type | Description |
|---|---|---|
| `name` | string, optional | The parameter or attribute name; `none` for return values and exceptions. |
| `type` | string, optional | The type from the docstring, else from the signature. For an exception, its class. |
| `type_html` | HTML, optional | `type` with links to documented types; `none` when nothing links. |
| `default` | string, optional | The default value, from the signature. |
| `description` | string | The description as written (Markdown). |
| `description_html` | HTML | The description, rendered. |

### DocBlock

| Field | Type | Description |
|---|---|---|
| `text` | string | The section as written (Markdown). |
| `html` | HTML | The section, rendered. Notes and warnings use the same boxes as `#pd-note` and `#pd-warning`. |
