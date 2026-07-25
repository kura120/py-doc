mod models;
mod parser;
mod resolver;
mod generator;
mod tui;

use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use walkdir::WalkDir;

use models::PythonPackage;
use resolver::Resolver;
use generator::SiteGenerator;

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
    Dark,
    Light,
    Slate,
}

impl ThemeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeKind::Dark => "dark",
            ThemeKind::Light => "light",
            ThemeKind::Slate => "slate",
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    author,
    version,
    about = "Generates cargo-doc style documentation for Python projects",
    long_about = None,
    disable_version_flag = true
)]
pub struct Args {
    /// Path to the Python project to document
    #[arg(short, long)]
    pub src: String,

    /// Directory to write the generated site to
    #[arg(short, long)]
    pub out: String,

    /// Package name shown in the docs
    #[arg(short, long)]
    pub name: String,

    /// Version string shown in the docs
    #[arg(short, long, default_value = "1.0.0")]
    pub version: String,

    /// Optional path to local HTML templates (sidebar.html, module.html, etc.)
    #[arg(short, long)]
    pub templates: Option<String>,

    /// Starting UI layout for the generated site
    #[arg(long, value_enum, default_value = "classic")]
    pub layout: LayoutKind,

    /// Starting color theme for the generated site (only visible under the Classic layout)
    #[arg(long, value_enum, default_value = "dark")]
    pub theme: ThemeKind,
}

/// Shared generation pipeline used by both the flag-driven CLI path and the
/// interactive TUI path, so the two never drift out of sync.
fn run_generation(args: &Args) -> Result<()> {
    let src_path = Path::new(&args.src);

    if !src_path.exists() {
        eprintln!("Error: The directory '{}' does not exist!", args.src);
        std::process::exit(1);
    }

    println!("\x1b[36;1m[1/3]\x1b[0m Scanning '{}' for Python modules...", args.src);

    let mut package = PythonPackage {
        name: args.name.clone(),
        modules: Vec::new(),
    };

    let py_files: Vec<_> = WalkDir::new(src_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| {
            let path = entry.path();
            path.is_file()
                && path.extension().map_or(false, |ext| ext == "py")
                && path.file_name().map_or(true, |n| n != "__main__.py")
                && !path
                    .components()
                    .any(|c| c.as_os_str() == ".venv" || c.as_os_str() == "__pycache__")
        })
        .collect();

    let pb = ProgressBar::new(py_files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} [{bar:30.cyan/blue}] {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=>-"),
    );

    let mut parse_errors: Vec<String> = Vec::new();
    for entry in &py_files {
        let path = entry.path();
        pb.set_message(
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );

        match parser::parse_file(path, src_path) {
            Ok(module) => package.modules.push(module),
            Err(e) => parse_errors.push(format!("{}: {}", path.display(), e)),
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
        eprintln!("  \x1b[33m⚠\x1b[0m {} file(s) failed to parse:", parse_errors.len());
        for err in &parse_errors {
            eprintln!("    - {}", err);
        }
    }

    let mut resolver = Resolver::new();
    resolver.build_index(&package);

    let template_dir = args.templates.as_deref();

    println!("\x1b[36;1m[2/3]\x1b[0m Loading theme assets...");
    let generator = SiteGenerator::new(
        &args.src,
        &args.out,
        &args.version,
        template_dir,
        args.layout.as_str(),
        args.theme.as_str(),
    )?;

    generator.generate(&mut package, &resolver)?;

    println!(
        "\n\x1b[32;1m✔ Documentation successfully generated at {}/index.html\x1b[0m",
        args.out
    );
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
            eprintln!("Run 'py-doc --help' for available flags, or run interactively for the setup wizard.");
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