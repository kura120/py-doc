mod models;
mod parser;
mod resolver;
mod generator;

use anyhow::{Result, Context};
use clap::Parser;
use std::path::Path;
use walkdir::WalkDir;

use models::PythonPackage;
use resolver::Resolver;
use generator::SiteGenerator;

#[derive(Parser, Debug)]
#[command(
    author, 
    about, 
    long_about = None, 
    disable_version_flag = true
)]
#[command(author, version, about = "Generates cargo-doc style documentation for Python projects")]
pub struct Args {
    #[arg(short, long)]
    pub src: String,

    #[arg(short, long)]
    pub out: String,

    #[arg(short, long)]
    pub name: String,

    // Add this field! We can give it a default value of "1.0.0"
    #[arg(short, long, default_value = "1.0.0")]
    pub version: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Scanning Python project in '{}'...", args.src);

    let mut package = PythonPackage {
        name: args.name.clone(),
        modules: Vec::new(),
    };

    let src_path = Path::new(&args.src);
    
    // Quick sanity check: does the folder even exist?
    if !src_path.exists() {
        eprintln!("Error: The directory '{}' does not exist!", args.src);
        std::process::exit(1);
    }

    let mut files_found = 0;

    for entry in WalkDir::new(src_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        
        // Debug: Log every file WalkDir visits
        if path.is_file() {
            println!("-> WalkDir found file: {:?}", path);
        }

        if path.extension().map_or(false, |ext| ext == "py") {
            files_found += 1;
            
            // Explicitly print matches/ignores
            if path.components().any(|c| c.as_os_str() == ".venv" || c.as_os_str() == "__pycache__") {
                println!("   [Ignored] In virtualenv/cache: {:?}", path);
                continue;
            }

            println!("   [Attempting Parse] {:?}", path);
            // Pass both 'path' and 'src_path' so the parser knows the root context
            match parser::parse_file(path, src_path) {
                Ok(module) => {
                    println!("   [Parsed Successfully] Module: {}", module.name);
                    package.modules.push(module);
                }
                Err(e) => eprintln!("   [Parse Error] Failed to parse {}: {}", path.display(), e),
            }
        }
    }

    println!("Total .py files found: {}", files_found);
    println!("Total modules successfully compiled: {}", package.modules.len());

    let mut resolver = Resolver::new();
    resolver.build_index(&package);

    let generator = SiteGenerator::new(&args.src, &args.out, &args.version)
        .context("Failed to initialize documentation generator")?;
        
    generator.generate(&mut package, &resolver)?;

    println!("Documentation successfully generated at {}/index.html", args.out);
    Ok(())
}