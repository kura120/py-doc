//! `--format markdown`: one `.md` page per module plus `index.md`,
//! `llms.txt` and `llms-full.txt`, for MkDocs, Docusaurus, wikis and LLM
//! tools.
//!
//! Pages mirror the HTML site's paths (`core/engine.html` becomes
//! `core/engine.md`). Docstrings are written from the raw text, which is
//! Markdown already, with the `#pd-` macros turned into plain Markdown.
//!
//! Anchors: every symbol heading is preceded by `<a id="..."></a>` with the
//! same id the HTML site uses (`class.Engine`, `fn.boot`, `var.TIMEOUT`,
//! `method.Engine.run`, `attr.Engine.x`), so doc-links resolve the same way
//! in both formats. A raw HTML anchor works on GitHub, in MkDocs and in
//! Docusaurus without any Markdown extension.
//!
//! Problems with macros (a missing image, an unknown doc-link) were already
//! reported while rendering, so they are not reported again here; the
//! Markdown keeps the text and never emits a link that does not resolve.

use regex::Regex;
use std::fmt::Write as _;
use std::sync::LazyLock;

use crate::generator::{
    DocMacro, SiteGenerator, clean_docstring, extract_z_index_and_clean, root_prefix_for,
};
use crate::models::{PythonClass, PythonFunction, PythonModule, PythonPackage, PythonVariable};
use crate::resolver::Resolver;

pub const INDEX: &str = "index.md";
pub const LLMS: &str = "llms.txt";
pub const LLMS_FULL: &str = "llms-full.txt";

/// `![alt](path)` with a local path, which is rewritten to the copied asset.
static IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)\s]+)\)").expect("valid regex"));

/// The `.md` path of an HTML page path.
pub fn md_path(link_path: &str) -> String {
    match link_path.strip_suffix(".html") {
        Some(stem) => format!("{stem}.md"),
        None => link_path.to_string(),
    }
}

/// Per-page state for converting docstrings.
struct Page<'a> {
    generator: &'a SiteGenerator,
    resolver: &'a Resolver,
    /// "./" or "../../": from this page to the output root.
    root_prefix: String,
}

impl Page<'_> {
    /// A doc-link target as a link relative to this page, if it resolves.
    fn link(&self, target: &str) -> Option<String> {
        let resolved = self.resolver.resolve(target)?;
        let (path, anchor) = match resolved.split_once('#') {
            Some((path, anchor)) => (path, Some(anchor)),
            None => (resolved.as_str(), None),
        };
        let mut link = format!("{}{}", self.root_prefix, md_path(path));
        if let Some(anchor) = anchor {
            link.push('#');
            link.push_str(anchor);
        }
        Some(link)
    }

    fn image(&self, path: &str) -> Option<String> {
        self.generator
            .copy_image(path)
            .ok()
            .map(|site_path| format!("{}{}", self.root_prefix, site_path))
    }

    /// Rewrites local `![alt](path)` images to the copied assets.
    fn rewrite_images(&self, line: &str) -> String {
        IMAGE
            .replace_all(line, |caps: &regex::Captures| {
                let (alt, url) = (&caps[1], &caps[2]);
                let external = ["http://", "https://", "data:", "//"]
                    .iter()
                    .any(|scheme| url.starts_with(scheme));
                match (external, self.image(url)) {
                    (false, Some(asset)) => format!("![{alt}]({asset})"),
                    _ => caps[0].to_string(),
                }
            })
            .into_owned()
    }

    /// A raw docstring as Markdown: dedented, `#pd-` macros converted,
    /// doctest sessions fenced as `pycon`, and fenced blocks kept as written.
    fn docstring(&self, raw: &str) -> String {
        let (_, cleaned) = extract_z_index_and_clean(raw);
        let text = clean_docstring(&cleaned);
        let mut out = String::new();
        let mut in_fence = false;
        let mut custom_code = false;
        let mut doctest_indent: Option<usize> = None;

        for line in text.lines() {
            let trimmed = line.trim();

            if custom_code {
                out.push_str(line);
                out.push('\n');
                if trimmed == "```" {
                    custom_code = false;
                    out.push('\n');
                }
                continue;
            }
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
            }
            if in_fence || trimmed.starts_with("```") {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            if let Some(indent) = doctest_indent {
                if trimmed.is_empty() {
                    doctest_indent = None;
                    out.push_str("```\n\n");
                } else {
                    out.push_str(line.get(indent..).unwrap_or(trimmed));
                    out.push('\n');
                }
                continue;
            }
            if trimmed.starts_with(">>>") {
                doctest_indent = Some(line.len() - line.trim_start().len());
                out.push_str("\n```pycon\n");
                out.push_str(trimmed);
                out.push('\n');
                continue;
            }
            if trimmed.starts_with("#pd-write") {
                continue;
            }

            match DocMacro::parse(line) {
                Some(DocMacro::Note { text }) => {
                    let _ = write!(out, "\n> **Note:** {text}\n\n");
                }
                Some(DocMacro::Warning { text }) => {
                    let _ = write!(out, "\n> **Warning:** {text}\n\n");
                }
                Some(DocMacro::DocLink { target }) => match self.link(target) {
                    Some(link) => {
                        let _ = write!(out, "\nSee reference: [`{target}`]({link})\n\n");
                    }
                    None => {
                        let _ = write!(out, "\nSee reference: `{target}` (not found)\n\n");
                    }
                },
                Some(DocMacro::Image { path }) => match self.image(path) {
                    Some(asset) => {
                        let _ = write!(out, "\n![]({asset})\n\n");
                    }
                    None => {
                        let _ = write!(out, "\n*Image not found: `{path}`*\n\n");
                    }
                },
                Some(DocMacro::CodeBlockStart { language }) => {
                    custom_code = true;
                    let _ = write!(out, "\n```{language}\n");
                }
                None => {
                    out.push_str(&self.rewrite_images(line));
                    out.push('\n');
                }
            }
        }
        if doctest_indent.is_some() || custom_code {
            out.push_str("```\n");
        }
        collapse_blank_lines(out.trim())
    }
}

/// At most one blank line in a row, outside fenced blocks, so macros that
/// pad themselves with blank lines do not pile them up.
fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1);
    let mut in_fence = false;
    let mut blank = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && line.trim().is_empty() {
            if !blank {
                out.push('\n');
            }
            blank = true;
            continue;
        }
        blank = false;
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn anchor(out: &mut String, level: usize, id: &str, title: &str) {
    let _ = write!(
        out,
        "<a id=\"{id}\"></a>\n\n{} {title}\n\n",
        "#".repeat(level)
    );
}

fn code(out: &mut String, signature: &str) {
    let _ = write!(out, "```python\n{signature}\n```\n\n");
}

fn variable_signature(var: &PythonVariable) -> String {
    let mut sig = String::new();
    if var.is_type_alias {
        sig.push_str("type ");
    }
    sig.push_str(&var.name);
    if let Some(annotation) = &var.annotation {
        let _ = write!(sig, ": {annotation}");
    }
    if let Some(value) = &var.value {
        let _ = write!(sig, " = {value}");
        if var.value_truncated {
            sig.push('…');
        }
    }
    sig
}

fn function_signature(func: &PythonFunction) -> String {
    let mut sig = String::new();
    for decorator in &func.decorators {
        let _ = writeln!(sig, "@{decorator}");
    }
    if func.is_async {
        sig.push_str("async ");
    }
    let _ = write!(sig, "def {}({})", func.name, func.args.join(", "));
    if let Some(ret) = &func.return_type {
        let _ = write!(sig, " -> {ret}");
    }
    sig.push_str(": ...");
    sig
}

fn class_signature(class: &PythonClass) -> String {
    if class.bases.is_empty() {
        format!("class {}: ...", class.name)
    } else {
        format!("class {}({}): ...", class.name, class.bases.join(", "))
    }
}

/// The page title: the dotted name, or the package name for the root.
fn title(module: &PythonModule, package: &str) -> String {
    if module.qualname.is_empty() {
        package.to_string()
    } else {
        module.qualname.clone()
    }
}

fn badges(out: &mut String, badges: &[String]) {
    if !badges.is_empty() {
        let _ = write!(out, "*{}*\n\n", badges.join(" · "));
    }
}

fn doc(out: &mut String, page: &Page, raw: &Option<String>) {
    if let Some(raw) = raw {
        let text = page.docstring(raw);
        if !text.trim().is_empty() {
            out.push_str(&text);
            out.push('\n');
        }
    }
}

fn function(out: &mut String, page: &Page, func: &PythonFunction, level: usize, id: &str) {
    anchor(out, level, id, &format!("`{}()`", func.name));
    code(out, &function_signature(func));
    badges(out, &func.badges);
    doc(out, page, &func.raw_docstring);
}

fn variable(out: &mut String, page: &Page, var: &PythonVariable, level: usize, id: &str) {
    anchor(out, level, id, &format!("`{}`", var.name));
    code(out, &variable_signature(var));
    badges(out, &var.badges);
    doc(out, page, &var.raw_docstring);
}

fn module_page(module: &PythonModule, package: &PythonPackage, page: &Page) -> String {
    let mut out = String::new();
    if module.is_document {
        // A doc-only page: its docstring has its own headings.
        doc(&mut out, page, &module.raw_docstring);
        return out;
    }

    let _ = write!(out, "# `{}`\n\n", title(module, &package.name));
    doc(&mut out, page, &module.raw_docstring);

    if !module.variables.is_empty() {
        out.push_str("## Variables\n\n");
        for var in &module.variables {
            variable(&mut out, page, var, 3, &format!("var.{}", var.name));
        }
    }
    if !module.classes.is_empty() {
        out.push_str("## Classes\n\n");
        for class in &module.classes {
            anchor(
                &mut out,
                3,
                &format!("class.{}", class.name),
                &format!("class `{}`", class.name),
            );
            code(&mut out, &class_signature(class));
            doc(&mut out, page, &class.raw_docstring);
            if !class.variables.is_empty() {
                out.push_str("#### Attributes\n\n");
                for var in &class.variables {
                    let id = format!("attr.{}.{}", class.name, var.name);
                    variable(&mut out, page, var, 5, &id);
                }
            }
            if !class.functions.is_empty() {
                out.push_str("#### Methods\n\n");
                for func in &class.functions {
                    let id = format!("method.{}.{}", class.name, func.name);
                    function(&mut out, page, func, 5, &id);
                }
            }
        }
    }
    if !module.functions.is_empty() {
        out.push_str("## Functions\n\n");
        for func in &module.functions {
            function(&mut out, page, func, 3, &format!("fn.{}", func.name));
        }
    }
    out
}

/// Modules in the site's order: `SiteGenerator::render` sorts them by
/// `#pd-z-index`, then name, then path, so pinned modules come first and
/// the order is stable.
fn sorted(package: &PythonPackage) -> Vec<&PythonModule> {
    package.modules.iter().collect()
}

/// The root `__init__.py`, whose docstring introduces the package.
fn root_init(package: &PythonPackage) -> Option<&PythonModule> {
    package
        .modules
        .iter()
        .find(|m| m.qualname.is_empty() && m.name == "__init__")
}

fn index_page(package: &PythonPackage, version: &str, page: &Page) -> String {
    let mut out = format!("# {}\n\n", package.name);
    let _ = write!(out, "Version {version}\n\n");
    if let Some(root) = root_init(package) {
        doc(&mut out, page, &root.raw_docstring);
    }
    out.push_str("## Modules\n\n");
    for module in sorted(package) {
        let _ = write!(
            out,
            "- [`{}`](./{})",
            title(module, &package.name),
            md_path(&module.link_path)
        );
        if let Some(summary) = &module.summary {
            let _ = write!(out, ": {summary}");
        }
        out.push('\n');
    }
    out
}

/// https://llmstxt.org: a title, a one-line summary and a list of pages.
fn llms_txt(package: &PythonPackage) -> String {
    let mut out = format!("# {}\n\n", package.name);
    if let Some(summary) = root_init(package).and_then(|m| m.summary.as_deref()) {
        let _ = write!(out, "> {summary}\n\n");
    }
    let _ = write!(
        out,
        "API reference generated by py-doc. `{LLMS_FULL}` holds every page in one file.\n\n## API\n\n"
    );
    for module in sorted(package) {
        let _ = write!(
            out,
            "- [{}]({})",
            title(module, &package.name),
            md_path(&module.link_path)
        );
        if let Some(summary) = &module.summary {
            let _ = write!(out, ": {summary}");
        }
        out.push('\n');
    }
    out
}

/// Writes every page, `index.md`, `llms.txt` and `llms-full.txt` for a
/// package that `SiteGenerator::render` prepared.
pub fn write(
    generator: &SiteGenerator,
    package: &PythonPackage,
    version: &str,
) -> anyhow::Result<()> {
    let resolver = Resolver::build(package);
    let page_for = |link_path: &str| Page {
        generator,
        resolver: &resolver,
        root_prefix: root_prefix_for(link_path),
    };

    let mut full = String::new();
    for module in sorted(package) {
        let path = md_path(&module.link_path);
        let text = module_page(module, package, &page_for(&module.link_path));
        generator.write_file(&path, &text)?;
        let _ = write!(full, "<!-- {path} -->\n\n{text}\n");
    }
    let index = index_page(package, version, &page_for(INDEX));
    generator.write_file(INDEX, &index)?;
    generator.write_file(LLMS, &llms_txt(package))?;
    generator.write_file(LLMS_FULL, &format!("<!-- {INDEX} -->\n\n{index}\n{full}"))?;
    Ok(())
}
