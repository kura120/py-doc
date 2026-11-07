use anyhow::{Context as AnyhowContext, Result, anyhow, bail};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html};
use rayon::prelude::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Instant;
use tera::{Context as TeraContext, Kwargs, State, Tera, Value};

use crate::cache::{self, Cache, ModuleDeps, ModuleEntryRef, PageEntry, hash_bytes, hash_parts};
use crate::highlight::highlight_python;
use crate::models::{PythonClass, PythonFunction, PythonModule, PythonPackage, PythonVariable};
use crate::parser::{self, split_order_prefix};
use crate::resolver::Resolver;

/// The bundled theme. Compiled into the binary so generation works offline
/// and a binary always pairs with the templates it was built with.
const EMBEDDED_ASSETS: [(&str, &str); 7] = [
    ("base.html", include_str!("../assets/base.html")),
    ("sidebar.html", include_str!("../assets/sidebar.html")),
    ("index.html", include_str!("../assets/index.html")),
    ("module.html", include_str!("../assets/module.html")),
    ("document.html", include_str!("../assets/document.html")),
    ("style.css", include_str!("../assets/style.css")),
    ("app.js", include_str!("../assets/app.js")),
];
const TEMPLATE_NAMES: [&str; 5] = [
    "base.html",
    "sidebar.html",
    "index.html",
    "module.html",
    "document.html",
];
/// The bundled templates are registered under this prefix, so a custom
/// template can extend the one it replaces (`{% extends "py-doc/module.html" %}`).
/// Tera falls back to it for any name the custom directory does not provide.
const BUNDLED_PREFIX: &str = "py-doc/";
const STATIC_NAMES: [&str; 2] = ["style.css", "app.js"];
/// Written next to the ejected theme to explain how to use it.
const THEME_README: &str = include_str!("../assets/theme-README.md");
/// Lists every file a run wrote, so a later `--clean` run can remove the
/// ones that are no longer produced without touching anything else.
const MANIFEST_NAME: &str = ".py-doc-manifest";

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

fn starts_with_pd_write(cleaned_doc: &str) -> bool {
    cleaned_doc.trim_start().starts_with("#pd-write")
}

/// True when a raw module docstring marks a doc-only page (`#pd-write`,
/// possibly after a `#pd-z-index` line). Shared with the coverage report.
pub fn is_document_docstring(raw_doc: &str) -> bool {
    starts_with_pd_write(&extract_z_index_and_clean(raw_doc).1)
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
            // Accepts "#pd-code", "#pd-code python", "#pd-code: python" and
            // "#pd-code ```python". With no language the block is Python.
            let language = lang_part
                .trim()
                .trim_start_matches(':')
                .trim()
                .trim_matches('`');
            let language = if language.is_empty() {
                "python".to_string()
            } else {
                language.to_string()
            };
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
    /// What rendering looked at, for the incremental-build cache.
    deps: RefCell<ModuleDeps>,
}

/// The previous run's cache (None for a full build) and this run's key.
pub struct RunCache {
    pub previous: Option<Cache>,
    pub key: String,
}

/// A package rendered by [`SiteGenerator::render`], with what the cache
/// needs to skip pages and to be saved.
pub struct Prepared {
    pub package: PythonPackage,
    /// Per module of `package`, in the same order.
    rendered: Vec<SlotInfo>,
    private_files: Vec<PrivateFile>,
    old_pages: BTreeMap<String, PageEntry>,
    key: String,
    /// Whether the modules differ from the previous cache's.
    modules_changed: bool,
}

/// One source file handed to [`SiteGenerator::render`].
pub struct ModuleSource {
    pub path: PathBuf,
    pub source_hash: String,
    /// The parsed module, or None when the cache holds this exact file.
    pub parsed: Option<PythonModule>,
}

/// The cache state carried with a module through one run.
struct Rendered {
    deps: ModuleDeps,
    full_hash: String,
    shared_hash: String,
}

/// A public module's file during a run.
struct ModuleSlot {
    path: PathBuf,
    source_path: String,
    source_hash: String,
    /// The file's private part, for the resolver.
    hidden: Option<PythonModule>,
    /// Set once the module is rendered, or up front when the cache has it.
    rendered: Option<Rendered>,
}

/// A [`ModuleSlot`] once its module is rendered.
struct SlotInfo {
    source_path: String,
    source_hash: String,
    hidden: Option<PythonModule>,
    rendered: Rendered,
}

/// A file whose module is private as a whole.
struct PrivateFile {
    source_path: String,
    source_hash: String,
    hidden: Option<PythonModule>,
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
    /// Site-relative paths of every file written in this run.
    written: Mutex<BTreeSet<String>>,
    /// Hash of every template and static file in use.
    theme_fingerprint: String,
    /// Some template comes from --templates. Such a template may show any
    /// part of any module on every page, so pages are not reused selectively.
    custom_templates: bool,
    /// The "Using custom templates" line, printed by `announce_templates`.
    templates_note: Option<String>,
    /// `--css` files as (site path, contents), linked after style.css.
    extra_css: Vec<(String, String)>,
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

/// The name a page shows for its module: a package's `__init__` is shown
/// under its folder's name, or the package name at the top level.
fn display_module_name<'a>(module: &'a PythonModule, package_name: &'a str) -> &'a str {
    if module.name != "__init__" {
        return &module.name;
    }
    match module.folder.as_deref() {
        Some(folder) if !folder.is_empty() => folder,
        _ => package_name,
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

        let mut tera = Tera::default();
        tera.register_filter("striptags", striptags_filter);
        tera.set_fallback_prefixes([BUNDLED_PREFIX])
            .context("Failed to configure the template engine")?;

        let custom = match &template_dir {
            Some(dir) => custom_templates(dir)?,
            None => Vec::new(),
        };
        let mut templates: Vec<(String, String)> = TEMPLATE_NAMES
            .iter()
            .map(|name| {
                Ok((
                    format!("{BUNDLED_PREFIX}{name}"),
                    bundled_asset(name)?.to_string(),
                ))
            })
            .collect::<Result<_>>()?;
        templates.extend(custom.iter().cloned());

        let mut theme_parts: Vec<String> = templates
            .iter()
            .flat_map(|(name, body)| [name.clone(), body.clone()])
            .collect();
        for name in STATIC_NAMES {
            let (body, _) = load_asset(template_dir.as_deref(), name)?;
            theme_parts.push(name.to_string());
            theme_parts.push(body);
        }
        let theme_fingerprint = hash_parts(theme_parts.iter().map(String::as_str));

        tera.add_raw_templates(templates)
            .context("Failed to load templates")?;

        let templates_note = template_dir.as_ref().map(|dir| {
            let names: Vec<&str> = custom.iter().map(|(name, _)| name.as_str()).collect();
            format!(
                "  Using custom templates from '{}': {}",
                dir.display(),
                if names.is_empty() {
                    "none found, using the bundled theme".to_string()
                } else {
                    names.join(", ")
                }
            )
        });

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
            written: Mutex::new(BTreeSet::new()),
            theme_fingerprint,
            custom_templates: !custom.is_empty(),
            templates_note,
            extra_css: Vec::new(),
        })
    }

    /// Prints which custom templates are used, if --templates was given.
    pub fn announce_templates(&self) {
        if let Some(note) = &self.templates_note {
            status!("{}", note);
        }
    }

    /// Hash of the template and static files, for the cache key.
    pub fn theme_fingerprint(&self) -> &str {
        &self.theme_fingerprint
    }

    /// Adds `--css` stylesheets. Each is copied to `custom/<n>-<name>`: the
    /// index keeps the order they were given in and keeps two files with
    /// the same name apart. A file that cannot be read is an error.
    pub fn with_extra_css(mut self, files: &[String]) -> Result<Self> {
        for (index, file) in files.iter().enumerate() {
            let path = Path::new(file);
            if !path.is_file() {
                bail!("the CSS file '{}' given with --css does not exist", file);
            }
            let contents = fs::read_to_string(path)
                .with_context(|| format!("Failed to read CSS file '{}'", file))?;
            let name: String = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            // The stylesheet is part of the cache key, like the theme.
            self.theme_fingerprint =
                hash_parts([self.theme_fingerprint.as_str(), file.as_str(), &contents]);
            self.extra_css
                .push((format!("custom/{}-{}", index, name), contents));
        }
        Ok(self)
    }

    /// Number of non-fatal problems found (missing images, unresolved doc-links).
    pub fn warning_count(&self) -> usize {
        self.warnings.load(Ordering::Relaxed)
    }

    fn record(&self, site_path: &str) {
        self.written
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(site_path.to_string());
    }

    /// Writes the manifest for this run. With `clean`, first deletes files
    /// that an earlier run listed and this run no longer produces. Only
    /// files named in a manifest are ever removed, so anything else kept in
    /// the output folder (a CNAME, hand-written pages) is safe.
    fn update_manifest(&self, clean: bool) -> Result<usize> {
        let out = Path::new(&self.output_dir);
        let manifest_path = out.join(MANIFEST_NAME);
        let written = self
            .written
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let previous = fs::read_to_string(&manifest_path).ok();
        if clean && previous.is_none() {
            status!(
                "  --clean: this folder has no {} from an earlier run, so nothing was removed.",
                MANIFEST_NAME
            );
        }

        // Files earlier runs wrote that this run did not.
        let leftovers: Vec<&str> = previous
            .as_deref()
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|site_path| {
                !site_path.is_empty()
                    && !written.contains(*site_path)
                    && is_safe_site_path(site_path)
                    && out.join(site_path).is_file()
            })
            .collect();

        let mut listed: BTreeSet<&str> = written.iter().map(String::as_str).collect();
        let mut removed = 0;
        for site_path in leftovers {
            let stale = out.join(site_path);
            if clean && fs::remove_file(&stale).is_ok() {
                removed += 1;
                remove_empty_parents(out, &stale);
            } else {
                // Still on disk: keep it listed so a later --clean can remove it.
                listed.insert(site_path);
            }
        }

        let mut body = listed.into_iter().collect::<Vec<_>>().join("\n");
        body.push('\n');
        if previous.as_deref() != Some(body.as_str()) {
            fs::write(&manifest_path, body)
                .with_context(|| format!("Failed to write {:?}", manifest_path))?;
        }
        Ok(removed)
    }

    fn warn(&self, origin: &str, message: &str) {
        self.warnings.fetch_add(1, Ordering::Relaxed);
        eprintln!("  \x1b[33m⚠\x1b[0m {}: {}", origin, message);
    }

    /// Warns about the module being rendered, and remembers the warning so
    /// a later run that reuses this module reports it again.
    fn warn_at(&self, ctx: &RenderCtx, message: &str) {
        self.warn(ctx.origin, message);
        ctx.deps.borrow_mut().warnings.push(message.to_string());
    }

    /// Resolves an image path (relative to the source root, and inside it)
    /// to the file and its URL path below `assets/`.
    fn resolve_image(&self, img_path: &str) -> Result<(PathBuf, String)> {
        let resolved = self
            .src_root
            .join(img_path)
            .canonicalize()
            .map_err(|_| anyhow!("Image file not found: '{}'", img_path))?;
        let relative = resolved
            .strip_prefix(&self.src_root)
            .map_err(|_| anyhow!("Image path '{}' is outside the source directory", img_path))?;

        let url_path = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        Ok((resolved, url_path))
    }

    /// What the cache records about an image path: where it is copied to and
    /// a hash of its bytes, or why it cannot be used.
    fn image_status(&self, img_path: &str) -> String {
        let found = self.resolve_image(img_path).and_then(|(file, url_path)| {
            let bytes = fs::read(&file)?;
            Ok(format!("ok {} {}", hash_bytes(&bytes), url_path))
        });
        match found {
            Ok(status) => status,
            Err(e) => format!("err {}", e),
        }
    }

    /// Copies an image into the site once per run. With `only_if_missing`
    /// an existing file is kept (the cache vouches for its content).
    fn copy_image(&self, resolved: &Path, url_path: &str, only_if_missing: bool) -> Result<()> {
        let site_path = format!("assets/{}", url_path);

        // Modules render in parallel and often share an image. Copy each
        // one once, under the lock, so two threads never write the same file.
        let mut written = self
            .written
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !written.contains(&site_path) {
            let dest = Path::new(&self.output_dir).join(&site_path);
            if !(only_if_missing && dest.is_file()) {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(resolved, &dest)?;
            }
            written.insert(site_path);
        }
        Ok(())
    }

    /// Copies an image into the site and returns the URL to use from the
    /// current page. Paths are relative to the source root and must stay inside it.
    fn process_image_path(&self, img_path: &str, ctx: &RenderCtx) -> Result<String> {
        ctx.deps
            .borrow_mut()
            .images
            .push((img_path.to_string(), self.image_status(img_path)));
        let (resolved, url_path) = self.resolve_image(img_path)?;
        self.copy_image(&resolved, &url_path, false)?;
        Ok(format!("{}assets/{}", ctx.root_prefix, url_path))
    }

    /// Whether a cached module's rendering still holds: every doc-link
    /// resolves as before and every image is unchanged.
    fn deps_hold(&self, deps: &ModuleDeps, resolver: &Resolver) -> bool {
        deps.links.iter().all(|(target, link, hidden)| {
            let now = resolver.resolve(target);
            let now_hidden = now.is_none() && resolver.is_hidden(target);
            now == *link && now_hidden == *hidden
        }) && deps
            .images
            .iter()
            .all(|(path, status)| self.image_status(path) == *status)
    }

    /// The side effects of rendering a module that is reused from the
    /// cache: its warnings and its images.
    fn replay(&self, module: &PythonModule, deps: &ModuleDeps) -> Result<()> {
        for warning in &deps.warnings {
            self.warn(&module.source_path, warning);
        }
        for (path, status) in &deps.images {
            if status.starts_with("ok ") {
                let (resolved, url_path) = self.resolve_image(path)?;
                self.copy_image(&resolved, &url_path, true)?;
            }
        }
        Ok(())
    }

    fn render_macro(&self, mac: DocMacro, ctx: &RenderCtx) -> String {
        match mac {
            DocMacro::Image { path } => match self.process_image_path(path, ctx) {
                Ok(url) => format!("\n\n<img src=\"{}\" alt=\"\">\n\n", escape_html(&url)),
                Err(e) => {
                    self.warn_at(ctx, &e.to_string());
                    AlertType::Error.to_html(&escape_html(&e.to_string()))
                }
            },
            DocMacro::Note { text } => AlertType::Note.to_html(&render_inline(text)),
            DocMacro::Warning { text } => AlertType::Warning.to_html(&render_inline(text)),
            DocMacro::DocLink { target } => {
                let label = format!("<code>{}</code>", escape_html(target));
                let resolved = ctx.resolver.resolve(target);
                let hidden = resolved.is_none() && ctx.resolver.is_hidden(target);
                ctx.deps
                    .borrow_mut()
                    .links
                    .push((target.to_string(), resolved.clone(), hidden));
                let link_html = match resolved {
                    Some(link) => format!(
                        "<a href=\"{}{}\" class=\"doc-internal-link\">{}</a>",
                        ctx.root_prefix,
                        escape_html(&link),
                        label
                    ),
                    None if hidden => {
                        self.warn_at(
                            ctx,
                            &format!(
                                "#pd-doc-link target '{}' is private and hidden from the docs (see --include-private)",
                                target
                            ),
                        );
                        format!("{} <em>(not found)</em>", label)
                    }
                    None => {
                        self.warn_at(
                            ctx,
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

            if trimmed.starts_with("#pd-") {
                let name = trimmed.split([':', ' ']).next().unwrap_or(trimmed);
                let hint = if name == "#pd-z-index" {
                    " (it only applies in a module docstring)"
                } else {
                    ""
                };
                self.warn_at(
                    ctx,
                    &format!("'{}' is not a macro that works here{}", name, hint),
                );
            }

            processed_md.push_str(line);
            processed_md.push('\n');
        }
        if doctest_indent.is_some() {
            processed_md.push_str("```\n");
        }
        if in_custom_code {
            // Never drop content: an unterminated block runs to the end of the docstring.
            self.warn_at(
                ctx,
                "#pd-code block is not closed with a line containing only ```",
            );
            processed_md.push_str(&format!(
                "\n\n```{}\n{}\n```\n\n",
                code_language,
                code_accumulator.trim_end()
            ));
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
                                self.warn_at(ctx, &e.to_string());
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

    /// Fills in summaries, ordering, the doc-only flag and rendered HTML for
    /// one module. Returns what the rendering depended on.
    fn render_module(&self, module: &mut PythonModule, resolver: &Resolver) -> ModuleDeps {
        let root_prefix = root_prefix_for(&module.link_path);
        let origin = module.source_path.clone();
        let ctx = RenderCtx {
            root_prefix: &root_prefix,
            resolver,
            origin: &origin,
            deps: RefCell::new(ModuleDeps::default()),
        };

        if let Some(raw_doc) = module.docstring.take() {
            let (z_val, cleaned_doc) = extract_z_index_and_clean(&raw_doc);
            module.z_index = z_val;
            // Must be decided on the raw docstring: render_markdown strips the marker.
            module.is_document = starts_with_pd_write(&cleaned_doc)
                && module.classes.is_empty()
                && module.functions.is_empty();
            module.summary = summarize(&cleaned_doc);
            module.docstring = Some(self.render_markdown(&cleaned_doc, &ctx));
            module.raw_docstring = Some(raw_doc);
        }

        let render_function = |func: &mut PythonFunction| {
            if let Some(doc) = func.docstring.take() {
                func.summary = summarize(&doc);
                func.docstring = Some(self.render_markdown(&doc, &ctx));
                func.raw_docstring = Some(doc);
            }
        };

        let render_variable = |var: &mut PythonVariable| {
            if let Some(doc) = var.docstring.take() {
                var.summary = summarize(&doc);
                var.docstring = Some(self.render_markdown(&doc, &ctx));
                var.raw_docstring = Some(doc);
            }
        };

        // Nested classes are rendered too: walk them with an explicit stack.
        let mut stack: Vec<&mut PythonClass> = module.classes.iter_mut().collect();
        while let Some(class) = stack.pop() {
            if let Some(doc) = class.docstring.take() {
                class.summary = summarize(&doc);
                class.docstring = Some(self.render_markdown(&doc, &ctx));
                class.raw_docstring = Some(doc);
            }
            class.functions.iter_mut().for_each(render_function);
            class.variables.iter_mut().for_each(render_variable);
            stack.extend(class.classes.iter_mut());
        }
        module.functions.iter_mut().for_each(render_function);
        module.variables.iter_mut().for_each(render_variable);
        ctx.deps.into_inner()
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

    /// `package` and `nav_groups` are serialized once by the caller: Tera
    /// values are reference-counted, so every page shares one copy instead of
    /// re-serializing the whole package (all rendered docstrings) per page.
    fn base_context(&self, package: &Value, nav_groups: &Value, root_path: &str) -> TeraContext {
        let mut context = TeraContext::new();
        context.insert_value("package", package.clone());
        context.insert_value("nav_groups", nav_groups.clone());
        context.insert("version", &self.version);
        context.insert("root_path", root_path);
        context.insert("layout", &self.layout);
        let extra_css: Vec<&str> = self
            .extra_css
            .iter()
            .map(|(path, _)| path.as_str())
            .collect();
        context.insert("extra_css", &extra_css);
        if let Some(theme) = &self.theme {
            context.insert("theme", theme);
        }
        if let Some(source_url) = &self.source_url {
            context.insert("source_url", source_url);
        }
        context
    }

    /// Skips a page when the cache says it was rendered from exactly these
    /// inputs and the file on disk is still the one written then.
    fn current_page(
        &self,
        old_pages: &BTreeMap<String, PageEntry>,
        site_path: &str,
        inputs: &str,
    ) -> Option<PageEntry> {
        let entry = old_pages.get(site_path)?;
        if entry.inputs != inputs {
            return None;
        }
        let on_disk = fs::read(Path::new(&self.output_dir).join(site_path)).ok()?;
        if hash_bytes(&on_disk) != entry.output {
            return None;
        }
        self.record(site_path);
        Some(entry.clone())
    }

    fn write_page(&self, site_path: &str, inputs: &str, body: String) -> Result<PageEntry> {
        let path = Path::new(&self.output_dir).join(site_path);
        if let Some(parent_dir) = path.parent() {
            fs::create_dir_all(parent_dir)
                .with_context(|| format!("Failed to create output directory {:?}", parent_dir))?;
        }
        let output = hash_bytes(body.as_bytes());
        fs::write(&path, body).with_context(|| format!("Failed to write {:?}", path))?;
        self.record(site_path);
        Ok(PageEntry {
            inputs: inputs.to_string(),
            output,
        })
    }

    /// Writes a non-page file unless it already has exactly this content.
    fn write_if_changed(&self, site_path: &str, body: &[u8]) -> Result<()> {
        let path = Path::new(&self.output_dir).join(site_path);
        if fs::read(&path).ok().as_deref() != Some(body) {
            fs::write(&path, body).with_context(|| format!("Failed to write {}", site_path))?;
        }
        self.record(site_path);
        Ok(())
    }

    /// Writes `contents` to `site_path` in the output folder and lists it in
    /// the manifest. For output formats other than the HTML site.
    pub fn write_file(&self, site_path: &str, contents: &str) -> Result<()> {
        let path = Path::new(&self.output_dir).join(site_path);
        fs::write(&path, contents).with_context(|| format!("Failed to write {:?}", path))?;
        self.record(site_path);
        Ok(())
    }

    /// Parses what the cache cannot supply, then renders every docstring to
    /// HTML in place (keeping the original in `raw_docstring` for modules
    /// rendered in this run), fills in page paths, summaries and ordering,
    /// and copies the images docstrings use. `cache` is the previous run's
    /// cache (see [`RunCache`]); a module it supplies is already rendered and
    /// has no `raw_docstring`, so a run that writes `api.json` passes none.
    /// Without `include_private`, private names are hidden as by
    /// [`PythonPackage::hide_private`].
    pub fn render(
        &self,
        name: &str,
        sources: Vec<ModuleSource>,
        parse_root: &Path,
        cache: RunCache,
        include_private: bool,
    ) -> Result<Prepared> {
        let RunCache {
            previous: cache,
            key,
        } = cache;
        fs::create_dir_all(&self.output_dir)?;

        let mut old = cache.unwrap_or_default();
        // An old cache stays sound even if this run is interrupted: module
        // entries are keyed by content, and a page is only reused when the
        // file on disk is still the one the cache recorded.
        let old_pages = std::mem::take(&mut old.pages);

        let mut package = PythonPackage {
            name: name.to_string(),
            modules: Vec::with_capacity(sources.len()),
            hidden: Vec::new(),
        };
        let mut slots = Vec::with_capacity(sources.len());
        // Files that are private as a whole: no page, only names for the resolver.
        let mut private_files: Vec<PrivateFile> = Vec::new();
        let mut parsed_files = 0;
        for source in sources {
            let source_path = cache::source_path_for(&source.path, parse_root);
            let (public, hidden, rendered) = match source.parsed {
                Some(module) => {
                    parsed_files += 1;
                    let (public, hidden) = split_private(module, include_private);
                    (public, hidden, None)
                }
                None => match old.modules.remove(&source_path) {
                    Some(entry) => {
                        let rendered = entry.module.is_some().then_some(Rendered {
                            deps: entry.deps,
                            full_hash: entry.full_hash,
                            shared_hash: entry.shared_hash,
                        });
                        (entry.module, entry.hidden, rendered)
                    }
                    None => {
                        parsed_files += 1;
                        let module = parser::parse_file(&source.path, parse_root)?;
                        let (public, hidden) = split_private(module, include_private);
                        (public, hidden, None)
                    }
                },
            };
            match public {
                Some(module) => {
                    package.modules.push(module);
                    slots.push(ModuleSlot {
                        path: source.path,
                        source_path,
                        source_hash: source.source_hash,
                        hidden,
                        rendered,
                    });
                }
                None => private_files.push(PrivateFile {
                    source_path,
                    source_hash: source.source_hash,
                    hidden,
                }),
            }
        }
        package.hidden = slots
            .iter()
            .filter_map(|slot| slot.hidden.clone())
            .chain(private_files.iter().filter_map(|file| file.hidden.clone()))
            .collect();

        // Output locations first: doc-links and image URLs depend on them.
        for module in package.modules.iter_mut() {
            module.link_path = output_path_for(&module.source_path);
        }
        // Built from names only, so cached (rendered) modules serve as well
        // as freshly parsed ones.
        let resolver = Resolver::build(&package);

        // A cached module whose doc-links now resolve differently, or whose
        // images changed, is parsed and rendered again. The file itself is
        // unchanged, so its public and hidden parts are the same as cached.
        let stale: Vec<usize> = slots
            .par_iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.rendered
                    .as_ref()
                    .is_some_and(|rendered| !self.deps_hold(&rendered.deps, &resolver))
            })
            .map(|(index, _)| index)
            .collect();
        for index in stale {
            let module = parser::parse_file(&slots[index].path, parse_root)?;
            let (public, _) = split_private(module, include_private);
            let mut module = public
                .ok_or_else(|| anyhow!("{} changed while it was read", slots[index].source_path))?;
            module.link_path = output_path_for(&module.source_path);
            package.modules[index] = module;
            slots[index].rendered = None;
        }
        let fresh_modules = slots.iter().filter(|s| s.rendered.is_none()).count();

        package
            .modules
            .par_iter_mut()
            .zip(slots.par_iter_mut())
            .try_for_each(|(module, slot)| -> Result<()> {
                match &slot.rendered {
                    Some(rendered) => self.replay(module, &rendered.deps)?,
                    None => {
                        let deps = self.render_module(module, &resolver);
                        let full_hash = rendered_hash(&slot.source_hash, &deps, module)?;
                        let shared_hash = shared_hash(module)?;
                        slot.rendered = Some(Rendered {
                            deps,
                            full_hash,
                            shared_hash,
                        });
                    }
                }
                Ok(())
            })?;

        let mut pairs = package
            .modules
            .drain(..)
            .zip(slots)
            .map(|(module, slot)| {
                let rendered = slot
                    .rendered
                    .ok_or_else(|| anyhow!("{} was not rendered", module.source_path))?;
                Ok((
                    module,
                    SlotInfo {
                        source_path: slot.source_path,
                        source_hash: slot.source_hash,
                        hidden: slot.hidden,
                        rendered,
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        pairs.sort_by(|a, b| module_order(&a.0, &b.0));
        let (modules, rendered): (Vec<PythonModule>, Vec<SlotInfo>) = pairs.into_iter().unzip();

        // Left-over entries in `old` are modules that were edited or deleted.
        let modules_changed = parsed_files > 0 || fresh_modules > 0 || !old.modules.is_empty();
        Ok(Prepared {
            package: PythonPackage {
                name: package.name,
                modules,
                hidden: package.hidden,
            },
            rendered,
            private_files,
            old_pages,
            key,
            modules_changed,
        })
    }

    /// Saves the cache for the next run: the modules [`Self::render`]
    /// prepared and the pages [`Self::write_html`] wrote. A cache that could
    /// not be saved only costs the next run its speed, so it is a warning.
    pub fn save_cache(&self, prepared: &Prepared, pages: &BTreeMap<String, PageEntry>) {
        if !prepared.modules_changed && *pages == prepared.old_pages {
            return;
        }
        let no_deps = ModuleDeps::default();
        let public =
            prepared
                .package
                .modules
                .iter()
                .zip(&prepared.rendered)
                .map(|(module, info)| ModuleEntryRef {
                    source_path: &info.source_path,
                    source_hash: &info.source_hash,
                    dir: &module.dir,
                    module: Some(module),
                    hidden: info.hidden.as_ref(),
                    deps: &info.rendered.deps,
                    full_hash: &info.rendered.full_hash,
                    shared_hash: &info.rendered.shared_hash,
                });
        let private = prepared.private_files.iter().map(|file| ModuleEntryRef {
            source_path: &file.source_path,
            source_hash: &file.source_hash,
            dir: "",
            module: None,
            hidden: file.hidden.as_ref(),
            deps: &no_deps,
            full_hash: "",
            shared_hash: "",
        });
        let entries: Vec<ModuleEntryRef> = public.chain(private).collect();
        let saved = cache::encode_modules(&entries).and_then(|bytes| {
            cache::write(Path::new(&self.output_dir), &prepared.key, &bytes, pages)
        });
        if let Err(e) = saved {
            eprintln!("  \x1b[33m⚠\x1b[0m could not save the build cache: {:#}", e);
        }
    }

    /// Writes the manifest, after every format has written its files. With
    /// `clean`, first removes what earlier runs wrote and this one did not.
    pub fn finish(&self, clean: bool) -> Result<()> {
        let removed = self.update_manifest(clean)?;
        if removed > 0 {
            status!(
                "  \x1b[32m✔\x1b[0m Removed {} stale file{} from an earlier run",
                removed,
                if removed == 1 { "" } else { "s" }
            );
        }
        Ok(())
    }

    /// Writes the HTML site for a package that [`Self::render`] prepared,
    /// skipping each page whose inputs and file on disk are as the cache
    /// recorded. Returns the pages for [`Self::save_cache`].
    pub fn write_html(&self, prepared: &Prepared) -> Result<BTreeMap<String, PageEntry>> {
        let compile_start = Instant::now();
        let package = &prepared.package;
        let rendered = &prepared.rendered;
        let old_pages = &prepared.old_pages;

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

        // What each page is rendered from. Every page gets the whole package
        // and nav in its context; the bundled module and document templates
        // only show other modules' names, links, signatures and summaries
        // (the "shared" hash), never their docstrings. Custom templates
        // might, so with those every module's full content counts. The
        // index shows everything.
        let mut global_parts: Vec<&str> = vec![&prepared.key, &package.name];
        for info in rendered {
            global_parts.push(&info.rendered.shared_hash);
            if self.custom_templates {
                global_parts.push(&info.rendered.full_hash);
            }
        }
        let global = hash_parts(global_parts);
        let index_inputs = hash_parts(
            [global.as_str(), "index.html"]
                .into_iter()
                .chain(rendered.iter().map(|info| info.rendered.full_hash.as_str())),
        );
        let page_inputs: Vec<String> = package
            .modules
            .iter()
            .zip(rendered)
            .map(|(module, info)| {
                hash_parts([
                    global.as_str(),
                    module.link_path.as_str(),
                    &info.rendered.full_hash,
                ])
            })
            .collect();

        let index_kept = self.current_page(old_pages, "index.html", &index_inputs);
        let pages_kept: Vec<Option<PageEntry>> = package
            .modules
            .par_iter()
            .zip(page_inputs.par_iter())
            .map(|(module, inputs)| self.current_page(old_pages, &module.link_path, inputs))
            .collect();
        let kept_count =
            usize::from(index_kept.is_some()) + pages_kept.iter().filter(|p| p.is_some()).count();
        let page_count = package.modules.len() + 1;

        // Serializing the package is a large part of rendering; skip it
        // when every page is current.
        let values = (index_kept.is_none() || pages_kept.iter().any(Option::is_none)).then(|| {
            (
                Value::from_serializable(package),
                Value::from_serializable(&nav_groups),
            )
        });

        // Render index page
        let index_entry = match index_kept {
            Some(entry) => entry,
            None => {
                let (package_value, nav_value) =
                    values.as_ref().expect("built when a page is not current");
                let mut index_context = self.base_context(package_value, nav_value, "./");
                index_context.insert("total_modules", &total_modules);
                index_context.insert("total_classes", &total_classes);
                index_context.insert("total_functions", &total_functions);
                if let Some(doc) = root_init.and_then(|m| m.docstring.as_ref()) {
                    index_context.insert("package_docstring", doc);
                }
                if let Some(root) = root_init
                    && (!root.classes.is_empty()
                        || !root.functions.is_empty()
                        || !root.variables.is_empty())
                {
                    index_context.insert("package_link_path", &root.link_path);
                }

                let rendered_index = self.tera.render("index.html", &index_context)?;
                self.write_page("index.html", &index_inputs, rendered_index)?
            }
        };

        // Render detailed individual module detail pages, in parallel.
        let page_entries: Vec<PageEntry> = package
            .modules
            .par_iter()
            .zip(page_inputs.par_iter())
            .zip(pages_kept.into_par_iter())
            .map(|((module, inputs), kept)| -> Result<PageEntry> {
                if let Some(entry) = kept {
                    return Ok(entry);
                }
                let (package_value, nav_value) =
                    values.as_ref().expect("built when a page is not current");
                let mut mod_context = self.base_context(
                    package_value,
                    nav_value,
                    &root_prefix_for(&module.link_path),
                );
                mod_context.insert("module", module);
                mod_context.insert(
                    "display_module_name",
                    display_module_name(module, &package.name),
                );

                let template_name = if module.is_document {
                    "document.html"
                } else {
                    "module.html"
                };

                let rendered_mod = self
                    .tera
                    .render(template_name, &mod_context)
                    .with_context(|| format!("Failed to render {}", module.source_path))?;
                self.write_page(&module.link_path, inputs, rendered_mod)
            })
            .collect::<Result<Vec<_>>>()?;

        status!("\x1b[36;1m[3/3]\x1b[0m Writing search index, navigation and theme assets...");

        // The search index is loaded by app.js on the first search, not by
        // every page: as JSON over HTTP, or through the .js copy on file://,
        // where browsers block fetch() of local files.
        let search_index = search_index_json(package).to_string();
        self.write_if_changed(
            "search-index.js",
            format!("const searchIndex = {};", search_index).as_bytes(),
        )?;
        self.write_if_changed("search-index.json", search_index.as_bytes())?;
        self.write_if_changed(
            "nav-data.js",
            format!("const navData = {};", nav_data_json(&nav_groups)).as_bytes(),
        )?;

        if !self.extra_css.is_empty() {
            fs::create_dir_all(Path::new(&self.output_dir).join("custom"))?;
        }
        for (site_path, contents) in &self.extra_css {
            self.write_if_changed(site_path, contents.as_bytes())?;
        }

        for name in STATIC_NAMES {
            let (body, _) = load_asset(self.template_dir.as_deref(), name)?;
            self.write_if_changed(name, body.as_bytes())?;
        }

        let unchanged = if kept_count > 0 {
            format!(" ({} unchanged)", kept_count)
        } else {
            String::new()
        };
        status!(
            "  \x1b[32m✔\x1b[0m Generated {} pages in {:.2?}{}",
            page_count,
            compile_start.elapsed(),
            unchanged
        );

        let mut pages = BTreeMap::new();
        pages.insert("index.html".to_string(), index_entry);
        for (module, page) in package.modules.iter().zip(page_entries) {
            pages.insert(module.link_path.clone(), page);
        }
        Ok(pages)
    }
}

/// Calls `f` on every docstring of a module: its own and those of its
/// classes, methods, attributes, functions and variables.
fn for_each_docstring(module: &mut PythonModule, f: &mut impl FnMut(&mut Option<String>)) {
    f(&mut module.docstring);
    for class in module.classes.iter_mut() {
        f(&mut class.docstring);
        class.functions.iter_mut().for_each(|x| f(&mut x.docstring));
        class.variables.iter_mut().for_each(|x| f(&mut x.docstring));
    }
    module
        .functions
        .iter_mut()
        .for_each(|x| f(&mut x.docstring));
    module
        .variables
        .iter_mut()
        .for_each(|x| f(&mut x.docstring));
}

/// The full hash of a rendered module. Rendering is determined by the
/// file (its path and bytes, which `source_hash` covers), the options and
/// theme (the cache key, part of every page's inputs) and what it looked
/// up (`deps`), so the hash is taken over those instead of serializing the
/// rendered module. Without a source hash the module itself is hashed.
fn rendered_hash(source_hash: &str, deps: &ModuleDeps, module: &PythonModule) -> Result<String> {
    if source_hash.is_empty() {
        return Ok(hash_bytes(&cache::encode(module)?));
    }
    Ok(hash_parts([
        source_hash,
        &hash_bytes(&cache::encode(deps)?),
    ]))
}

/// Hash of the part of a rendered module that other pages can show: all of
/// it with docstrings reduced to whether there is one. The docstrings are
/// blanked in place and put back, rather than cloning the module.
fn shared_hash(module: &mut PythonModule) -> Result<String> {
    let mut saved: Vec<String> = Vec::new();
    for_each_docstring(module, &mut |doc| {
        if let Some(text) = doc.as_mut() {
            saved.push(std::mem::take(text));
        }
    });
    let shared = cache::encode(&*module).map(|bytes| hash_bytes(&bytes));
    // Put the docstrings back, visiting them in the same order.
    let mut saved = saved.into_iter();
    for_each_docstring(module, &mut |doc| {
        if let Some(text) = doc.as_mut() {
            *text = saved.next().unwrap_or_default();
        }
    });
    shared
}

/// Splits a parsed module into its public part (None when the whole module
/// is private) and its hidden part, exactly as
/// [`PythonPackage::hide_private`] does for a package. The hidden part keeps
/// only names: the resolver uses nothing else, and it is never rendered.
fn split_private(
    module: PythonModule,
    include_private: bool,
) -> (Option<PythonModule>, Option<PythonModule>) {
    if include_private {
        return (Some(module), None);
    }
    let mut one = PythonPackage {
        name: String::new(),
        modules: vec![module],
        hidden: Vec::new(),
    };
    one.hide_private();
    let hidden = one.hidden.pop().map(|mut hidden| {
        for_each_docstring(&mut hidden, &mut |doc| *doc = None);
        hidden
    });
    (one.modules.pop(), hidden)
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
    Ok((bundled_asset(name)?.to_string(), false))
}

/// `py-doc theme eject`: writes the bundled theme, exactly as compiled into
/// this binary, plus a README into `dir`, as a starting point for
/// `--templates`. Unless `force` is set, nothing is written when any of the
/// files already exists. Returns the paths written.
pub fn eject_theme(dir: &Path, force: bool) -> Result<Vec<PathBuf>> {
    let files: Vec<(&str, &str)> = EMBEDDED_ASSETS
        .iter()
        .copied()
        .chain(std::iter::once(("README.md", THEME_README)))
        .collect();

    if !force {
        let existing: Vec<String> = files
            .iter()
            .map(|(name, _)| dir.join(name))
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
            .collect();
        if !existing.is_empty() {
            bail!(
                "these files already exist, so nothing was written (pass --force to overwrite them):\n  {}",
                existing.join("\n  ")
            );
        }
    }

    fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let mut written = Vec::with_capacity(files.len());
    for (name, body) in files {
        let path = dir.join(name);
        fs::write(&path, body).with_context(|| format!("Failed to write {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

fn bundled_asset(name: &str) -> Result<&'static str> {
    EMBEDDED_ASSETS
        .iter()
        .find(|(asset_name, _)| *asset_name == name)
        .map(|(_, body)| *body)
        .ok_or_else(|| anyhow!("Unknown bundled asset '{}'", name))
}

/// Every `*.html` file under the custom template directory, named by its
/// path relative to it with `/` separators (`partials/footer.html`). A file
/// with a bundled name replaces that template; the rest are extra templates
/// and partials for custom templates to extend or include. Hidden files and
/// folders are skipped.
fn custom_templates(dir: &Path) -> Result<Vec<(String, String)>> {
    fn walk(dir: &Path, prefix: &str, found: &mut Vec<(String, String)>) -> Result<()> {
        let entries = fs::read_dir(dir)
            .with_context(|| format!("Failed to read template directory {:?}", dir))?;
        for entry in entries {
            let path = entry?.path();
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if file_name.starts_with('.') {
                continue;
            }
            let name = format!("{prefix}{file_name}");
            if path.is_dir() {
                walk(&path, &format!("{name}/"), found)?;
            } else if file_name.ends_with(".html") {
                if name.starts_with(BUNDLED_PREFIX) {
                    eprintln!(
                        "  \x1b[33m⚠\x1b[0m {}: skipped, the '{}' folder is reserved for the bundled theme",
                        name, BUNDLED_PREFIX
                    );
                    continue;
                }
                let body = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read custom template {:?}", path))?;
                found.push((name, body));
            }
        }
        Ok(())
    }

    let mut found = Vec::new();
    if dir.is_dir() {
        walk(dir, "", &mut found)?;
    }
    found.sort();
    Ok(found)
}

/// Renders one line of Markdown without the surrounding paragraph, for
/// macro text such as `#pd-note: use **care**`.
fn render_inline(text: &str) -> String {
    let mut rendered = String::new();
    html::push_html(
        &mut rendered,
        Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH),
    );
    let rendered = rendered.trim();
    rendered
        .strip_prefix("<p>")
        .and_then(|inner| inner.strip_suffix("</p>"))
        .unwrap_or(rendered)
        .to_string()
}

/// True for a plain relative path with no "..", drive or root component.
fn is_safe_site_path(site_path: &str) -> bool {
    !site_path.contains('\\')
        && !site_path.contains(':')
        && Path::new(site_path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// Removes directories left empty by a deleted file, stopping at the site root.
fn remove_empty_parents(root: &Path, removed_file: &Path) {
    let mut current = removed_file.parent();
    while let Some(dir) = current {
        // remove_dir refuses non-empty directories, which ends the walk.
        if dir == root || !dir.starts_with(root) || fs::remove_dir(dir).is_err() {
            break;
        }
        current = dir.parent();
    }
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
/// Adds `"variables"` to a search entry, only when there are any, so
/// projects without documented variables keep the same payload.
fn with_variables(mut entry: serde_json::Value, variables: &[PythonVariable]) -> serde_json::Value {
    if !variables.is_empty() {
        entry["variables"] = variables
            .iter()
            .map(|v| serde_json::json!({ "name": v.name, "summary": v.summary }))
            .collect();
    }
    entry
}

/// Search entries for `classes` and, after each, its nested classes under
/// their qualified names ("Outer.Inner"), which are also their anchors.
fn class_entries(classes: &[PythonClass]) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    for c in classes {
        let name = if c.qualname.is_empty() {
            &c.name
        } else {
            &c.qualname
        };
        entries.push(with_variables(
            serde_json::json!({
                "name": name,
                "summary": c.summary,
                "functions": function_entries(&c.functions, true),
            }),
            &c.variables,
        ));
        entries.extend(class_entries(&c.classes));
    }
    entries
}

fn search_index_json(package: &PythonPackage) -> serde_json::Value {
    let modules: Vec<serde_json::Value> = package
        .modules
        .iter()
        .map(|m| {
            let entry = serde_json::json!({
                "name": if m.qualname.is_empty() { &package.name } else { &m.qualname },
                "link_path": m.link_path,
                "summary": m.summary,
                "classes": class_entries(&m.classes),
                "functions": function_entries(&m.functions, true),
            });
            with_variables(entry, &m.variables)
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
    fn manifest_paths_cannot_leave_the_output_folder() {
        assert!(is_safe_site_path("01_core/engine.html"));
        assert!(!is_safe_site_path("../secrets.txt"));
        assert!(!is_safe_site_path("/etc/passwd"));
        assert!(!is_safe_site_path("C:\\Windows\\win.ini"));
        assert!(!is_safe_site_path("a/../../b"));
    }

    #[test]
    fn macro_text_renders_inline_markdown() {
        assert_eq!(
            render_inline("use **care** with `x`"),
            "use <strong>care</strong> with <code>x</code>"
        );
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
