use anyhow::{Context, Result};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_parser::parse_module;
use ruff_text_size::{Ranged, TextRange};
use std::fs;
use std::path::Path;

use crate::models::{PythonClass, PythonFunction, PythonModule};

/// Splits an ordering prefix off a folder name: "01_getting started" ->
/// (Some(1), "getting started"). Names without a numeric prefix are returned as-is.
pub fn split_order_prefix(name: &str) -> (Option<i32>, &str) {
    if let Some((prefix, rest)) = name.split_once('_')
        && !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_digit())
    {
        return (prefix.parse().ok(), rest);
    }
    (None, name)
}

/// Source text plus a line index, for slicing expressions and reporting line numbers.
struct Source<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> Source<'a> {
    fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { text, line_starts }
    }

    /// 1-based line number of a byte offset.
    fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset)
    }

    /// The text of a range exactly as written, with line breaks and
    /// indentation collapsed to single spaces.
    fn slice(&self, range: TextRange) -> String {
        let text = self
            .text
            .get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or("Any");
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

pub fn parse_file(filepath: &Path, root_dir: &Path) -> Result<PythonModule> {
    let source_code = fs::read_to_string(filepath).context("could not read file")?;
    let source = Source::new(&source_code);

    let parsed = parse_module(&source_code).map_err(|e| {
        anyhow::anyhow!(
            "line {}: {}",
            source.line_of(e.location.start().to_usize()),
            e.error
        )
    })?;

    let relative = filepath.strip_prefix(root_dir).unwrap_or(filepath);
    let components: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    let source_path = components.join("/");

    let dir_components = &components[..components.len().saturating_sub(1)];
    let dir = dir_components.join("/");
    let display_dirs: Vec<&str> = dir_components
        .iter()
        .map(|c| split_order_prefix(c).1)
        .collect();
    let folder = if display_dirs.is_empty() {
        None
    } else {
        Some(display_dirs.join("/"))
    };

    let name = filepath
        .file_stem()
        .context("invalid filename")?
        .to_string_lossy()
        .to_string();

    let mut qualname_parts = display_dirs.clone();
    if name != "__init__" {
        qualname_parts.push(&name);
    }
    let qualname = qualname_parts.join(".");

    let mut module = PythonModule {
        name,
        source_path,
        qualname,
        docstring: None,
        summary: None,
        classes: Vec::new(),
        functions: Vec::new(),
        z_index: i32::MAX,
        folder,
        dir,
        link_path: String::new(),
        is_document: false,
    };

    let syntax_body = parsed.into_syntax().body;
    module.docstring = leading_docstring(&syntax_body);

    for stmt in &syntax_body {
        match stmt {
            Stmt::FunctionDef(func) => {
                module.functions.push(extract_function(func, &source));
            }
            Stmt::ClassDef(class_def) => {
                module.classes.push(extract_class(class_def, &source));
            }
            _ => {}
        }
    }

    Ok(module)
}

fn leading_docstring(body: &[Stmt]) -> Option<String> {
    if let Some(Stmt::Expr(expr)) = body.first()
        && let Expr::StringLiteral(string_lit) = &*expr.value
    {
        return Some(string_lit.value.to_string());
    }
    None
}

/// Annotations are shown as written, except quoted forward references
/// ("Engine"), which read better without their quotes.
fn annotation_to_string(expr: &Expr, source: &Source) -> String {
    match expr {
        Expr::StringLiteral(s) => s.value.to_string(),
        _ => source.slice(expr.range()),
    }
}

fn format_parameter(
    prefix: &str,
    parameter: &ast::Parameter,
    default: Option<&Expr>,
    source: &Source,
) -> String {
    let mut out = format!("{}{}", prefix, parameter.name);
    match (&parameter.annotation, default) {
        (Some(annotation), Some(default)) => {
            out.push_str(&format!(
                ": {} = {}",
                annotation_to_string(annotation, source),
                source.slice(default.range())
            ));
        }
        (Some(annotation), None) => {
            out.push_str(&format!(": {}", annotation_to_string(annotation, source)));
        }
        (None, Some(default)) => {
            out.push_str(&format!("={}", source.slice(default.range())));
        }
        (None, None) => {}
    }
    out
}

/// Maps well-known decorators to the short labels shown next to a signature.
fn badges_for(decorators: &[String]) -> Vec<String> {
    let mut badges = Vec::new();
    for decorator in decorators {
        let callee = decorator.split('(').next().unwrap_or(decorator);
        let label = match callee.rsplit('.').next().unwrap_or(callee) {
            "property" | "cached_property" => "property",
            "setter" => "setter",
            "staticmethod" => "static",
            "classmethod" => "classmethod",
            "abstractmethod" => "abstract",
            "overload" => "overload",
            _ => continue,
        };
        if !badges.iter().any(|b| b == label) {
            badges.push(label.to_string());
        }
    }
    badges
}

fn extract_function(func: &ast::StmtFunctionDef, source: &Source) -> PythonFunction {
    let params = &func.parameters;
    let mut args: Vec<String> = Vec::new();

    for arg in params.posonlyargs.iter() {
        args.push(format_parameter(
            "",
            &arg.parameter,
            arg.default.as_deref(),
            source,
        ));
    }
    if !params.posonlyargs.is_empty() {
        args.push("/".to_string());
    }
    for arg in params.args.iter() {
        args.push(format_parameter(
            "",
            &arg.parameter,
            arg.default.as_deref(),
            source,
        ));
    }
    if let Some(vararg) = &params.vararg {
        args.push(format_parameter("*", vararg, None, source));
    } else if !params.kwonlyargs.is_empty() {
        args.push("*".to_string());
    }
    for arg in params.kwonlyargs.iter() {
        args.push(format_parameter(
            "",
            &arg.parameter,
            arg.default.as_deref(),
            source,
        ));
    }
    if let Some(kwarg) = &params.kwarg {
        args.push(format_parameter("**", kwarg, None, source));
    }

    let return_type = func
        .returns
        .as_ref()
        .map(|ret_expr| annotation_to_string(ret_expr, source));

    let decorators: Vec<String> = func
        .decorator_list
        .iter()
        .map(|d| source.slice(d.expression.range()))
        .collect();
    let badges = badges_for(&decorators);

    PythonFunction {
        name: func.name.to_string(),
        args,
        return_type,
        decorators,
        badges,
        is_async: func.is_async,
        line: source.line_of(func.name.range().start().to_usize()),
        docstring: leading_docstring(&func.body),
        summary: None,
    }
}

fn extract_class(class_def: &ast::StmtClassDef, source: &Source) -> PythonClass {
    let functions = class_def
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::FunctionDef(func) => Some(extract_function(func, source)),
            _ => None,
        })
        .collect();

    let bases = class_def
        .arguments
        .as_ref()
        .map(|arguments| {
            arguments
                .args
                .iter()
                .map(|base| source.slice(base.range()))
                .collect()
        })
        .unwrap_or_default();

    PythonClass {
        name: class_def.name.to_string(),
        bases,
        line: source.line_of(class_def.name.range().start().to_usize()),
        docstring: leading_docstring(&class_def.body),
        summary: None,
        functions,
    }
}
