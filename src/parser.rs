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

/// `__init__`, `__call__`, `__eq__`, ...: special names that are public
/// even though they start with an underscore.
fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// The one place that decides whether a name is part of the public API.
///
/// `all` is the `__all__` of the module that defines the name, for top-level
/// names; pass None for class members. When a module defines `__all__`, it
/// alone decides (a listed `_name` is public). Otherwise names starting with
/// `_` are private, except dunders.
pub fn is_public_name(name: &str, all: Option<&[String]>) -> bool {
    match all {
        Some(all) => all.iter().any(|n| n == name),
        None => !name.starts_with('_') || is_dunder(name),
    }
}

/// A module is private when its own name, or the name of any package that
/// contains it, starts with `_`. `__init__` stands for its package, so only
/// the folders count for it. `components` are the path parts below the
/// source root, the last one being the module name without `.py`.
pub fn is_private_module(components: &[&str]) -> bool {
    components
        .iter()
        .any(|c| *c != "__init__" && c.starts_with('_'))
}

/// Reads a literal `__all__` from a module body: `__all__ = [...]` (or a
/// tuple, or an annotated assignment, or a `+` of literals), `__all__ +=
/// [...]`, `__all__.extend([...])` and `__all__.append("...")`. Returns None
/// if there is no `__all__`, or if any of those uses something that is not a
/// literal, since the real list cannot be known without running the code.
fn read_dunder_all(body: &[Stmt]) -> Option<Vec<String>> {
    fn is_all(expr: &Expr) -> bool {
        matches!(expr, Expr::Name(name) if name.id.as_str() == "__all__")
    }
    fn literal_names(expr: &Expr, out: &mut Vec<String>) -> bool {
        match expr {
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                elts.iter().all(|elt| match elt {
                    Expr::StringLiteral(s) => {
                        out.push(s.value.to_string());
                        true
                    }
                    _ => false,
                })
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: ast::Operator::Add,
                right,
                ..
            }) => literal_names(left, out) && literal_names(right, out),
            _ => false,
        }
    }

    let mut all: Option<Vec<String>> = None;
    let mut dynamic = false;
    for stmt in body {
        match stmt {
            Stmt::Assign(assign) if assign.targets.iter().any(is_all) => {
                let mut names = Vec::new();
                dynamic |= !literal_names(&assign.value, &mut names);
                all = Some(names);
            }
            Stmt::AnnAssign(assign) if is_all(&assign.target) => {
                if let Some(value) = &assign.value {
                    let mut names = Vec::new();
                    dynamic |= !literal_names(value, &mut names);
                    all = Some(names);
                }
            }
            Stmt::AugAssign(assign) if is_all(&assign.target) => {
                let names = all.get_or_insert_with(Vec::new);
                dynamic |= assign.op != ast::Operator::Add || !literal_names(&assign.value, names);
            }
            Stmt::Expr(expr) => {
                if let Expr::Call(call) = &*expr.value
                    && let Expr::Attribute(attr) = &*call.func
                    && is_all(&attr.value)
                {
                    let names = all.get_or_insert_with(Vec::new);
                    let args = &call.arguments.args;
                    dynamic |= match (attr.attr.as_str(), &args[..]) {
                        ("extend", [arg]) => !literal_names(arg, names),
                        ("append", [Expr::StringLiteral(s)]) => {
                            names.push(s.value.to_string());
                            false
                        }
                        _ => true,
                    };
                }
            }
            _ => {}
        }
    }
    if dynamic { None } else { all }
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

    let mut module_path: Vec<&str> = dir_components.iter().map(String::as_str).collect();
    module_path.push(&name);
    let is_private = is_private_module(&module_path);

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
        all: None,
        is_private,
    };

    let syntax_body = parsed.into_syntax().body;
    module.docstring = leading_docstring(&syntax_body);
    module.all = read_dunder_all(&syntax_body);

    for stmt in &syntax_body {
        match stmt {
            Stmt::FunctionDef(func) => {
                let mut function = extract_function(func, &source);
                function.is_private = !is_public_name(&function.name, module.all.as_deref());
                module.functions.push(function);
            }
            Stmt::ClassDef(class_def) => {
                let mut class = extract_class(class_def, &source);
                class.is_private = !is_public_name(&class.name, module.all.as_deref());
                module.classes.push(class);
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
        // Correct for methods; parse_file overrides it for top-level functions.
        is_private: !is_public_name(&func.name, None),
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
        is_private: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_of(code: &str) -> Option<Vec<String>> {
        read_dunder_all(&parse_module(code).unwrap().into_syntax().body)
    }

    fn names(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn reads_literal_dunder_all() {
        assert_eq!(all_of("x = 1"), None);
        assert_eq!(all_of("__all__ = ['a', \"b\"]"), names(&["a", "b"]));
        assert_eq!(all_of("__all__ = ('a',)"), names(&["a"]));
        assert_eq!(all_of("__all__: list[str] = ['a']"), names(&["a"]));
        assert_eq!(all_of("__all__ = ['a'] + ['b']"), names(&["a", "b"]));
        assert_eq!(
            all_of(
                "__all__ = ['a']\n__all__ += ('b',)\n__all__.extend(['c'])\n__all__.append('d')"
            ),
            names(&["a", "b", "c", "d"])
        );
        // A later assignment replaces the list.
        assert_eq!(all_of("__all__ = ['a']\n__all__ = ['b']"), names(&["b"]));
        assert_eq!(all_of("__all__ = []"), names(&[]));
    }

    #[test]
    fn dynamic_dunder_all_is_ignored() {
        assert_eq!(all_of("__all__ = other.__all__"), None);
        assert_eq!(all_of("__all__ = ['a'] + other.__all__"), None);
        assert_eq!(all_of("__all__ = ['a']\n__all__ += other.__all__"), None);
        assert_eq!(all_of("__all__ = ['a']\n__all__.extend(names)"), None);
        assert_eq!(all_of("__all__ = ['a', name]"), None);
        assert_eq!(all_of("__all__ = [n for n in dir()]"), None);
    }

    #[test]
    fn public_names() {
        assert!(is_public_name("connect", None));
        assert!(is_public_name("__init__", None));
        assert!(is_public_name("__call__", None));
        assert!(!is_public_name("_helper", None));
        assert!(!is_public_name("__mangled", None));
        assert!(!is_public_name("__", None));

        let all = vec!["connect".to_string(), "_exported".to_string()];
        assert!(is_public_name("connect", Some(&all)));
        assert!(is_public_name("_exported", Some(&all)));
        assert!(!is_public_name("other", Some(&all)));
        assert!(!is_public_name("__init__", Some(&all)));
    }

    #[test]
    fn private_modules() {
        assert!(!is_private_module(&["api"]));
        assert!(!is_private_module(&["__init__"]));
        assert!(!is_private_module(&["01_core", "engine"]));
        assert!(!is_private_module(&["pkg", "__init__"]));
        assert!(is_private_module(&["_helpers"]));
        assert!(is_private_module(&["__about__"]));
        assert!(is_private_module(&["_internal", "__init__"]));
        assert!(is_private_module(&["_internal", "core"]));
        assert!(is_private_module(&["pkg", "_impl", "core"]));
    }
}
