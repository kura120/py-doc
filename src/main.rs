mod generator;
mod highlight;
mod models;
mod parser;
mod resolver;
mod tui;

use anyhow::{Result, bail};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

use generator::SiteGenerator;
use models::PythonPackage;

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

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
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

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, Eq)]
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

#[derive(Parser, Debug, Clone)]
#[command(
    author,
    version,
    about = "Generates cargo-doc style documentation for Python projects",
    long_about = None
)]
pub struct Args {
    /// Path to the Python project (or a single .py file) to document
    #[arg(short, long)]
    pub src: String,

    /// Directory to write the generated site to
    #[arg(short, long)]
    pub out: String,

    /// Package name shown in the docs
    #[arg(short, long)]
    pub name: String,

    /// Version of your project, shown in the docs
    #[arg(long, default_value = "1.0.0")]
    pub doc_version: String,

    /// Directory with replacement theme files (sidebar.html, module.html, style.css, ...).
    /// Any file not found there falls back to the bundled theme.
    #[arg(short, long)]
    pub templates: Option<String>,

    /// Starting UI layout for the generated site
    #[arg(long, value_enum, default_value = "classic")]
    pub layout: LayoutKind,

    /// Starting color theme for the generated site
    #[arg(long, value_enum, default_value = "auto")]
    pub theme: ThemeKind,

    /// File or directory name (or path relative to --src) to skip. Repeatable.
    #[arg(short, long, value_name = "NAME")]
    pub exclude: Vec<String>,

    /// Base URL of a browsable copy of the sources (e.g. a GitHub tree URL),
    /// used to link each symbol to its definition
    #[arg(long, value_name = "URL")]
    pub source_url: Option<String>,

    /// Exit with an error if any file fails to parse, or an image or doc-link cannot be resolved
    #[arg(long)]
    pub strict: bool,
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
fn run_generation(args: &Args) -> Result<()> {
    let src_path = Path::new(&args.src);

    if !src_path.exists() {
        bail!("the source path '{}' does not exist", args.src);
    }

    println!(
        "\x1b[36;1m[1/3]\x1b[0m Scanning '{}' for Python modules...",
        args.src
    );

    let (root, py_files) = collect_python_files(src_path, &args.exclude);

    let mut package = PythonPackage {
        name: args.name.clone(),
        modules: Vec::new(),
    };

    let pb = ProgressBar::new(py_files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=>-"),
    );

    let mut parse_errors: Vec<String> = Vec::new();
    for path in &py_files {
        pb.set_message(
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );

        match parser::parse_file(path, &root) {
            Ok(module) => package.modules.push(module),
            Err(e) => {
                let shown = path.strip_prefix(&root).unwrap_or(path);
                parse_errors.push(format!("{}: {:#}", shown.display(), e));
            }
        }
        pb.inc(1);
    }
    pb.finish_and_clear();

    println!(
        "  \x1b[32m✔\x1b[0m Parsed {} of {} Python files.",
        package.modules.len(),
        py_files.len()
    );
    if !parse_errors.is_empty() {
        eprintln!(
            "  \x1b[33m⚠\x1b[0m {} file(s) failed to parse:",
            parse_errors.len()
        );
        for err in &parse_errors {
            eprintln!("    - {}", err);
        }
    }

    println!("\x1b[36;1m[2/3]\x1b[0m Rendering documentation...");
    let generator = SiteGenerator::new(
        &root,
        &args.out,
        &args.doc_version,
        args.templates.as_deref(),
        args.layout.as_str(),
        args.theme.as_baked(),
        args.source_url.as_deref(),
    )?;

    generator.generate(&mut package)?;

    println!(
        "\n\x1b[32;1m✔ Documentation generated at {}/index.html\x1b[0m",
        args.out.trim_end_matches(['/', '\\'])
    );

    let problems = parse_errors.len() + generator.warning_count();
    if args.strict && problems > 0 {
        bail!("{} problem(s) found and --strict is set", problems);
    }
    Ok(())
}

fn main() -> Result<()> {
    let raw_args: Vec<String> = std::env::args().collect();

    if raw_args.len() == 1 {
        // No flags at all: launch the interactive setup wizard — unless we're
        // not actually attached to a terminal (e.g. CI/non-interactive), in
        // which case a hanging full-screen TUI would just be a stuck build.
        use crossterm::tty::IsTty;
        if !std::io::stdout().is_tty() || !std::io::stdin().is_tty() {
            eprintln!("py-doc: no arguments given and no interactive terminal detected.");
            eprintln!(
                "Run 'py-doc --help' for available flags, or run interactively for the setup wizard."
            );
            std::process::exit(1);
        }

        return match tui::run_tui()? {
            Some(args) => run_generation(&args),
            None => {
                println!("Cancelled.");
                Ok(())
            }
        };
    }

    let args = Args::parse();
    run_generation(&args)
}
