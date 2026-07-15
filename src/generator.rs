use anyhow::{Result, Context as AnyhowContext};
use tera::{Tera, Context as TeraContext}; // Explicitly aliased to avoid any compiler namespace conflicts
use std::fs;
use std::path::Path;
use std::collections::HashSet;
use pulldown_cmark::{Parser, html, Event, Tag, CowStr};

use crate::models::{PythonPackage, PythonModule};
use crate::resolver::Resolver;

/// Serialization helper to pass grouped folders and their modules into templates
#[derive(Debug, Clone, serde::Serialize)]
pub struct NavGroup {
    pub name: Option<String>, // Some("guides") or None for root level
    pub modules: Vec<PythonModule>,
}

/// Helper to dynamically strip Python indentation from docstrings
fn clean_docstring(docstring: &str) -> String {
    let lines: Vec<&str> = docstring.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    // Find the minimum indentation level of non-empty lines (excluding the first line)
    let mut min_indent = usize::MAX;
    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent < min_indent {
            min_indent = indent;
        }
    }

    let min_indent = if min_indent == usize::MAX { 0 } else { min_indent };

    // Rebuild the string, removing up to min_indent spaces from each line
    let mut cleaned = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            cleaned.push_str(line);
        } else if line.trim().is_empty() {
            // Keep empty lines clean
        } else {
            let stripped = if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim_start()
            };
            cleaned.push_str(stripped);
        }
        cleaned.push_str("\n");
    }
    cleaned
}

/// Helper to parse and strip the `#pd-z-index` macro from a docstring
fn extract_z_index_and_clean(docstring: &str) -> (i32, String) {
    let mut z_index = i32::MAX; // Default to bottom
    let mut cleaned_lines = Vec::new();

    for line in docstring.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#pd-z-index:") {
            let val_str = trimmed.trim_start_matches("#pd-z-index:").trim();
            if let Ok(parsed_val) = val_str.parse::<i32>() {
                z_index = parsed_val;
            }
            continue; // Skip appending this macro line to final markdown
        }
        cleaned_lines.push(line);
    }

    (z_index, cleaned_lines.join("\n"))
}

enum AlertType {
    Error,
    Note,
    Warning,
}

impl AlertType {
    /// Return standard CSS classes and titles for systematic styling
    fn to_html(&self, content: &str) -> String {
        let (class_name, icon, title) = match self {
            AlertType::Error => ("alert-error", "⚠️", "Error"),
            AlertType::Note => ("alert-note", "ℹ️", "Note"),
            AlertType::Warning => ("alert-warning", "⚡", "Warning"),
        };

        format!(
            "\n\n<div class=\"alert {}\">\n  <strong>{} {}:</strong> {}\n</div>\n\n",
            class_name, icon, title, content
        )
    }
}

/// Systematic Representation of our custom Markdown Macros
enum DocMacro<'a> {
    Image { path: &'a str },
    Note { text: &'a str },
    Warning { text: &'a str },
    DocLink { target: &'a str },
    CodeBlockStart { language: String },
}

impl<'a> DocMacro<'a> {
    /// Attempts to parse a single line into a known `DocMacro`
    fn parse(line: &'a str) -> Option<Self> {
        let trimmed = line.trim();
        
        if trimmed.starts_with("#pd-image:") {
            Some(DocMacro::Image {
                path: trimmed.trim_start_matches("#pd-image:").trim(),
            })
        } else if trimmed.starts_with("#pd-note:") {
            Some(DocMacro::Note {
                text: trimmed.trim_start_matches("#pd-note:").trim(),
            })
        } else if trimmed.starts_with("#pd-warning:") {
            Some(DocMacro::Warning {
                text: trimmed.trim_start_matches("#pd-warning:").trim(),
            })
        } else if trimmed.starts_with("#pd-doc-link:") {
            Some(DocMacro::DocLink {
                target: trimmed.trim_start_matches("#pd-doc-link:").trim(),
            })
        } else if trimmed.starts_with("#pd-code") {
            let lang_part = trimmed.trim_start_matches("#pd-code").trim();
            let language = lang_part.trim_matches('`').to_string();
            Some(DocMacro::CodeBlockStart { language })
        } else {
            None
        }
    }

    /// Renders the individual macro into its markdown/HTML representation
    fn render(self, generator: &SiteGenerator) -> String {
        match self {
            DocMacro::Image { path } => {
                match generator.process_image_path(path) {
                    Ok(rel_path) => format!("\n\n![Smart Reference]({})\n\n", rel_path),
                    Err(e) => {
                        eprintln!("\x1b[31;1mSmart Image Error:\x1b[0m {}", e);
                        AlertType::Error.to_html(&e.to_string())
                    }
                }
            }
            DocMacro::Note { text } => AlertType::Note.to_html(text),
            DocMacro::Warning { text } => AlertType::Warning.to_html(text),
            DocMacro::DocLink { target } => {
                let link_html = if let Some((mod_name, sym_name)) = target.split_once('.') {
                    format!("<a href=\"{}.html#fn.{}\" class=\"doc-internal-link\"><code>{}</code></a>", mod_name, sym_name, target)
                } else {
                    format!("<a href=\"{}.html\" class=\"doc-internal-link\"><code>{}</code></a>", target, target)
                };
                format!("\n\n<div class=\"doc-link-container\">🔗 See Reference: {}</div>\n\n", link_html)
            }
            DocMacro::CodeBlockStart { .. } => String::new(),
        }
    }
}

pub struct SiteGenerator {
    pub tera: Tera,
    pub src_dir: String,
    pub output_dir: String,
    pub version: String,
}

impl SiteGenerator {
    pub fn new(src_dir: &str, output_dir: &str, version: &str) -> Result<Self> {
        let mut tera = Tera::default();

        let index_tmpl = include_str!("../templates/index.html");
        let module_tmpl = include_str!("../templates/module.html");
        let document_tmpl = include_str!("../templates/document.html");
        let sidebar_tmpl = include_str!("../templates/sidebar.html"); // Load component

        tera.add_raw_template("index.html", index_tmpl)
            .with_context(|| "Failed to load index.html template assets")?;
        tera.add_raw_template("module.html", module_tmpl)
            .with_context(|| "Failed to load module.html template assets")?;
        tera.add_raw_template("document.html", document_tmpl)
            .with_context(|| "Failed to load document.html template assets")?;
        tera.add_raw_template("sidebar.html", sidebar_tmpl) // Register component
            .with_context(|| "Failed to load sidebar.html component")?;

        Ok(Self {
            tera,
            src_dir: src_dir.to_string(),
            output_dir: output_dir.to_string(),
            version: version.to_string(),
        })
    }

    fn process_image_path(&self, img_path: &str) -> Result<String> {
        let src_image_path = Path::new(&self.src_dir).join(img_path);

        if !src_image_path.exists() {
            return Err(anyhow::anyhow!(
                "Image file not found at local path: '{}' (resolved as '{}')",
                img_path,
                src_image_path.display()
            ));
        }

        // Create the output asset folder
        let dest_dir = Path::new(&self.output_dir).join("assets");
        fs::create_dir_all(&dest_dir)?;

        // Keep the original filename
        let file_name = src_image_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid image path filename"))?;
        let dest_image_path = dest_dir.join(file_name);

        // Copy the file
        fs::copy(&src_image_path, &dest_image_path)?;

        Ok(format!("assets/{}", file_name.to_string_lossy()))
    }

    /// Custom markdown processor that hooks into image rendering and extracts #pd references
    fn render_markdown(&self, markdown: &str, _resolver: &Resolver) -> String {
        // 1. Strip Python's block indentation
        let dedented_md = clean_docstring(markdown);

        // 2. Strip the `#pd-write` directive line so it doesn't leak into the final HTML
        let cleaned_md = if dedented_md.trim_start().starts_with("#pd-write") {
            dedented_md
                .lines()
                .filter(|line| !line.trim().starts_with("#pd-write"))
                .collect::<Vec<&str>>()
                .join("\n")
        } else {
            dedented_md
        };

        let mut processed_md = String::new();
        let mut in_custom_code = false;
        let mut code_language = String::new();
        let mut code_accumulator = String::new();

        for line in cleaned_md.lines() {
            let trimmed = line.trim();

            // Multi-line Code Block Hook (#pd-code ```lang)
            if in_custom_code {
                if trimmed == "```" {
                    in_custom_code = false;
                    processed_md.push_str(&format!(
                        "\n\n```{}\n{}\n```\n\n",
                        code_language,
                        code_accumulator
                    ));
                } else {
                    code_accumulator.push_str(line);
                    code_accumulator.push_str("\n");
                }
                continue;
            }

            // Parse macro rules
            if let Some(mac) = DocMacro::parse(line) {
                match mac {
                    DocMacro::CodeBlockStart { language } => {
                        in_custom_code = true;
                        code_language = language;
                        code_accumulator.clear();
                    }
                    other_macro => {
                        processed_md.push_str(&other_macro.render(self));
                    }
                }
                continue;
            }

            processed_md.push_str(line);
            processed_md.push_str("\n");
        }

        // Parse markdown with pulldown-cmark
        let parser = Parser::new(&processed_md);
        
        // Intercept standard Markdown image tags to validate/copy those as well
        let mapped_events = parser.map(|event| match event {
            Event::Start(Tag::Image(link_type, dest_url, title)) => {
                if !dest_url.starts_with("http://") && !dest_url.starts_with("https://") {
                    match self.process_image_path(&dest_url) {
                        Ok(new_path) => Event::Start(Tag::Image(
                            link_type,
                            CowStr::Boxed(new_path.into_boxed_str()),
                            title,
                        )),
                        Err(e) => {
                            eprintln!("\x1b[31;1mSmart Image Error in Markdown Tag:\x1b[0m {}", e);
                            Event::Text(CowStr::Boxed(format!("[⚠️ Image Error: {}]", e).into_boxed_str()))
                        }
                    }
                } else {
                    Event::Start(Tag::Image(link_type, dest_url, title))
                }
            }
            other => other,
        });

        let mut html_output = String::new();
        html::push_html(&mut html_output, mapped_events);
        html_output
    }

    pub fn generate(&self, package: &mut PythonPackage, resolver: &Resolver) -> Result<()> {
        fs::create_dir_all(&self.output_dir)?;

        let mut doc_only_modules = HashSet::new();

        // Parse z-index first, strip it, and apply markdown compilation passes
        for module in &mut package.modules {
            let raw_doc = module.docstring.clone().unwrap_or_default();
            let (z_val, cleaned_doc) = extract_z_index_and_clean(&raw_doc);
            
            // Set parsed z_index directly to our module representation
            module.z_index = z_val;
            
            if module.docstring.is_some() {
                module.docstring = Some(cleaned_doc);
            }

            let has_code = !module.classes.is_empty() || !module.functions.is_empty();

            if let Some(ref doc) = module.docstring {
                // Look for `#pd-write` macro to see if this module acts as a pure document
                if doc.trim_start().starts_with("#pd-write") {
                    if !has_code {
                        doc_only_modules.insert(module.name.clone());
                    } else {
                        eprintln!(
                            "\x1b[33mWarning:\x1b[0m Module '{}' has '#pd-write' macro but contains code execution items. Defaulting to standard layout.",
                            module.name
                        );
                    }
                }
                module.docstring = Some(self.render_markdown(doc, resolver));
            }
            for class in &mut module.classes {
                if let Some(ref doc) = class.docstring {
                    class.docstring = Some(self.render_markdown(doc, resolver));
                }
                for method in &mut class.functions {
                    if let Some(ref doc) = method.docstring {
                        method.docstring = Some(self.render_markdown(doc, resolver));
                    }
                }
            }
            for func in &mut module.functions {
                if let Some(ref doc) = func.docstring {
                    func.docstring = Some(self.render_markdown(doc, resolver));
                }
            }
        }

        // Sort modules first: prioritizes explicit z-index (ascending), then alphabetically
        package.modules.sort_by(|a, b| {
            if a.z_index == b.z_index {
                a.name.cmp(&b.name)
            } else {
                a.z_index.cmp(&b.z_index)
            }
        });

        // 1. Group modules into directories for folder generation
        // We will store the folder's extracted z-index alongside its clean display name
        let mut groups_map: std::collections::BTreeMap<Option<String>, (i32, Vec<PythonModule>)> = std::collections::BTreeMap::new();

        for module in package.modules.clone() {
            // Find the physical folder name from the module's filepath to read its prefix
            let physical_folder_z_index = if let Some(ref clean_folder_name) = module.folder {
                let path = Path::new(&module.filepath);
                let mut z_val = i32::MAX; // Default if no prefix is found
                
                if let Some(parent) = path.parent() {
                    if let Some(folder_dir_name) = parent.file_name().map(|f| f.to_string_lossy()) {
                        if let Some(under_idx) = folder_dir_name.find('_') {
                            let prefix = &folder_dir_name[..under_idx];
                            if let Ok(parsed_z) = prefix.parse::<i32>() {
                                z_val = parsed_z;
                            }
                        }
                    }
                }
                z_val
            } else {
                i32::MAX // Root files default
            };

            let entry = groups_map.entry(module.folder.clone()).or_insert((physical_folder_z_index, Vec::new()));
            entry.1.push(module);
        }

        // 2. Convert groups to NavGroups and sort internally/globally
        let mut nav_groups: Vec<(i32, NavGroup)> = Vec::new();
        for (folder_name, (folder_z, mut mods)) in groups_map {
            // Sort files INSIDE this folder based on their internal z_index, then name
            mods.sort_by(|a, b| {
                if a.z_index == b.z_index {
                    a.name.cmp(&b.name)
                } else {
                    a.z_index.cmp(&b.z_index)
                }
            });

            nav_groups.push((
                folder_z,
                NavGroup {
                    name: folder_name,
                    modules: mods,
                },
            ));
        }

        // 3. Sort the folders and root level files globally!
        // Rules:
        // - Root level files (where name is None) have their order dictated by their individual module z_index.
        // - Folders (where name is Some) use their folder prefix z_index.
        nav_groups.sort_by(|(z_a, group_a), (z_b, group_b)| {
            let actual_z_a = if group_a.name.is_none() {
                // For root items, default to the first file's z-index or MAX
                group_a.modules.first().map(|m| m.z_index).unwrap_or(i32::MAX)
            } else {
                *z_a
            };

            let actual_z_b = if group_b.name.is_none() {
                group_b.modules.first().map(|m| m.z_index).unwrap_or(i32::MAX)
            } else {
                *z_b
            };

            if actual_z_a == actual_z_b {
                group_a.name.cmp(&group_b.name)
            } else {
                actual_z_a.cmp(&actual_z_b)
            }
        });

        // Strip the sorting z-index wrapper back off so we just pass Vec<NavGroup> to Tera
        let final_nav_groups: Vec<NavGroup> = nav_groups.into_iter().map(|(_, g)| g).collect();

        // Render index page
        let mut index_context = TeraContext::new();
        index_context.insert("package", package);
        index_context.insert("nav_groups", &final_nav_groups);
        index_context.insert("version", &self.version);
        let rendered_index = self.tera.render("index.html", &index_context)?;
        fs::write(Path::new(&self.output_dir).join("index.html"), rendered_index)?;

        // Render dedicated individual module detail pages
        for module in &package.modules {
            let mut mod_context = TeraContext::new();
            mod_context.insert("package", &package);
            mod_context.insert("nav_groups", &final_nav_groups);
            mod_context.insert("module", module);
            mod_context.insert("version", &self.version);

            // Select layout dynamically based on `#pd-write` status
            let template_name = if doc_only_modules.contains(&module.name) {
                "document.html"
            } else {
                "module.html"
            };

            let rendered_mod = self.tera.render(template_name, &mod_context)?;
            let file_name = format!("{}.html", module.name);
            fs::write(Path::new(&self.output_dir).join(file_name), rendered_mod)?;
        }

        // Render Search JS index
        let search_index = serde_json::to_string(&package)?;
        fs::write(Path::new(&self.output_dir).join("search-index.js"), format!("const searchIndex = {};", search_index))?;

        // Output style.css
        let css_content = include_str!("../templates/style.css");
        fs::write(Path::new(&self.output_dir).join("style.css"), css_content)?;

        // Output app.js
        let js_content = include_str!("../templates/app.js");
        fs::write(Path::new(&self.output_dir).join("app.js"), js_content)?;

        Ok(())
    }
}