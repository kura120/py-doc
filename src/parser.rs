use anyhow::{Result, Context};
use ruff_python_parser::parse_module;
use ruff_python_ast::{self as ast, Stmt, Expr};
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
                module.functions.push(extract_function(func));
            }
            Stmt::ClassDef(class_def) => {
                module.classes.push(extract_class(class_def));
            }
            _ => {}
        }
    }

    Ok(module)
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Name(name) => name.id.to_string(),
        Expr::Subscript(subscript) => {
            let value_str = expr_to_string(&subscript.value);
            let slice_str = expr_to_string(&subscript.slice);
            format!("{}[{}]", value_str, slice_str)
        }
        Expr::NoneLiteral(_) => "None".to_string(),
        Expr::StringLiteral(s) => s.value.to_string(),
        _ => "Any".to_string(),
    }
}

fn extract_function(func: &ast::StmtFunctionDef) -> PythonFunction {
    let mut docstring = None;
    if let Some(Stmt::Expr(expr)) = func.body.first() {
        if let Expr::StringLiteral(string_lit) = &*expr.value {
            docstring = Some(string_lit.value.to_string());
        }
    }

    let args = func.parameters.args.iter().map(|arg| {
        let name = arg.parameter.name.to_string();
        if let Some(ref annotation) = arg.parameter.annotation {
            format!("{}: {}", name, expr_to_string(annotation))
        } else {
            name
        }
    }).collect();

    let return_type = func.returns.as_ref().map(|ret_expr| expr_to_string(ret_expr));

    PythonFunction {
        name: func.name.to_string(),
        args,
        return_type,
        docstring,
    }
}

fn extract_class(class_def: &ast::StmtClassDef) -> PythonClass {
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
                functions.push(extract_function(func));
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