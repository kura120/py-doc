use anyhow::{Context as AnyhowContext, Result, anyhow};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tera::{Context as TeraContext, Kwargs, State, Tera};

use crate::highlight::highlight_python;
use crate::models::{PythonClass, PythonFunction, PythonModule, PythonPackage};
use crate::parser::split_order_prefix;
use crate::resolver::Resolver;

/// The bundled theme. Compiled into the binary so generation works offline
/// and a binary always pairs with the templates it was built with.
const EMBEDDED_ASSETS: [(&str, &str); 6] = [
    ("sidebar.html", include_str!("../assets/sidebar.html")),
    ("index.html", include_str!("../assets/index.html")),
    ("module.html", include_str!("../assets/module.html")),
    ("document.html", include_str!("../assets/document.html")),
    ("style.css", include_str!("../assets/style.css")),
    ("app.js", include_str!("../assets/app.js")),
];
const TEMPLATE_NAMES: [&str; 4] = ["sidebar.html", "index.html", "module.html", "document.html"];
const STATIC_NAMES: [&str; 2] = ["style.css", "app.js"];

static TAG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"<[^>]*>").expect("valid regex"));

#[derive(Debug, Clone, serde::Serialize)]
pub struct NavGroup {
    /// Display path of the folder ("core/sub"); None for root-level modules.
    pub name: Option<String>,
    /// Link to this folder's own __init__.py page, if it has one.
    pub link_path: Option<String>,
    /// Docstring pulled from the folder's __init__.py (merged in, not shown as its own card).
    pub docstring: Option<String>,
    /// Classes defined directly in the folder's __init__.py.
    pub classes: Vec<PythonClass>,
    /// Functions defined directly in the folder's __init__.py.
    pub functions: Vec<PythonFunction>,
    /// Child modules of this folder, excluding its own __init__/__main__.
    pub modules: Vec<PythonModule>,
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Helper to dynamically strip Python indentation from docstrings
fn clean_docstring(docstring: &str) -> String {
    let lines: Vec<&str> = docstring.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut cleaned = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            cleaned.push_str(line);
        } else if !line.trim().is_empty() {
            cleaned.push_str(line.get(min_indent..).unwrap_or_else(|| line.trim_start()));
        }
        cleaned.push('\n');
    }
    cleaned
}

/// Helper to parse and strip the `#pd-z-index` macro from a docstring
fn extract_z_index_and_clean(docstring: &str) -> (i32, String) {
    let mut z_index = i32::MAX;
    let mut cleaned_lines = Vec::new();

    for line in docstring.lines() {
        if let Some(value) = line.trim().strip_prefix("#pd-z-index:") {
            if let Ok(parsed_val) = value.trim().parse::<i32>() {
                z_index = parsed_val;
            }
            continue;
        }
        cleaned_lines.push(line);
    }

    (z_index, cleaned_lines.join("\n"))
}

/// First meaningful line of a raw docstring as plain text, for cards and search.
fn summarize(docstring: &str) -> Option<String> {
    docstring
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("#pd-") && !line.starts_with("```"))
        .map(|line| {
            let line = line.trim_start_matches('#').trim();
            let mut summary: String = line.chars().take(160).collect();
            if line.chars().count() > 160 {
                summary.push('…');
            }
            summary
        })
        .filter(|summary| !summary.is_empty())
}

// Custom tera striptags filter
fn striptags_filter(value: &str, _args: Kwargs, _state: &State) -> Result<String, tera::Error> {
    Ok(TAG_RE.replace_all(value, "").into_owned())
}

enum AlertType {
    Error,
    Note,
    Warning,
}

const ICON_NOTE: &str = r#"<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><circle cx="10" cy="10" r="7.25" stroke="currentColor" stroke-width="1.4"/><path d="M10 9v4.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><circle cx="10" cy="6.75" r="0.9" fill="currentColor"/></svg>"#;
const ICON_WARNING: &str = r#"<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M10 3.5 17.5 16h-15L10 3.5Z" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M10 8.5v3.25" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><circle cx="10" cy="13.75" r="0.9" fill="currentColor"/></svg>"#;
const ICON_ERROR: &str = r#"<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><circle cx="10" cy="10" r="7.25" stroke="currentColor" stroke-width="1.4"/><path d="M7.5 7.5l5 5M12.5 7.5l-5 5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>"#;
const ICON_LINK: &str = r#"<svg viewBox="0 0 20 20" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><path d="M8.5 4.5H15.5V11.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/><path d="M15.5 4.5 8 12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><path d="M11.5 6.5H5A1.5 1.5 0 0 0 3.5 8v6A1.5 1.5 0 0 0 5 15.5h6a1.5 1.5 0 0 0 1.5-1.5v-2.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/></svg>"#;

impl AlertType {
    /// `content` is inserted as-is: callers pass markdown/HTML they trust or have escaped.
    fn to_html(&self, content: &str) -> String {
        let (class_name, icon, title) = match self {
            AlertType::Error => ("alert-error", ICON_ERROR, "Error"),
            AlertType::Note => ("alert-note", ICON_NOTE, "Note"),
            AlertType::Warning => ("alert-warning", ICON_WARNING, "Warning"),
        };

        format!(
            "\n\n<div class=\"alert {}\">\n  <strong><span class=\"alert-icon\">{}</span>{}:</strong> {}\n</div>\n\n",
            class_name, icon, title, content
        )
    }
}

enum DocMacro<'a> {
    Image { path: &'a str },
    Note { text: &'a str },
    Warning { text: &'a str },
    DocLink { target: &'a str },
    CodeBlockStart { language: String },
}

impl<'a> DocMacro<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let trimmed = line.trim();

        if let Some(path) = trimmed.strip_prefix("#pd-image:") {
            Some(DocMacro::Image { path: path.trim() })
        } else if let Some(text) = trimmed.strip_prefix("#pd-note:") {
            Some(DocMacro::Note { text: text.trim() })
        } else if let Some(text) = trimmed.strip_prefix("#pd-warning:") {
            Some(DocMacro::Warning { text: text.trim() })
        } else if let Some(target) = trimmed.strip_prefix("#pd-doc-link:") {
            Some(DocMacro::DocLink {
                target: target.trim(),
            })
        } else if let Some(lang_part) = trimmed.strip_prefix("#pd-code") {
            let language = lang_part.trim().trim_matches('`').to_string();
            Some(DocMacro::CodeBlockStart { language })
        } else {
            None
        }
    }
}

/// Per-page state needed while rendering one module's docstrings.
struct RenderCtx<'a> {
    /// "./" or "../../": how this page reaches the site root.
    root_prefix: &'a str,
    resolver: &'a Resolver,
    /// The module being rendered, for warning messages.
    origin: &'a str,
}

pub struct SiteGenerator {
    pub tera: Tera,
    /// Canonical root that all module and image paths are resolved against.
    src_root: PathBuf,
    pub output_dir: String,
    pub version: String,
    pub template_dir: Option<PathBuf>,
    pub layout: String,
    /// None means "follow the reader's system setting".
    pub theme: Option<String>,
    /// Base URL of a browsable copy of the source tree, for "source" links.
    pub source_url: Option<String>,
    warnings: AtomicUsize,
}

/// Where a module's page is written, relative to the site root.
fn output_path_for(source_path: &str) -> String {
    let stem = source_path.strip_suffix(".py").unwrap_or(source_path);
    if stem == "__init__" {
        // The root package's __init__.py must not claim index.html: that
        // is the site's landing page.
        "__init__.html".to_string()
    } else if stem == "index" {
        "index.module.html".to_string()
    } else if let Some(dir) = stem.strip_suffix("/__init__") {
        format!("{}/index.html", dir)
    } else {
        format!("{}.html", stem)
    }
}

/// Relative prefix that takes a page back to the site root.
fn root_prefix_for(link_path: &str) -> String {
    match link_path.matches('/').count() {
        0 => "./".to_string(),
        depth => "../".repeat(depth),
    }
}

fn module_order(a: &PythonModule, b: &PythonModule) -> std::cmp::Ordering {
    a.z_index
        .cmp(&b.z_index)
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.source_path.cmp(&b.source_path))
}

impl SiteGenerator {
    pub fn new(
        src_root: &Path,
        output_dir: &str,
        version: &str,
        template_dir: Option<&str>,
        layout: &str,
        theme: Option<&str>,
        source_url: Option<&str>,
    ) -> Result<Self> {
        let template_dir = template_dir.map(PathBuf::from);
        let mut overridden = Vec::new();

        let mut tera = Tera::default();
        tera.register_filter("striptags", striptags_filter);
        for name in TEMPLATE_NAMES {
            let (body, is_custom) = load_asset(template_dir.as_deref(), name)?;
            if is_custom {
                overridden.push(name);
            }
            tera.add_raw_template(name, &body)
                .with_context(|| format!("Failed to load template {}", name))?;
        }
        if let Some(dir) = &template_dir {
            println!(
                "  Using custom templates from '{}': {}",
                dir.display(),
                if overridden.is_empty() {
                    "none found, using the bundled theme".to_string()
                } else {
                    overridden.join(", ")
                }
            );
        }

        let src_root = src_root.canonicalize().with_context(|| {
            format!(
                "Failed to resolve source directory '{}'",
                src_root.display()
            )
        })?;

        Ok(Self {
            tera,
            src_root,
            output_dir: output_dir.to_string(),
            version: version.to_string(),
            template_dir,
            layout: layout.to_string(),
            theme: theme.map(str::to_string),
            source_url: source_url.map(|url| url.trim_end_matches('/').to_string()),
            warnings: AtomicUsize::new(0),
        })
    }

    /// Number of non-fatal problems found (missing images, unresolved doc-links).
    pub fn warning_count(&self) -> usize {
        self.warnings.load(Ordering::Relaxed)
    }

    fn warn(&self, origin: &str, message: &str) {
        self.warnings.fetch_add(1, Ordering::Relaxed);
        eprintln!("  \x1b[33m⚠\x1b[0m {}: {}", origin, message);
    }

    /// Copies an image into the site and returns the URL to use from the
    /// current page. Paths are relative to the source root and must stay inside it.
    fn process_image_path(&self, img_path: &str, ctx: &RenderCtx) -> Result<String> {
        let resolved = self
            .src_root
            .join(img_path)
            .canonicalize()
            .map_err(|_| anyhow!("Image file not found: '{}'", img_path))?;
        let relative = resolved
            .strip_prefix(&self.src_root)
            .map_err(|_| anyhow!("Image path '{}' is outside the source directory", img_path))?;

        let dest = Path::new(&self.output_dir).join("assets").join(relative);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&resolved, &dest)?;

        let url_path = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        Ok(format!("{}assets/{}", ctx.root_prefix, url_path))
    }

    fn render_macro(&self, mac: DocMacro, ctx: &RenderCtx) -> String {
        match mac {
            DocMacro::Image { path } => match self.process_image_path(path, ctx) {
                Ok(url) => format!("\n\n<img src=\"{}\" alt=\"\">\n\n", escape_html(&url)),
                Err(e) => {
                    self.warn(ctx.origin, &e.to_string());
                    AlertType::Error.to_html(&escape_html(&e.to_string()))
                }
            },
            DocMacro::Note { text } => AlertType::Note.to_html(text),
            DocMacro::Warning { text } => AlertType::Warning.to_html(text),
            DocMacro::DocLink { target } => {
                let label = format!("<code>{}</code>", escape_html(target));
                let link_html = match ctx.resolver.resolve(target) {
                    Some(link) => format!(
                        "<a href=\"{}{}\" class=\"doc-internal-link\">{}</a>",
                        ctx.root_prefix,
                        escape_html(&link),
                        label
                    ),
                    None => {
                        self.warn(
                            ctx.origin,
                            &format!("#pd-doc-link target '{}' was not found", target),
                        );
                        format!("{} <em>(not found)</em>", label)
                    }
                };
                format!(
                    "\n\n<div class=\"doc-link-container\"><span class=\"doc-link-icon\">{}</span>See Reference: {}</div>\n\n",
                    ICON_LINK, link_html
                )
            }
            DocMacro::CodeBlockStart { .. } => String::new(),
        }
    }

    fn render_markdown(&self, markdown: &str, ctx: &RenderCtx) -> String {
        let dedented_md = clean_docstring(markdown);

        let mut processed_md = String::new();
        let mut in_custom_code = false;
        let mut code_language = String::new();
        let mut code_accumulator = String::new();
        let mut in_fence = false;
        // Indentation of the doctest session being collected, if any.
        let mut doctest_indent: Option<usize> = None;

        for line in dedented_md.lines() {
            let trimmed = line.trim();

            if in_custom_code {
                if trimmed == "```" {
                    in_custom_code = false;
                    processed_md.push_str(&format!(
                        "\n\n```{}\n{}\n```\n\n",
                        code_language,
                        code_accumulator.trim_end()
                    ));
                } else {
                    code_accumulator.push_str(line);
                    code_accumulator.push('\n');
                }
                continue;
            }

            // Inside an ordinary fenced block everything is literal.
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
            }
            if in_fence || trimmed.starts_with("```") {
                processed_md.push_str(line);
                processed_md.push('\n');
                continue;
            }

            // A doctest session (">>> ..." up to the next blank line) becomes
            // a code block instead of being folded into the paragraph above.
            if let Some(indent) = doctest_indent {
                if trimmed.is_empty() {
                    doctest_indent = None;
                    processed_md.push_str("```\n\n");
                } else {
                    let unindented = line.get(indent..).unwrap_or(trimmed);
                    processed_md.push_str(unindented);
                    processed_md.push('\n');
                }
                continue;
            }
            if trimmed.starts_with(">>>") {
                doctest_indent = Some(line.len() - line.trim_start().len());
                processed_md.push_str("\n```pycon\n");
                processed_md.push_str(trimmed);
                processed_md.push('\n');
                continue;
            }

            if trimmed.starts_with("#pd-write") {
                continue;
            }

            if let Some(mac) = DocMacro::parse(line) {
                match mac {
                    DocMacro::CodeBlockStart { language } => {
                        in_custom_code = true;
                        code_language = language;
                        code_accumulator.clear();
                    }
                    other_macro => processed_md.push_str(&self.render_macro(other_macro, ctx)),
                }
                continue;
            }

            processed_md.push_str(line);
            processed_md.push('\n');
        }
        if doctest_indent.is_some() {
            processed_md.push_str("```\n");
        }

        let options =
            Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;

        let mut events: Vec<Event> = Vec::new();
        // (language, text) of the fenced/indented code block being collected
        let mut code_block: Option<(String, String)> = None;

        for event in Parser::new_ext(&processed_md, options) {
            match event {
                Event::Start(Tag::CodeBlock(kind)) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => {
                            info.split_whitespace().next().unwrap_or("").to_string()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_block = Some((language, String::new()));
                }
                Event::Text(text) if code_block.is_some() => {
                    if let Some((_, buffer)) = code_block.as_mut() {
                        buffer.push_str(&text);
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((language, code)) = code_block.take() {
                        events.push(Event::Html(CowStr::from(render_code_block(
                            &language, &code,
                        ))));
                    }
                }
                Event::Start(Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => {
                    let is_external = ["http://", "https://", "data:", "//"]
                        .iter()
                        .any(|scheme| dest_url.starts_with(scheme));
                    if is_external {
                        events.push(Event::Start(Tag::Image {
                            link_type,
                            dest_url,
                            title,
                            id,
                        }));
                    } else {
                        match self.process_image_path(&dest_url, ctx) {
                            Ok(new_path) => events.push(Event::Start(Tag::Image {
                                link_type,
                                dest_url: CowStr::from(new_path),
                                title,
                                id,
                            })),
                            Err(e) => {
                                self.warn(ctx.origin, &e.to_string());
                                events.push(Event::Text(CowStr::from(format!(
                                    "[⚠️ Image Error: {}]",
                                    e
                                ))));
                            }
                        }
                    }
                }
                other => events.push(other),
            }
        }

        let mut html_output = String::new();
        html::push_html(&mut html_output, events.into_iter());
        html_output
    }

    /// Fills in summaries, ordering, the doc-only flag and rendered HTML for one module.
    fn render_module(&self, module: &mut PythonModule, resolver: &Resolver) {
        let root_prefix = root_prefix_for(&module.link_path);
        let origin = module.source_path.clone();
        let ctx = RenderCtx {
            root_prefix: &root_prefix,
            resolver,
            origin: &origin,
        };

        if let Some(raw_doc) = module.docstring.take() {
            let (z_val, cleaned_doc) = extract_z_index_and_clean(&raw_doc);
            module.z_index = z_val;
            // Must be decided on the raw docstring: render_markdown strips the marker.
            module.is_document = cleaned_doc.trim_start().starts_with("#pd-write")
                && module.classes.is_empty()
                && module.functions.is_empty();
            module.summary = summarize(&cleaned_doc);
            module.docstring = Some(self.render_markdown(&cleaned_doc, &ctx));
        }

        let render_function = |func: &mut PythonFunction| {
            if let Some(doc) = func.docstring.take() {
                func.summary = summarize(&doc);
                func.docstring = Some(self.render_markdown(&doc, &ctx));
            }
        };

        for class in module.classes.iter_mut() {
            if let Some(doc) = class.docstring.take() {
                class.summary = summarize(&doc);
                class.docstring = Some(self.render_markdown(&doc, &ctx));
            }
            class.functions.iter_mut().for_each(render_function);
        }
        module.functions.iter_mut().for_each(render_function);
    }

    /// Groups modules by directory into the folder structure used by the
    /// index page and the sidebar.
    fn build_nav_groups(&self, modules: &[PythonModule]) -> Vec<NavGroup> {
        // Keyed by raw directory so that two folders never merge just
        // because they share a leaf name.
        let mut groups_map: std::collections::BTreeMap<&str, Vec<PythonModule>> =
            std::collections::BTreeMap::new();
        for module in modules {
            groups_map
                .entry(module.dir.as_str())
                .or_default()
                .push(module.clone());
        }

        let mut nav_groups: Vec<(i32, NavGroup)> = Vec::new();
        for (dir, mut mods) in groups_map {
            mods.sort_by(module_order);

            // __main__ never reaches here in practice (main.rs skips it while
            // scanning), but strip it defensively regardless of folder.
            mods.retain(|m| m.name != "__main__");

            let folder_name = mods.first().and_then(|m| m.folder.clone());
            let folder_z = dir
                .rsplit('/')
                .next()
                .and_then(|leaf| split_order_prefix(leaf).0)
                .unwrap_or(i32::MAX);

            let init_pos = mods.iter().position(|m| m.name == "__init__");
            let (link_path, docstring, classes, functions) = match (dir.is_empty(), init_pos) {
                // A real folder: pull its __init__.py out and merge its
                // content into the folder itself instead of listing it
                // as its own separate module card.
                (false, Some(pos)) => {
                    let init_mod = mods.remove(pos);
                    (
                        Some(init_mod.link_path),
                        init_mod.docstring,
                        init_mod.classes,
                        init_mod.functions,
                    )
                }
                // Root-level bucket: the package's own __init__.py is
                // represented by the landing page header instead.
                (true, Some(pos)) => {
                    mods.remove(pos);
                    (None, None, Vec::new(), Vec::new())
                }
                (_, None) => (None, None, Vec::new(), Vec::new()),
            };

            if dir.is_empty() && mods.is_empty() {
                continue;
            }

            nav_groups.push((
                folder_z,
                NavGroup {
                    name: if dir.is_empty() {
                        None
                    } else {
                        folder_name.or(Some(dir.to_string()))
                    },
                    link_path,
                    docstring,
                    classes,
                    functions,
                    modules: mods,
                },
            ));
        }

        // Sort folders globally
        nav_groups.sort_by(|(z_a, group_a), (z_b, group_b)| {
            let effective_z = |z: &i32, group: &NavGroup| {
                if group.name.is_none() {
                    group.modules.first().map(|m| m.z_index).unwrap_or(i32::MAX)
                } else {
                    *z
                }
            };
            let size = |group: &NavGroup| {
                group.modules.len() + group.classes.len() + group.functions.len()
            };

            effective_z(z_a, group_a)
                .cmp(&effective_z(z_b, group_b))
                // descending: bigger/busier folders surface first
                .then_with(|| size(group_b).cmp(&size(group_a)))
                .then_with(|| group_a.name.cmp(&group_b.name))
        });

        nav_groups.into_iter().map(|(_, group)| group).collect()
    }

    fn base_context(&self, package: &PythonPackage, root_path: &str) -> TeraContext {
        let mut context = TeraContext::new();
        context.insert("package", package);
        context.insert("version", &self.version);
        context.insert("root_path", root_path);
        context.insert("layout", &self.layout);
        if let Some(theme) = &self.theme {
            context.insert("theme", theme);
        }
        if let Some(source_url) = &self.source_url {
            context.insert("source_url", source_url);
        }
        context
    }

    pub fn generate(&self, package: &mut PythonPackage) -> Result<()> {
        let compile_start = Instant::now();

        fs::create_dir_all(&self.output_dir)?;

        // Output locations first: doc-links and image URLs depend on them.
        for module in package.modules.iter_mut() {
            module.link_path = output_path_for(&module.source_path);
        }
        let resolver = Resolver::build(package);

        package
            .modules
            .par_iter_mut()
            .for_each(|module| self.render_module(module, &resolver));

        package.modules.sort_by(module_order);

        let nav_groups = self.build_nav_groups(&package.modules);

        // At-a-glance totals for the index page header
        let total_modules = package
            .modules
            .iter()
            .filter(|m| m.name != "__init__" && m.name != "__main__")
            .count();
        let total_classes: usize = package.modules.iter().map(|m| m.classes.len()).sum();
        let total_functions: usize = package.modules.iter().map(|m| m.functions.len()).sum();

        // The root package's own __init__.py feeds the landing page header.
        let root_init = package
            .modules
            .iter()
            .find(|m| m.link_path == "__init__.html");

        // Render index page
        let mut index_context = self.base_context(package, "./");
        index_context.insert("nav_groups", &nav_groups);
        index_context.insert("total_modules", &total_modules);
        index_context.insert("total_classes", &total_classes);
        index_context.insert("total_functions", &total_functions);
        if let Some(doc) = root_init.and_then(|m| m.docstring.as_ref()) {
            index_context.insert("package_docstring", doc);
        }
        if let Some(root) = root_init
            && (!root.classes.is_empty() || !root.functions.is_empty())
        {
            index_context.insert("package_link_path", &root.link_path);
        }

        let rendered_index = self.tera.render("index.html", &index_context)?;
        fs::write(
            Path::new(&self.output_dir).join("index.html"),
            rendered_index,
        )?;

        // Render detailed individual module detail pages
        for module in &package.modules {
            let absolute_output_path = Path::new(&self.output_dir).join(&module.link_path);

            if let Some(parent_dir) = absolute_output_path.parent() {
                fs::create_dir_all(parent_dir).with_context(|| {
                    format!("Failed to create output directory {:?}", parent_dir)
                })?;
            }

            let mut mod_context = self.base_context(package, &root_prefix_for(&module.link_path));
            mod_context.insert("nav_groups", &nav_groups);
            mod_context.insert("module", module);

            let template_name = if module.is_document {
                "document.html"
            } else {
                "module.html"
            };

            let rendered_mod = self
                .tera
                .render(template_name, &mod_context)
                .with_context(|| format!("Failed to render {}", module.source_path))?;
            fs::write(&absolute_output_path, rendered_mod)
                .with_context(|| format!("Failed to write {:?}", absolute_output_path))?;
        }

        println!("\x1b[36;1m[3/3]\x1b[0m Writing search index, navigation and theme assets...");

        fs::write(
            Path::new(&self.output_dir).join("search-index.js"),
            format!("const searchIndex = {};", search_index_json(package)),
        )?;
        fs::write(
            Path::new(&self.output_dir).join("nav-data.js"),
            format!("const navData = {};", nav_data_json(&nav_groups)),
        )?;

        for name in STATIC_NAMES {
            let (body, _) = load_asset(self.template_dir.as_deref(), name)?;
            fs::write(Path::new(&self.output_dir).join(name), body)
                .with_context(|| format!("Failed to write {}", name))?;
        }

        println!(
            "  \x1b[32m✔\x1b[0m Generated {} pages in {:.2?}",
            package.modules.len() + 1,
            compile_start.elapsed()
        );

        Ok(())
    }
}

/// Reads a theme file from the custom template directory when it exists
/// there, and from the bundled theme otherwise. The flag reports which.
fn load_asset(template_dir: Option<&Path>, name: &str) -> Result<(String, bool)> {
    if let Some(dir) = template_dir {
        let path = dir.join(name);
        if path.is_file() {
            let body = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read custom template {:?}", path))?;
            return Ok((body, true));
        }
    }
    let body = EMBEDDED_ASSETS
        .iter()
        .find(|(asset_name, _)| *asset_name == name)
        .map(|(_, body)| *body)
        .ok_or_else(|| anyhow!("Unknown bundled asset '{}'", name))?;
    Ok((body.to_string(), false))
}

fn render_code_block(language: &str, code: &str) -> String {
    let body = match language {
        "python" | "py" | "python3" | "pycon" => highlight_python(code),
        _ => escape_html(code),
    };
    if language.is_empty() {
        format!("<pre><code>{}</code></pre>\n", body)
    } else {
        format!(
            "<pre><code class=\"language-{}\">{}</code></pre>\n",
            escape_html(language),
            body
        )
    }
}

fn function_entries(functions: &[PythonFunction], with_summary: bool) -> Vec<serde_json::Value> {
    functions
        .iter()
        .map(|f| {
            if with_summary {
                serde_json::json!({ "name": f.name, "summary": f.summary })
            } else {
                serde_json::json!({ "name": f.name })
            }
        })
        .collect()
}

/// A deliberately small search payload: names, links and one-line
/// summaries only. No rendered HTML and no build-machine paths.
fn search_index_json(package: &PythonPackage) -> serde_json::Value {
    let modules: Vec<serde_json::Value> = package
        .modules
        .iter()
        .map(|m| {
            serde_json::json!({
                "name": if m.qualname.is_empty() { &package.name } else { &m.qualname },
                "link_path": m.link_path,
                "summary": m.summary,
                "classes": m.classes.iter().map(|c| serde_json::json!({
                    "name": c.name,
                    "summary": c.summary,
                    "functions": function_entries(&c.functions, true),
                })).collect::<Vec<_>>(),
                "functions": function_entries(&m.functions, true),
            })
        })
        .collect();

    serde_json::json!({ "name": package.name, "modules": modules })
}

/// Sidebar tree payload consumed by app.js: names and links only.
fn nav_data_json(nav_groups: &[NavGroup]) -> serde_json::Value {
    let symbols = |classes: &[PythonClass], functions: &[PythonFunction]| {
        (
            classes
                .iter()
                .map(|c| serde_json::json!({ "name": c.name }))
                .collect::<Vec<_>>(),
            function_entries(functions, false),
        )
    };

    nav_groups
        .iter()
        .map(|group| {
            let (classes, functions) = symbols(&group.classes, &group.functions);
            serde_json::json!({
                "name": group.name,
                "link_path": group.link_path,
                "classes": classes,
                "functions": functions,
                "modules": group.modules.iter().map(|m| {
                    let (classes, functions) = symbols(&m.classes, &m.functions);
                    serde_json::json!({
                        "name": m.name,
                        "link_path": m.link_path,
                        "classes": classes,
                        "functions": functions,
                    })
                }).collect::<Vec<_>>(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_init_does_not_claim_the_landing_page() {
        assert_eq!(output_path_for("__init__.py"), "__init__.html");
        assert_eq!(output_path_for("index.py"), "index.module.html");
        assert_eq!(output_path_for("01_core/__init__.py"), "01_core/index.html");
        assert_eq!(output_path_for("01_core/engine.py"), "01_core/engine.html");
    }

    #[test]
    fn root_prefix_matches_page_depth() {
        assert_eq!(root_prefix_for("guide.html"), "./");
        assert_eq!(root_prefix_for("a/b/c.html"), "../../");
    }

    #[test]
    fn z_index_macro_is_parsed_and_removed() {
        let (z, cleaned) = extract_z_index_and_clean("#pd-write\n#pd-z-index: 3\n# Title");
        assert_eq!(z, 3);
        assert_eq!(cleaned, "#pd-write\n# Title");
    }

    #[test]
    fn summary_skips_macros_and_heading_markers() {
        assert_eq!(
            summarize("#pd-write\n\n# Getting Started\nBody").as_deref(),
            Some("Getting Started")
        );
        assert_eq!(summarize("\n  \n"), None);
    }

    #[test]
    fn docstring_indentation_is_removed() {
        assert_eq!(
            clean_docstring("Title.\n\n    Body\n      nested\n"),
            "Title.\n\nBody\n  nested\n"
        );
    }
}
