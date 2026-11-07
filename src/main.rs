/// Progress output: stdout normally, stderr while a JSON coverage report
/// owns stdout. Defined before the modules so they can use it.
macro_rules! status {
    ($($arg:tt)*) => {
        if crate::status_to_stderr() {
            eprintln!($($arg)*)
        } else {
            println!($($arg)*)
        }
    };
}

mod cache;
mod config;
mod coverage;
mod generator;
mod highlight;
mod models;
mod output;
mod parser;
mod resolver;
mod serve;
mod tui;

use anyhow::{Result, bail};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::{DirEntry, WalkDir};

use cache::Cache;
use config::Config;
use generator::{ModuleSource, RunCache, SiteGenerator};
use models::PythonPackage;

static STATUS_TO_STDERR: AtomicBool = AtomicBool::new(false);

/// Read by `status!`.
fn status_to_stderr() -> bool {
    STATUS_TO_STDERR.load(Ordering::Relaxed)
}

/// Directory names that are never documented: environments, caches and
/// build output. Hidden directories (".git", ".venv", ...) are skipped too.
const DEFAULT_EXCLUDES: [&str; 8] = [
    "__pycache__",
    "venv",
    "node_modules",
    "site-packages",
    "build",
    "dist",
    "htmlcov",
    "__pypackages__",
];

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutKind {
    Classic,
    Minimal,
    Modern,
}

impl LayoutKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            LayoutKind::Classic => "classic",
            LayoutKind::Minimal => "minimal",
            LayoutKind::Modern => "modern",
        }
    }
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    /// Follow the reader's system light/dark setting
    Auto,
    Dark,
    Light,
    Slate,
}

impl ThemeKind {
    /// The value baked into the site; None lets the reader's system decide.
    pub fn as_baked(&self) -> Option<&'static str> {
        match self {
            ThemeKind::Auto => None,
            ThemeKind::Dark => Some("dark"),
            ThemeKind::Light => Some("light"),
            ThemeKind::Slate => Some("slate"),
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageFormat {
    /// A table, worst-covered module first
    Text,
    /// Machine-readable; the schema is in the README
    Json,
}

/// What a run writes to `--out`. Each format is one arm of the match in
/// `run_generation`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// The HTML site
    Html,
    /// `api.json`: the parsed API, with raw and rendered docstrings
    Json,
    /// Not supported yet (planned, #20)
    #[value(hide = true)]
    Markdown,
}

/// The command line: the build options, or a subcommand. `serve` takes
/// the same build options, so they are only on the subcommand's side of a
/// command line (`py-doc serve --src x`, not `py-doc --src x serve`).
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Generates cargo-doc style documentation for Python projects",
    long_about = None,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub args: Args,
}

/// Every option, as given on the command line or in `[tool.py-doc]`.
///
/// This one struct is both the clap arguments and the serde shape of the
/// `[tool.py-doc]` table, so a field's flag (`--doc-version`) and its key
/// (`doc-version`) come from the same name. Every field is optional here;
/// `Config::resolve` merges the layers and applies the defaults. To add an
/// option: add a field here, then add it to `Config` and its `resolve`.
#[derive(clap::Args, Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Args {
    /// Path to the Python project (or a single .py file) to document
    #[arg(short, long)]
    pub src: Option<String>,

    /// Directory to write the generated site to
    #[arg(short, long)]
    pub out: Option<String>,

    /// Package name shown in the docs [default: `[project] name` in pyproject.toml]
    #[arg(short, long)]
    pub name: Option<String>,

    /// Version of your project, shown in the docs
    /// [default: `[project] version` in pyproject.toml, else 1.0.0]
    #[arg(long)]
    pub doc_version: Option<String>,

    /// Directory with replacement theme files (sidebar.html, module.html, style.css, ...).
    /// Any file not found there falls back to the bundled theme.
    #[arg(short, long)]
    pub templates: Option<String>,

    /// Extra stylesheet linked after the bundled style.css on every page,
    /// to override the theme's CSS custom properties or add rules. Repeatable.
    #[arg(long, value_name = "FILE")]
    #[serde(default)]
    pub css: Vec<String>,

    /// Starting UI layout for the generated site [default: classic]
    #[arg(long, value_enum)]
    pub layout: Option<LayoutKind>,

    /// Starting color theme for the generated site [default: auto]
    #[arg(long, value_enum)]
    pub theme: Option<ThemeKind>,

    /// File or directory name (or path relative to --src) to skip. Repeatable.
    #[arg(short, long, value_name = "NAME")]
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Base URL of a browsable copy of the sources (e.g. a GitHub tree URL),
    /// used to link each symbol to its definition
    #[arg(long, value_name = "URL")]
    pub source_url: Option<String>,

    /// Exit with an error if any file fails to parse, or an image or doc-link cannot be resolved
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub strict: Option<bool>,

    /// Remove files that an earlier run wrote to --out and this run no longer
    /// produces. Files py-doc did not write are never touched.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub clean: Option<bool>,

    /// Ignore the build cache in --out and render every page. A new cache
    /// is still written for the next run.
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub no_cache: Option<bool>,

    /// Also document private names (`_name`), private modules and names left
    /// out of `__all__`
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub include_private: Option<bool>,

    /// What to write to --out: `html` (the site) and/or `json` (api.json).
    /// Repeatable [default: html]
    #[arg(long, value_enum, value_name = "FORMAT")]
    #[serde(default)]
    pub format: Vec<OutputFormat>,

    /// Print a documentation coverage report. On the command line without
    /// --out, only the report is made and no site is written
    #[arg(long, value_name = "BOOL", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub coverage: Option<bool>,

    /// Exit with an error when documentation coverage is below PERCENT
    /// (0-100). Turns on --coverage
    #[arg(long, value_name = "PERCENT")]
    pub fail_under: Option<f64>,

    /// Format of the coverage report [default: text]
    #[arg(long, value_enum)]
    pub coverage_format: Option<CoverageFormat>,

    /// pyproject.toml to read `[tool.py-doc]` and `[project]` from. By default
    /// it is looked for in --src (or the current directory) and its parents,
    /// stopping at the repository root.
    #[arg(long, value_name = "FILE")]
    #[serde(skip)]
    pub config: Option<String>,
}

/// Subcommands. Without one, py-doc generates docs from the flags (or
/// starts the setup wizard when there are none).
#[derive(clap::Subcommand, Debug, Clone, PartialEq)]
pub enum Command {
    /// Work with the bundled theme
    #[command(subcommand)]
    Theme(ThemeCommand),
    /// Build the docs, serve them on 127.0.0.1 and rebuild on every change
    /// to the sources, templates or CSS, reloading open pages. Takes every
    /// build option, and reads `[tool.py-doc]` like a normal build.
    Serve(Box<ServeArgs>),
}

#[derive(clap::Args, Debug, Clone, PartialEq)]
pub struct ServeArgs {
    /// Port to listen on; the next free one is used if it is taken.
    /// 0 picks any free port
    #[arg(long, default_value_t = 8000)]
    pub port: u16,

    /// Open the site in the default browser once it is served
    #[arg(long)]
    pub open: bool,

    #[command(flatten)]
    pub build: Args,
}

#[derive(clap::Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum ThemeCommand {
    /// Copy the bundled templates, CSS and JavaScript into DIR, as a
    /// starting point for --templates
    Eject {
        /// Folder to write the theme to
        #[arg(default_value = "py-doc-theme")]
        dir: String,
        /// Overwrite files that already exist
        #[arg(long)]
        force: bool,
    },
}

fn run_command(command: Command) -> Result<()> {
    match command {
        Command::Serve(serve_args) => {
            let pyproject = config::PyProject::load(&serve_args.build)?;
            let ServeArgs { port, open, build } = *serve_args;
            let config = Config::resolve(build, pyproject.as_ref())?;
            serve::run(config, port, open)
        }
        Command::Theme(ThemeCommand::Eject { dir, force }) => {
            let written = generator::eject_theme(Path::new(&dir), force)?;
            println!(
                "\x1b[32m✔\x1b[0m Wrote the bundled theme ({} files) to {}",
                written.len(),
                dir.trim_end_matches(['/', '\\'])
            );
            println!("  Use it with: py-doc --templates {} ...", dir);
            println!("  Delete the files you don't change; they fall back to the bundled theme.");
            Ok(())
        }
    }
}

fn is_excluded(entry: &DirEntry, root: &Path, user_excludes: &[String]) -> bool {
    let name = entry.file_name().to_string_lossy();

    if entry.file_type().is_dir()
        && (name.starts_with('.')
            || name.ends_with(".egg-info")
            || DEFAULT_EXCLUDES.contains(&name.as_ref()))
    {
        return true;
    }

    let relative = entry
        .path()
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    user_excludes.iter().any(|pattern| {
        let pattern = pattern.replace('\\', "/");
        let pattern = pattern.trim_matches('/');
        pattern == name || pattern == relative
    })
}

/// Returns the root that module paths are relative to, and the files to document.
fn collect_python_files(src_path: &Path, user_excludes: &[String]) -> (PathBuf, Vec<PathBuf>) {
    if src_path.is_file() {
        let root = match src_path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        return (root, vec![src_path.to_path_buf()]);
    }

    let mut files: Vec<PathBuf> = WalkDir::new(src_path)
        .into_iter()
        // Never test the root itself: only names below it decide exclusion.
        .filter_entry(|entry| entry.depth() == 0 || !is_excluded(entry, src_path, user_excludes))
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            entry.file_type().is_file()
                && path.extension().is_some_and(|ext| ext == "py")
                && path.file_name().is_none_or(|n| n != "__main__.py")
        })
        .map(|entry| entry.into_path())
        .collect();
    files.sort();

    (src_path.to_path_buf(), files)
}

/// Shared generation pipeline used by both the flag-driven CLI path and the
/// interactive TUI path, so the two never drift out of sync.
///
/// With `args.out` None (a check-only coverage run) it stops after the
/// coverage report and writes nothing.
fn run_generation(args: &Config) -> Result<()> {
    build(args, false)
}

/// A `serve` build that stopped before writing anything.
#[derive(Debug)]
pub(crate) struct NotRebuilt(usize);

impl std::fmt::Display for NotRebuilt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} file(s) failed to parse; the site was not rebuilt",
            self.0
        )
    }
}

impl std::error::Error for NotRebuilt {}

/// [`run_generation`], for `serve` when `serving`: then a file that fails
/// to parse stops the build before anything is written, so the server
/// keeps the last good site instead of one with that module missing.
pub(crate) fn build(args: &Config, serving: bool) -> Result<()> {
    let src_path = Path::new(&args.src);

    if !src_path.exists() {
        bail!("the source path '{}' does not exist", args.src);
    }

    // A JSON report owns stdout, so progress moves to stderr to keep it parseable.
    let json_report = args
        .coverage
        .as_ref()
        .is_some_and(|c| c.format == CoverageFormat::Json);
    STATUS_TO_STDERR.store(json_report, Ordering::Relaxed);

    let steps = if args.out.is_some() { 3 } else { 2 };
    status!(
        "\x1b[36;1m[1/{steps}]\x1b[0m Scanning '{}' for Python modules...",
        args.src
    );

    let (root, py_files) = collect_python_files(src_path, &args.exclude);

    // The generator comes first: the cache key includes its theme files.
    // A coverage report needs every module parsed, so it never reads the
    // cache (a run that also builds the site still writes one), and a
    // check-only run has no output folder and no cache at all. Nor does
    // `--format json` read it: api.json carries raw docstrings, which a
    // cached (already rendered) module no longer has, so that run is a full
    // build. With `html` too, it still writes the cache.
    let generator = match &args.out {
        Some(out) => Some(
            SiteGenerator::new(
                &root,
                out,
                &args.doc_version,
                args.templates.as_deref(),
                args.layout.as_str(),
                args.theme.as_baked(),
                args.source_url.as_deref(),
            )?
            .with_extra_css(&args.css)?,
        ),
        None => None,
    };
    let cache_key = generator
        .as_ref()
        .map(|g| cache::cache_key(&args.cache_settings(), g.theme_fingerprint()));
    let old_cache = match (&args.out, &cache_key) {
        (Some(out), Some(key))
            if !args.no_cache
                && args.coverage.is_none()
                && !args.formats.contains(&OutputFormat::Json) =>
        {
            Cache::load(Path::new(out), key)
        }
        _ => None,
    };

    let pb = ProgressBar::new(py_files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=>-"),
    );

    // Files are parsed in parallel; collecting keeps them in sorted order.
    // A file the cache holds with the same content is not parsed at all.
    // Without a cache nothing is hashed, so an uncached build costs the same
    // as before.
    let results: Vec<_> = py_files
        .par_iter()
        .map(|path| {
            let mut source_hash = String::new();
            let mut cached = false;
            if cache_key.is_some() {
                let source_path = cache::source_path_for(path, &root);
                // An unreadable file gets no hash, misses the cache and fails to parse below.
                if let Ok(bytes) = std::fs::read(path) {
                    source_hash = cache::hash_source(&source_path, &bytes);
                }
                cached = !source_hash.is_empty()
                    && old_cache
                        .as_ref()
                        .is_some_and(|c| c.has_source(&source_path, &source_hash));
            }
            let result = if cached {
                Ok(None)
            } else {
                parser::parse_file(path, &root).map(Some)
            };
            pb.inc(1);
            (path, source_hash, result)
        })
        .collect();

    let mut sources = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();
    for (path, source_hash, result) in results {
        match result {
            Ok(parsed) => sources.push(ModuleSource {
                path: path.clone(),
                source_hash,
                parsed,
            }),
            Err(e) => {
                let shown = path.strip_prefix(&root).unwrap_or(path);
                parse_errors.push(format!("{}: {:#}", shown.display(), e));
            }
        }
    }
    pb.finish_and_clear();

    let unchanged = sources.iter().filter(|s| s.parsed.is_none()).count();
    status!(
        "  \x1b[32m✔\x1b[0m Parsed {} of {} Python files{}.",
        sources.len(),
        py_files.len(),
        if unchanged > 0 {
            format!(" ({} unchanged)", unchanged)
        } else {
            String::new()
        }
    );
    if !parse_errors.is_empty() {
        eprintln!(
            "  \x1b[33m⚠\x1b[0m {} file(s) failed to parse:",
            parse_errors.len()
        );
        for err in &parse_errors {
            eprintln!("    - {}", err);
        }
        if serving {
            return Err(NotRebuilt(parse_errors.len()).into());
        }
    }

    // Measured before rendering, which replaces docstrings with HTML. The
    // cache is not read in coverage runs, so every module is parsed here.
    let report = args.coverage.as_ref().map(|_| {
        let mut package = PythonPackage {
            name: args.name.clone(),
            modules: sources.iter().filter_map(|s| s.parsed.clone()).collect(),
            hidden: Vec::new(),
        };
        if !args.include_private {
            package.hide_private();
        }
        coverage::Report::of(&package, args.include_private)
    });

    let mut problems = parse_errors.len();
    match (&args.out, generator, cache_key) {
        (Some(out), Some(generator), Some(key)) => {
            status!("\x1b[36;1m[2/3]\x1b[0m Rendering documentation...");
            generator.announce_templates();

            let prepared = generator.render(
                &args.name,
                sources,
                &root,
                RunCache {
                    previous: old_cache,
                    key,
                },
                args.include_private,
            )?;

            let out_shown = out.trim_end_matches(['/', '\\']);
            let mut written = Vec::new();
            for format in &args.formats {
                match format {
                    OutputFormat::Html => {
                        let pages = generator.write_html(&prepared)?;
                        generator.save_cache(&prepared, &pages);
                        written.push(format!("{out_shown}/index.html"));
                    }
                    OutputFormat::Json => {
                        if !args.formats.contains(&OutputFormat::Html) {
                            status!(
                                "\x1b[36;1m[3/3]\x1b[0m Writing {}...",
                                output::json::FILE_NAME
                            );
                        }
                        let api = output::json::to_string(
                            &prepared.package,
                            &args.doc_version,
                            args.include_private,
                        );
                        generator.write_file(output::json::FILE_NAME, &api)?;
                        written.push(format!("{out_shown}/{}", output::json::FILE_NAME));
                    }
                    // Rejected by Config::resolve until #20 implements it.
                    OutputFormat::Markdown => bail!("format `markdown` is not supported yet"),
                }
            }
            generator.finish(args.clean)?;

            status!(
                "\n\x1b[32;1m✔ Documentation generated at {}\x1b[0m",
                written.join(" and ")
            );
            problems += generator.warning_count();
        }
        _ => {
            status!(
                "\x1b[36;1m[2/2]\x1b[0m Measuring documentation coverage (no site is written)..."
            );
        }
    }

    let mut failures = Vec::new();
    if let (Some(report), Some(settings)) = (&report, &args.coverage) {
        match settings.format {
            CoverageFormat::Json => println!("{}", report.to_json(settings.fail_under)),
            CoverageFormat::Text => println!("\n{}", report.to_text().trim_end()),
        }
        if let Some(threshold) = settings.fail_under
            && !report.passes(Some(threshold))
        {
            // Floored, so a value just under the threshold never prints as equal to it.
            let actual = (report.total.percent() * 100.0).floor() / 100.0;
            failures.push(format!(
                "documentation coverage is {actual:.2}%, below --fail-under {threshold}%"
            ));
        }
    }
    if args.strict && problems > 0 {
        failures.push(format!("{problems} problem(s) found and --strict is set"));
    }
    if !failures.is_empty() {
        bail!("{}", failures.join("; "));
    }
    Ok(())
}

fn main() -> Result<()> {
    let no_args = std::env::args().len() == 1;
    let Cli { command, args: cli } = Cli::parse();
    if let Some(command) = command {
        return run_command(command);
    }
    let pyproject = config::PyProject::load(&cli)?;

    // No flags and no [tool.py-doc]: launch the interactive setup wizard,
    // unless we're not attached to a terminal (e.g. CI), in which case a
    // hanging full-screen TUI would just be a stuck build.
    if no_args && !pyproject.as_ref().is_some_and(|p| p.is_configured()) {
        use crossterm::tty::IsTty;
        if !std::io::stdout().is_tty() || !std::io::stdin().is_tty() {
            eprintln!("py-doc: no arguments given and no interactive terminal detected.");
            eprintln!(
                "Run 'py-doc --help' for available flags, add a [tool.py-doc] table to pyproject.toml, \
                 or run interactively for the setup wizard."
            );
            std::process::exit(1);
        }

        let prefill = config::file_layer(pyproject.as_ref());
        return match tui::run_tui(&prefill)? {
            Some(config) => run_generation(&config),
            None => {
                println!("Cancelled.");
                Ok(())
            }
        };
    }

    let config = Config::resolve(cli, pyproject.as_ref())?;
    run_generation(&config)
}
