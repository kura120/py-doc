use anyhow::{Result, Context};
use ruff_python_parser::parse_module;
use ruff_python_ast::{self as ast, Stmt, Expr};
use ruff_text_size::Ranged;
use std::fs;
use std::path::Path;

use crate::models::{PythonClass, PythonFunction, PythonModule};

pub fn parse_file(filepath: &Path, root_dir: &Path) -> Result<PythonModule> {
    let source_code = fs::read_to_string(filepath)
        .with_context(|| format!("Failed to read file: {}", filepath.display()))?;
    
    let parsed = parse_module(&source_code)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {:?}", filepath.display(), e))?;
    
    // Calculate folder name relative to the root directory
    // Calculate folder name relative to the root directory
    let folder = if let Ok(relative_path) = filepath.strip_prefix(root_dir) {
        relative_path
            .parent()
            .and_then(|p| {
                let p_str = p.to_string_lossy();
                if p_str.is_empty() {
                    None // Directly in the root input directory
                } else {
                    // Extract only the direct parent folder name
                    let folder_name = p.file_name()?.to_string_lossy().to_string();
                    
                    // Strip "XX_" prefix if it exists (e.g., "01_getting started" -> "getting started")
                    if let Some(under_idx) = folder_name.find('_') {
                        let prefix = &folder_name[..under_idx];
                        if prefix.chars().all(|c| c.is_ascii_digit()) {
                            Some(folder_name[under_idx + 1..].to_string())
                        } else {
                            Some(folder_name)
                        }
                    } else {
                        Some(folder_name)
                    }
                }
            })
    } else {
        None
    };

    let mut module = PythonModule {
        name: filepath.file_stem()
            .context("Invalid filename")?
            .to_string_lossy()
            .to_string(),
        filepath: filepath.to_string_lossy().to_string(),
        docstring: None,
        classes: Vec::new(),
        functions: Vec::new(),
        z_index: i32::MAX,
        folder,
        link_path: String::new(), // NEW field for the nav-link fix
        is_document: false,
    };

    let syntax_body = parsed.into_syntax().body;
    if let Some(Stmt::Expr(expr)) = syntax_body.first() {
        if let Expr::StringLiteral(string_lit) = &*expr.value {
            module.docstring = Some(string_lit.value.to_string());
        }
    }

    for stmt in &syntax_body {
        match stmt {
            Stmt::FunctionDef(func) => {
                module.functions.push(extract_function(func, &source_code));
            }
            Stmt::ClassDef(class_def) => {
                module.classes.push(extract_class(class_def, &source_code));
            }
            _ => {}
        }
    }

    Ok(module)
}

/// Renders an expression exactly as written in the source, with any
/// line breaks and indentation collapsed to single spaces.
fn expr_source(expr: &Expr, source: &str) -> String {
    let range = expr.range();
    let text = source
        .get(range.start().to_usize()..range.end().to_usize())
        .unwrap_or("Any");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Annotations are shown as written, except quoted forward references
/// ("Engine"), which read better without their quotes.
fn annotation_to_string(expr: &Expr, source: &str) -> String {
    match expr {
        Expr::StringLiteral(s) => s.value.to_string(),
        _ => expr_source(expr, source),
    }
}

fn format_parameter(
    prefix: &str,
    parameter: &ast::Parameter,
    default: Option<&Expr>,
    source: &str,
) -> String {
    let mut out = format!("{}{}", prefix, parameter.name);
    match (&parameter.annotation, default) {
        (Some(annotation), Some(default)) => {
            out.push_str(&format!(
                ": {} = {}",
                annotation_to_string(annotation, source),
                expr_source(default, source)
            ));
        }
        (Some(annotation), None) => {
            out.push_str(&format!(": {}", annotation_to_string(annotation, source)));
        }
        (None, Some(default)) => {
            out.push_str(&format!("={}", expr_source(default, source)));
        }
        (None, None) => {}
    }
    out
}

fn extract_function(func: &ast::StmtFunctionDef, source: &str) -> PythonFunction {
    let mut docstring = None;
    if let Some(Stmt::Expr(expr)) = func.body.first() {
        if let Expr::StringLiteral(string_lit) = &*expr.value {
            docstring = Some(string_lit.value.to_string());
        }
    }

    let params = &func.parameters;
    let mut args: Vec<String> = Vec::new();

    for arg in params.posonlyargs.iter() {
        args.push(format_parameter("", &arg.parameter, arg.default.as_deref(), source));
    }
    if !params.posonlyargs.is_empty() {
        args.push("/".to_string());
    }
    for arg in params.args.iter() {
        args.push(format_parameter("", &arg.parameter, arg.default.as_deref(), source));
    }
    if let Some(vararg) = &params.vararg {
        args.push(format_parameter("*", vararg, None, source));
    } else if !params.kwonlyargs.is_empty() {
        args.push("*".to_string());
    }
    for arg in params.kwonlyargs.iter() {
        args.push(format_parameter("", &arg.parameter, arg.default.as_deref(), source));
    }
    if let Some(kwarg) = &params.kwarg {
        args.push(format_parameter("**", kwarg, None, source));
    }

    let return_type = func.returns.as_ref().map(|ret_expr| annotation_to_string(ret_expr, source));

    PythonFunction {
        name: func.name.to_string(),
        args,
        return_type,
        docstring,
    }
}

fn extract_class(class_def: &ast::StmtClassDef, source: &str) -> PythonClass {
    let mut functions = Vec::new(); // Standardized to matches PythonClass
    let mut docstring = None;

    for stmt in &class_def.body {
        match stmt {
            Stmt::Expr(expr) if docstring.is_none() => {
                if let Expr::StringLiteral(string_lit) = &*expr.value {
                    docstring = Some(string_lit.value.to_string());
                }
            }
            Stmt::FunctionDef(func) => {
                functions.push(extract_function(func, source));
            }
            _ => {}
        }
    }

    PythonClass {
        name: class_def.name.to_string(),
        docstring,
        functions,
    }
}