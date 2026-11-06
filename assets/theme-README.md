# py-doc theme

These are the templates, CSS and JavaScript built into the py-doc binary that wrote them, as a starting point for your own theme. Use the folder with `--templates`:

```bash
py-doc --src ./myproject --out ./docs --templates ./py-doc-theme
```

or in `pyproject.toml`:

```toml
[tool.py-doc]
templates = "py-doc-theme"
```

Unchanged, these files produce exactly the default site.

## Keep only what you change

A file you delete falls back to the bundled one, so a newer py-doc can still fix it. The bundled files are also available under the `py-doc/` prefix, so a template can extend the one it replaces and override a single block:

```html
{% extends "py-doc/module.html" %}

{% block header %}
<h1 class="module-title">{{ display_module_name }}</h1>
{% endblock header %}
```

`{{ super() }}` inside a block keeps the bundled content.

## Files

| File | What it is |
| --- | --- |
| `base.html` | Page skeleton. Blocks: `head` (with `title`, `description`, `stylesheets`), `body_attributes`, `sidebar`, `header`, `content`, `outline`, `footer` (empty), `scripts` |
| `index.html` | The landing page; extends `base.html` |
| `module.html` | A module page; extends `base.html`. `content` holds `module_docstring`, `classes` and `functions` |
| `document.html` | A `#pd-write` page; extends `base.html` |
| `sidebar.html` | Navigation, search and the layout and theme selectors |
| `style.css` | All styles. Colors, fonts and sizes are CSS custom properties |
| `app.js` | Navigation tree, search and theme switching |

Any other `*.html` file in this folder, including ones in subfolders, can be included or extended by name, for example `{% include "partials/footer.html" %}`.

## Colors and fonts only

To change just colors and fonts, you don't need this folder. Keep the bundled theme and pass a small stylesheet with `--css`:

```css
:root {
    --accent-blue: #2563eb;
}
```

Each `--css` file is linked after `style.css` on every page. Templates get their paths as `extra_css`, and `base.html` links them in its `stylesheets` block; a template that replaces that block has to link them itself.

The py-doc README lists the custom properties and the template variables.
