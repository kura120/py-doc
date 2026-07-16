use anyhow::{Result, Context as AnyhowContext};
use tera::{Tera, Context as TeraContext, Kwargs, State}; 
use std::fs;
use std::path::Path;
use std::collections::HashSet;
use std::time::Instant;
use std::thread;
use pulldown_cmark::{Parser, html, Event, Tag, CowStr};
use rayon::prelude::*; // Blazing-fast parallel processing!

use crate::models::{PythonPackage, PythonModule};
use crate::resolver::Resolver;

const BASE_URL: &str = "https://raw.githubusercontent.com/kura120/py-doc/refs/heads/master/assets";


#[derive(Debug, Clone, serde::Serialize)]
pub struct NavGroup {
    pub name: Option<String>,
    pub modules: Vec<PythonModule>,
}

/// Helper to dynamically strip Python indentation from docstrings
fn clean_docstring(docstring: &str) -> String {
    let lines: Vec<&str> = docstring.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

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
    let mut z_index = i32::MAX;
    let mut cleaned_lines = Vec::new();

    for line in docstring.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#pd-z-index:") {
            let val_str = trimmed.trim_start_matches("#pd-z-index:").trim();
            if let Ok(parsed_val) = val_str.parse::<i32>() {
                z_index = parsed_val;
            }
            continue;
        }
        cleaned_lines.push(line);
    }

    (z_index, cleaned_lines.join("\n"))
}

// Custom tera striptags filter
fn striptags_filter(value: &str, _args: Kwargs, _state: &State) -> Result<String, tera::Error> {
    let re = regex::Regex::new(r"<[^>]*>").map_err(|e| {
        tera::Error::message(format!("Failed to compile regex: {}", e))
    })?;
    Ok(re.replace_all(value, "").into_owned())
}

enum AlertType {
    Error,
    Note,
    Warning,
}

impl AlertType {
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
        println!("\x1b[36;1m[1/3]\x1b[0m Fetching remote theme assets in parallel...");
        let fetch_start = Instant::now();
        

        // Spin up background threads to fetch all assets concurrently
        let urls = vec![
            ("sidebar", format!("{}/sidebar.html", BASE_URL)),
            ("index", format!("{}/index.html", BASE_URL)),
            ("module", format!("{}/module.html", BASE_URL)),
            ("document", format!("{}/document.html", BASE_URL)),
        ];

        let mut handles = vec![];
        for (name, url) in urls {
            handles.push(thread::spawn(move || -> Result<(String, String)> {
                let mut response = ureq::get(&url)
                    .call()
                    .with_context(|| format!("Failed to fetch {}", url))?;
                let body = response
                    .body_mut()
                    .read_to_string()
                    .with_context(|| format!("Failed to read {}", url))?;
                Ok((name.to_string(), body))
            }));
        }

        // Collect the results
        let mut templates = std::collections::HashMap::new();
        for handle in handles {
            let (name, body) = handle.join().map_err(|_| anyhow::anyhow!("Thread panicked fetching asset"))??;
            templates.insert(name, body);
        }

        println!("\x1b[32m✔ Loaded all templates in {:.2?}\x1b[0m", fetch_start.elapsed());

        // Initialize Tera
        let mut tera = Tera::default();

        // CRITICAL FIX 1: Register custom filter BEFORE compiling templates
        tera.register_filter("striptags", striptags_filter);

        // CRITICAL FIX 2: Register children/components (sidebar) BEFORE parent files (index)
        tera.add_raw_template("sidebar.html", templates.get("sidebar").unwrap())
            .with_context(|| "Failed to load sidebar.html component")?;
        tera.add_raw_template("index.html", templates.get("index").unwrap())
            .with_context(|| "Failed to load index.html template assets")?;
        tera.add_raw_template("module.html", templates.get("module").unwrap())
            .with_context(|| "Failed to load module.html template assets")?;
        tera.add_raw_template("document.html", templates.get("document").unwrap())
            .with_context(|| "Failed to load document.html template assets")?;

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

        let dest_dir = Path::new(&self.output_dir).join("assets");
        fs::create_dir_all(&dest_dir)?;

        let file_name = src_image_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid image path filename"))?;
        let dest_image_path = dest_dir.join(file_name);

        fs::copy(&src_image_path, &dest_image_path)?;
        Ok(format!("assets/{}", file_name.to_string_lossy()))
    }

    fn render_markdown(&self, markdown: &str) -> String {
        let dedented_md = clean_docstring(markdown);

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

        let parser = Parser::new(&processed_md);
        
        let mapped_events = parser.map(|event| match event {
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                if !dest_url.starts_with("http://") && !dest_url.starts_with("https://") {
                    match self.process_image_path(&dest_url) {
                        Ok(new_path) => Event::Start(Tag::Image {
                            link_type,
                            dest_url: CowStr::Boxed(new_path.into_boxed_str()),
                            title,
                            id,
                        }),
                        Err(e) => {
                            eprintln!("\x1b[31;1mSmart Image Error in Markdown Tag:\x1b[0m {}", e);
                            Event::Text(CowStr::Boxed(format!("[⚠️ Image Error: {}]", e).into_boxed_str()))
                        }
                    }
                } else {
                    Event::Start(Tag::Image { link_type, dest_url, title, id })
                }
            }
            other => other,
        });

        let mut html_output = String::new();
        html::push_html(&mut html_output, mapped_events);
        html_output
    }

    pub fn generate(&self, package: &mut PythonPackage, _resolver: &Resolver) -> Result<()> {
        println!("\x1b[36;1m[2/3]\x1b[0m Compiling and generating documentation...");
        let compile_start = Instant::now();

        fs::create_dir_all(&self.output_dir)?;

        // Parallelize markdown processing over all files using Rayon!
        package.modules.par_iter_mut().for_each(|module| {
            let raw_doc = module.docstring.clone().unwrap_or_default();
            let (z_val, cleaned_doc) = extract_z_index_and_clean(&raw_doc);
            
            module.z_index = z_val;
            if module.docstring.is_some() {
                module.docstring = Some(cleaned_doc);
            }

            if let Some(ref doc) = module.docstring {
                module.docstring = Some(self.render_markdown(doc));
            }

            // Process classes & methods inside this module concurrently
            module.classes.iter_mut().for_each(|class| {
                if let Some(ref doc) = class.docstring {
                    class.docstring = Some(self.render_markdown(doc));
                }
                class.functions.iter_mut().for_each(|method| {
                    if let Some(ref doc) = method.docstring {
                        method.docstring = Some(self.render_markdown(doc));
                    }
                });
            });

            module.functions.iter_mut().for_each(|func| {
                if let Some(ref doc) = func.docstring {
                    func.docstring = Some(self.render_markdown(doc));
                }
            });
        });

        // Determine pure documentation files (doc-only)
        let mut doc_only_modules = HashSet::new();
        for module in &package.modules {
            let has_code = !module.classes.is_empty() || !module.functions.is_empty();
            if let Some(ref doc) = module.docstring {
                if doc.trim_start().starts_with("#pd-write") && !has_code {
                    doc_only_modules.insert(module.name.clone());
                }
            }
        }

        // Sort modules
        package.modules.sort_by(|a, b| {
            if a.z_index == b.z_index {
                a.name.cmp(&b.name)
            } else {
                a.z_index.cmp(&b.z_index)
            }
        });

        // Group modules into directories
        let mut groups_map: std::collections::BTreeMap<Option<String>, (i32, Vec<PythonModule>)> = std::collections::BTreeMap::new();
        for module in package.modules.clone() {
            let physical_folder_z_index = if module.folder.is_some() {
                let path = Path::new(&module.filepath);
                let mut z_val = i32::MAX;
                
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
                i32::MAX
            };

            let entry = groups_map.entry(module.folder.clone()).or_insert((physical_folder_z_index, Vec::new()));
            entry.1.push(module);
        }

        // Convert and sort groups
        let mut nav_groups: Vec<(i32, NavGroup)> = Vec::new();
        for (folder_name, (folder_z, mut mods)) in groups_map {
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

        // Sort folders globally
        nav_groups.sort_by(|(z_a, group_a), (z_b, group_b)| {
            let actual_z_a = if group_a.name.is_none() {
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

        let final_nav_groups: Vec<NavGroup> = nav_groups.into_iter().map(|(_, g)| g).collect();

        // Render index page
        let mut index_context = TeraContext::new();
        index_context.insert("package", package);
        index_context.insert("nav_groups", &final_nav_groups);
        index_context.insert("version", &self.version);
        let rendered_index = self.tera.render("index.html", &index_context)?;
        fs::write(Path::new(&self.output_dir).join("index.html"), rendered_index)?;

        // Render detailed individual module detail pages
        for module in &package.modules {
            let mut mod_context = TeraContext::new();
            mod_context.insert("package", &package);
            mod_context.insert("nav_groups", &final_nav_groups);
            mod_context.insert("module", module);
            mod_context.insert("version", &self.version);

            let template_name = if doc_only_modules.contains(&module.name) {
                "document.html"
            } else {
                "module.html"
            };

            let rendered_mod = self.tera.render(template_name, &mod_context)?;
            let file_name = format!("{}.html", module.name);
            fs::write(Path::new(&self.output_dir).join(file_name), rendered_mod)?;
        }

        println!("\x1b[36;1m[3/3]\x1b[0m Generating search indexes & styles...");
        
        // Render Search JS index
        let search_index = serde_json::to_string(&package)?;
        fs::write(Path::new(&self.output_dir).join("search-index.js"), format!("const searchIndex = {};", search_index))?;

        // Define the base assets URL
        let out_dir = self.output_dir.clone();

        // Download and save style.css and app.js in parallel, catching any errors!
        thread::scope(|s| {
            s.spawn(|| {
                let css_url = format!("{}/style.css", BASE_URL);
                match ureq::get(&css_url).call() {
                    Ok(mut response) => {
                        match response.body_mut().read_to_string() {
                            Ok(content) => {
                                let dest = Path::new(&out_dir).join("style.css");
                                if let Err(e) = fs::write(&dest, content) {
                                    eprintln!("\x1b[31;1mError writing style.css:\x1b[0m {}", e);
                                } else {
                                    println!("\x1b[32m✔ Successfully wrote style.css to disk\x1b[0m");
                                }
                            }
                            Err(e) => eprintln!("\x1b[31;1mError reading style.css body:\x1b[0m {}", e),
                        }
                    }
                    Err(e) => eprintln!("\x1b[31;1mError downloading style.css:\x1b[0m {}", e),
                }
            });

            s.spawn(|| {
                let js_url = format!("{}/app.js", BASE_URL);
                match ureq::get(&js_url).call() {
                    Ok(mut response) => {
                        match response.body_mut().read_to_string() {
                            Ok(content) => {
                                let dest = Path::new(&out_dir).join("app.js");
                                if let Err(e) = fs::write(&dest, content) {
                                    eprintln!("\x1b[31;1mError writing app.js:\x1b[0m {}", e);
                                } else {
                                    println!("\x1b[32m✔ Successfully wrote app.js to disk\x1b[0m");
                                }
                            }
                            Err(e) => eprintln!("\x1b[31;1mError reading app.js body:\x1b[0m {}", e),
                        }
                    }
                    Err(e) => eprintln!("\x1b[31;1mError downloading app.js:\x1b[0m {}", e),
                }
            });
        });

        println!(
            "\x1b[32;1m✔ Successfully generated site for {} modules in {:.2?}\x1b[0m", 
            package.modules.len(), 
            compile_start.elapsed()
        );

        Ok(())
    }
}