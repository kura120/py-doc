use anyhow::{Context, Result};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_parser::parse_module;
use ruff_text_size::{Ranged, TextRange};
use std::fs;
use std::path::Path;

use crate::models::{PythonClass, PythonFunction, PythonModule, PythonVariable};

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

    /// Like `slice`, but stops after `limit` characters (so a huge literal
    /// is never copied in full). The flag says whether it was cut short.
    fn slice_limited(&self, range: TextRange, limit: usize) -> (String, bool) {
        let text = self
            .text
            .get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or("");
        let mut out = String::new();
        let mut chars = 0;
        for word in text.split_whitespace() {
            if !out.is_empty() {
                out.push(' ');
                chars += 1;
            }
            for c in word.chars() {
                if chars == limit {
                    return (out.trim_end().to_string(), true);
                }
                out.push(c);
                chars += 1;
            }
        }
        (out, false)
    }

    /// The text of a 1-based line, without its line break.
    fn line(&self, line: usize) -> &'a str {
        let start = self.line_starts.get(line - 1).copied().unwrap_or(0);
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        self.text
            .get(start..end)
            .unwrap_or("")
            .trim_end_matches(['\n', '\r'])
    }
}

/// Values longer than this (after collapsing whitespace) are cut short.
const VALUE_LIMIT: usize = 80;

/// Special attributes that are never worth documenting, even with
/// `--include-private`.
const UNDOCUMENTED_NAMES: [&str; 6] = [
    "__all__",
    "__slots__",
    "__match_args__",
    "__annotations__",
    "__doc__",
    "__module__",
];

/// What kind of class the attributes belong to, for their badges.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassKind {
    Module,
    Plain,
    Enum,
    Dataclass,
}

fn class_kind(class_def: &ast::StmtClassDef, source: &Source) -> ClassKind {
    let last_segment = |text: &str| {
        let callee = text.split('(').next().unwrap_or(text).trim();
        callee.rsplit('.').next().unwrap_or(callee).to_string()
    };
    let is_enum = class_def.arguments.as_ref().is_some_and(|arguments| {
        arguments.args.iter().any(|base| {
            matches!(
                last_segment(&source.slice(base.range())).as_str(),
                "Enum" | "IntEnum" | "StrEnum" | "Flag" | "IntFlag" | "ReprEnum"
            )
        })
    });
    if is_enum {
        return ClassKind::Enum;
    }
    let is_dataclass = class_def.decorator_list.iter().any(|d| {
        let text = source.slice(d.expression.range());
        let callee = text.split('(').next().unwrap_or(&text).trim();
        matches!(
            callee,
            "attr.s"
                | "attr.attrs"
                | "attr.define"
                | "attr.frozen"
                | "attrs.define"
                | "attrs.frozen"
        ) || last_segment(callee) == "dataclass"
    });
    if is_dataclass {
        ClassKind::Dataclass
    } else {
        ClassKind::Plain
    }
}

/// The docstring of an attribute: a string literal statement right after it
/// (PEP 257), or else `#:` comments on the lines above it or after it on
/// its last line (the Sphinx convention).
fn attribute_docstring(stmt: &Stmt, next: Option<&Stmt>, source: &Source) -> Option<String> {
    if let Some(Stmt::Expr(expr)) = next
        && let Expr::StringLiteral(string_lit) = &*expr.value
    {
        return Some(string_lit.value.to_string());
    }

    let first_line = source.line_of(stmt.range().start().to_usize());
    let mut comments = Vec::new();
    let mut line = first_line;
    while line > 1 {
        line -= 1;
        match source.line(line).trim_start().strip_prefix("#:") {
            Some(text) => comments.push(text.strip_prefix(' ').unwrap_or(text)),
            None => break,
        }
    }
    comments.reverse();
    if comments.is_empty() {
        let end = stmt.range().end().to_usize();
        let rest = source.text.get(end..).unwrap_or("");
        let rest = rest.split('\n').next().unwrap_or("").trim();
        if let Some(text) = rest.strip_prefix("#:") {
            comments.push(text.trim());
        }
    }
    (!comments.is_empty()).then(|| comments.join("\n"))
}

/// Where a definition was found while walking a body.
#[derive(Clone, Copy)]
struct Origin {
    /// Which suite (statement list) it is in; definitions in the same suite
    /// are never merged (overloads, property setters).
    block: usize,
    /// Inside an `if`/`elif`/`else` or `try`/`except`/`else` branch.
    conditional: bool,
    /// Inside an `if TYPE_CHECKING:` branch: only seen by type checkers.
    type_checking: bool,
}

/// A statement that may define something, with the statement after it in
/// the same suite (for attribute docstrings).
struct Found<'a> {
    stmt: &'a Stmt,
    next: Option<&'a Stmt>,
    origin: Origin,
}

/// `TYPE_CHECKING` or `typing.TYPE_CHECKING`; `not ...` gives Some(false).
fn type_checking_test(test: &Expr) -> Option<bool> {
    match test {
        Expr::Name(name) if name.id.as_str() == "TYPE_CHECKING" => Some(true),
        Expr::Attribute(attr) if attr.attr.as_str() == "TYPE_CHECKING" => Some(true),
        Expr::UnaryOp(op) if op.op == ast::UnaryOp::Not => {
            type_checking_test(&op.operand).map(|tc| !tc)
        }
        _ => None,
    }
}

/// `__name__ == "__main__"`, in either order.
fn is_main_guard(test: &Expr) -> bool {
    let Expr::Compare(compare) = test else {
        return false;
    };
    let ([ast::CmpOp::Eq], [right]) = (&compare.ops[..], &compare.comparators[..]) else {
        return false;
    };
    let is_name = |e: &Expr| matches!(e, Expr::Name(n) if n.id.as_str() == "__name__");
    let is_main = |e: &Expr| matches!(e, Expr::StringLiteral(s) if s.value.to_str() == "__main__");
    (is_name(&compare.left) && is_main(right)) || (is_main(&compare.left) && is_name(right))
}

/// Collects the statements of `body` that may define something, descending
/// into `if`, `try` and `with` blocks but never into functions or classes,
/// and skipping `if __name__ == "__main__":` blocks.
fn walk_body<'a>(body: &'a [Stmt], origin: Origin, blocks: &mut usize, out: &mut Vec<Found<'a>>) {
    let nested = |body: &'a [Stmt],
                  conditional: bool,
                  type_checking: bool,
                  blocks: &mut usize,
                  out: &mut Vec<Found<'a>>| {
        *blocks += 1;
        let inner = Origin {
            block: *blocks,
            conditional: origin.conditional || conditional,
            type_checking: origin.type_checking || type_checking,
        };
        walk_body(body, inner, blocks, out);
    };
    for (index, stmt) in body.iter().enumerate() {
        match stmt {
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::Assign(_)
            | Stmt::AnnAssign(_)
            | Stmt::TypeAlias(_) => out.push(Found {
                stmt,
                next: body.get(index + 1),
                origin,
            }),
            Stmt::If(if_stmt) => {
                if is_main_guard(&if_stmt.test) {
                    continue;
                }
                let tc = type_checking_test(&if_stmt.test);
                nested(&if_stmt.body, true, tc == Some(true), blocks, out);
                for clause in &if_stmt.elif_else_clauses {
                    // The `else` of `if not TYPE_CHECKING` is type-checking only.
                    let clause_tc = clause.test.is_none() && tc == Some(false);
                    nested(&clause.body, true, clause_tc, blocks, out);
                }
            }
            Stmt::Try(try_stmt) => {
                nested(&try_stmt.body, true, false, blocks, out);
                for ast::ExceptHandler::ExceptHandler(handler) in &try_stmt.handlers {
                    nested(&handler.body, true, false, blocks, out);
                }
                nested(&try_stmt.orelse, true, false, blocks, out);
                // `finally` always runs.
                nested(&try_stmt.finalbody, false, false, blocks, out);
            }
            Stmt::With(with_stmt) => nested(&with_stmt.body, false, false, blocks, out),
            _ => {}
        }
    }
}

/// The definitions of a module or class body.
fn walk(body: &[Stmt]) -> Vec<Found<'_>> {
    let mut out = Vec::new();
    let mut blocks = 0;
    let top = Origin {
        block: 0,
        conditional: false,
        type_checking: false,
    };
    walk_body(body, top, &mut blocks, &mut out);
    out
}

/// Keeps one definition per name when the same name is defined in several
/// suites (`if`/`else`, `try`/`except`): the first one, except that a
/// runtime definition replaces one only seen under `TYPE_CHECKING`.
/// Several definitions in the same suite are all kept (overloads, property
/// setters) unless `merge_same_block` is set. Kept definitions that are
/// conditional, or had a runtime rival in another suite, get a
/// "conditional" badge; those only seen under `TYPE_CHECKING` get
/// "type-checking".
fn dedupe<T>(
    items: Vec<(T, Origin)>,
    name: impl Fn(&T) -> &str,
    badges: impl Fn(&mut T) -> &mut Vec<String>,
    merge_same_block: bool,
) -> Vec<T> {
    let n = items.len();
    let mut keep = vec![true; n];
    let mut conditional: Vec<bool> = items.iter().map(|(_, o)| o.conditional).collect();
    // Output position; a runtime definition that replaces type-checking
    // ones takes the place of the first of them.
    let mut position: Vec<usize> = (0..n).collect();
    let mut moved = false;
    if n > 1 {
        // Kept items with the same name form a chain: `heads` maps a name to
        // (first, last) of its chain and `next` links it. No allocation per
        // name, since most names appear once.
        const END: usize = usize::MAX;
        let mut heads: std::collections::HashMap<&str, (usize, usize)> =
            std::collections::HashMap::with_capacity(n);
        let mut next = vec![END; n];
        for i in 0..n {
            let origin = items[i].1;
            let key = name(&items[i].0);
            let Some(&(first, last)) = heads.get(key) else {
                heads.insert(key, (i, i));
                continue;
            };
            let chain = || {
                let next = &next;
                std::iter::successors(Some(first), move |&j| (next[j] != END).then(|| next[j]))
            };
            if !merge_same_block && chain().any(|j| items[j].1.block == origin.block) {
                next[last] = i;
                heads.insert(key, (first, i));
            } else if !origin.type_checking && chain().all(|j| items[j].1.type_checking) {
                // A runtime definition wins over type-checking-only ones.
                position[i] = position[first];
                moved = true;
                for j in chain().collect::<Vec<_>>() {
                    keep[j] = false;
                }
                heads.insert(key, (i, i));
            } else {
                keep[i] = false;
                if !origin.type_checking {
                    // A rival runtime definition: which one exists depends on the branch.
                    for j in chain().collect::<Vec<_>>() {
                        conditional[j] = true;
                    }
                }
            }
        }
    }

    let mut kept: Vec<(usize, T)> = items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(i, (mut item, origin))| {
            let label = if origin.type_checking {
                Some("type-checking")
            } else if conditional[i] {
                Some("conditional")
            } else {
                None
            };
            if let Some(label) = label {
                badges(&mut item).push(label.to_string());
            }
            (i, item)
        })
        .collect();
    if moved {
        kept.sort_by_key(|(i, _)| (position[*i], *i));
    }
    kept.into_iter().map(|(_, item)| item).collect()
}

/// Variables assigned in a module or class body (including inside `if`,
/// `try` and `with` blocks), in source order. A name assigned twice is
/// documented once (see `dedupe`). `is_private` decides visibility from the
/// name.
fn extract_variables(
    found: &[Found],
    source: &Source,
    kind: ClassKind,
    is_private: impl Fn(&str) -> bool,
) -> Vec<PythonVariable> {
    let mut variables: Vec<(PythonVariable, Origin)> = Vec::new();
    for Found { stmt, next, origin } in found {
        // (target, annotation, value, is_type_alias)
        let mut targets: Vec<(&Expr, Option<&Expr>, Option<&Expr>, bool)> = Vec::new();
        match stmt {
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    match target {
                        Expr::Name(_) => targets.push((target, None, Some(&assign.value), false)),
                        Expr::Tuple(ast::ExprTuple { elts, .. })
                        | Expr::List(ast::ExprList { elts, .. }) => {
                            targets.extend(elts.iter().map(|elt| (elt, None, None, false)))
                        }
                        _ => {}
                    }
                }
            }
            Stmt::AnnAssign(assign) => {
                targets.push((
                    &assign.target,
                    Some(&assign.annotation),
                    assign.value.as_deref(),
                    false,
                ));
            }
            Stmt::TypeAlias(alias) => targets.push((&alias.name, None, Some(&alias.value), true)),
            _ => continue,
        }

        let mut docstring: Option<Option<String>> = None;
        for (target, annotation, value, is_type_alias) in targets {
            let Expr::Name(name) = target else { continue };
            let name = name.id.as_str();
            if UNDOCUMENTED_NAMES.contains(&name) {
                continue;
            }
            let annotation = annotation.map(|a| annotation_to_string(a, source));
            let (value, value_truncated) = match value {
                Some(value) => {
                    let (text, truncated) = source.slice_limited(value.range(), VALUE_LIMIT);
                    (Some(text), truncated)
                }
                None => (None, false),
            };
            let private = is_private(name);
            let mut badges = Vec::new();
            match kind {
                ClassKind::Enum if value.is_some() && !private && !is_type_alias => {
                    badges.push("enum".to_string())
                }
                ClassKind::Dataclass
                    if annotation.as_deref().is_some_and(|a| {
                        !a.starts_with("ClassVar") && !a.contains(".ClassVar")
                    }) =>
                {
                    badges.push("field".to_string())
                }
                _ => {}
            }
            let docstring = docstring
                .get_or_insert_with(|| attribute_docstring(stmt, *next, source))
                .clone();
            let variable = PythonVariable {
                name: name.to_string(),
                annotation,
                value,
                value_truncated,
                is_type_alias,
                badges,
                line: source.line_of(target.range().start().to_usize()),
                docstring,
                summary: None,
                is_private: private,
            };
            variables.push((variable, *origin));
        }
    }
    dedupe(variables, |v| &v.name, |v| &mut v.badges, true)
}

/// The functions, classes and variables of a module or class body.
/// `prefix` is the qualified name of the enclosing class, if any.
/// `is_public` decides functions and classes; module variables use it too,
/// while class attributes follow the underscore rule, dunders included.
fn extract_members(
    body: &[Stmt],
    source: &Source,
    kind: ClassKind,
    prefix: Option<&str>,
    is_public: &dyn Fn(&str) -> bool,
) -> (Vec<PythonFunction>, Vec<PythonClass>, Vec<PythonVariable>) {
    let found = walk(body);
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    for item in &found {
        match item.stmt {
            Stmt::FunctionDef(func) => {
                let mut function = extract_function(func, source);
                function.is_private = !is_public(&function.name);
                functions.push((function, item.origin));
            }
            Stmt::ClassDef(class_def) => {
                let mut class = extract_class(class_def, source, prefix);
                class.is_private = !is_public(&class.name);
                classes.push((class, item.origin));
            }
            _ => {}
        }
    }
    let variables = match kind {
        ClassKind::Module => extract_variables(&found, source, kind, |name| !is_public(name)),
        _ => extract_variables(&found, source, kind, |name| name.starts_with('_')),
    };
    (
        dedupe(functions, |f| &f.name, |f| &mut f.badges, false),
        dedupe(classes, |c| &c.name, |c| &mut c.badges, false),
        variables,
    )
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
        variables: Vec::new(),
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
    let all = module.all.clone();
    (module.functions, module.classes, module.variables) =
        extract_members(&syntax_body, &source, ClassKind::Module, None, &|name| {
            is_public_name(name, all.as_deref())
        });

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

fn extract_class(
    class_def: &ast::StmtClassDef,
    source: &Source,
    prefix: Option<&str>,
) -> PythonClass {
    let name = class_def.name.to_string();
    let qualname = match prefix {
        Some(prefix) => format!("{}.{}", prefix, name),
        None => name.clone(),
    };

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

    let (functions, classes, variables) = extract_members(
        &class_def.body,
        source,
        class_kind(class_def, source),
        Some(&qualname),
        &|name| is_public_name(name, None),
    );

    PythonClass {
        name,
        qualname,
        bases,
        line: source.line_of(class_def.name.range().start().to_usize()),
        docstring: leading_docstring(&class_def.body),
        summary: None,
        badges: Vec::new(),
        functions,
        classes,
        variables,
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

    fn module_vars(code: &str) -> Vec<PythonVariable> {
        let source = Source::new(code);
        let body = parse_module(code).unwrap().into_syntax().body;
        let all = read_dunder_all(&body);
        extract_variables(&walk(&body), &source, ClassKind::Module, |name| {
            !is_public_name(name, all.as_deref())
        })
    }

    /// (functions, classes, variables) of a module, as "name[badges]".
    fn members_of(code: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
        let source = Source::new(code);
        let body = parse_module(code).unwrap().into_syntax().body;
        let (functions, classes, variables) =
            extract_members(&body, &source, ClassKind::Module, None, &|name| {
                is_public_name(name, None)
            });
        let show = |name: &str, badges: &[String]| {
            if badges.is_empty() {
                name.to_string()
            } else {
                format!("{}[{}]", name, badges.join(","))
            }
        };
        (
            functions.iter().map(|f| show(&f.name, &f.badges)).collect(),
            classes
                .iter()
                .map(|c| show(&c.qualname, &c.badges))
                .collect(),
            variables.iter().map(|v| show(&v.name, &v.badges)).collect(),
        )
    }

    fn class_of(code: &str) -> PythonClass {
        let source = Source::new(code);
        let body = parse_module(code).unwrap().into_syntax().body;
        let Some(Stmt::ClassDef(class_def)) = body.first() else {
            panic!("no class")
        };
        extract_class(class_def, &source, None)
    }

    fn summary(vars: &[PythonVariable]) -> Vec<String> {
        vars.iter()
            .map(|v| {
                format!(
                    "{}{}{}{}{}{}",
                    if v.is_type_alias { "type " } else { "" },
                    v.name,
                    v.annotation
                        .as_ref()
                        .map(|a| format!(": {}", a))
                        .unwrap_or_default(),
                    v.value
                        .as_ref()
                        .map(|a| format!(" = {}", a))
                        .unwrap_or_default(),
                    if v.value_truncated { "..." } else { "" },
                    if v.is_private { " (private)" } else { "" },
                )
            })
            .collect()
    }

    #[test]
    fn module_variables_in_every_form() {
        let vars = module_vars(
            "X = 1\nY: int\nZ: str = 'z'\ntype T = list[int]\na = b = 2\nc, d = 1, 2\n\
             obj.attr = 3\n_hidden = 4\n__version__ = '1'\n__all__ = ['X']\n__all__ += ['Y']",
        );
        // `__all__` is never documented; with it, only X and Y are public.
        assert_eq!(
            summary(&vars),
            [
                "X = 1",
                "Y: int",
                "Z: str = 'z' (private)",
                "type T = list[int] (private)",
                "a = 2 (private)",
                "b = 2 (private)",
                "c (private)",
                "d (private)",
                "_hidden = 4 (private)",
                "__version__ = '1' (private)",
            ]
        );

        let vars = module_vars("_hidden = 1\n__version__ = '1'\nshown = 2\nshown = 3");
        assert_eq!(
            summary(&vars),
            ["_hidden = 1 (private)", "__version__ = '1'", "shown = 2"]
        );
    }

    #[test]
    fn long_values_are_cut_short() {
        let long = format!("[{}]", "1, ".repeat(40));
        let vars = module_vars(&format!("LONG = {}\nSHORT = (\n    1,\n    2,\n)", long));
        assert!(vars[0].value_truncated);
        assert_eq!(vars[0].value.as_ref().unwrap().chars().count(), VALUE_LIMIT);
        assert!(!vars[1].value_truncated);
        assert_eq!(vars[1].value.as_deref(), Some("( 1, 2, )"));
    }

    #[test]
    fn attribute_docstrings() {
        let vars = module_vars(
            "A = 1\n\"\"\"Doc of A.\"\"\"\n\
             #: First line\n#: second line\nB = 2\n\
             C = 3  #: Inline.\n\
             # plain comment\nD = 4\n\
             #: Comment loses to the string.\nE = 5\n\"\"\"String.\"\"\"\n",
        );
        let docs: Vec<_> = vars.iter().map(|v| v.docstring.as_deref()).collect();
        assert_eq!(
            docs,
            [
                Some("Doc of A."),
                Some("First line\nsecond line"),
                Some("Inline."),
                None,
                Some("String."),
            ]
        );
    }

    #[test]
    fn class_attributes_enums_and_dataclasses() {
        let plain = class_of(
            "class C:\n    \"\"\"Class doc.\"\"\"\n    x = 1\n    _y = 2\n    __hash__ = None\n\
             \x20   __slots__ = ('x',)\n    __match_args__ = ('x',)\n    __doc__ = 'd'\n\
             \x20   def f(self): pass\n",
        );
        assert_eq!(
            summary(&plain.variables),
            ["x = 1", "_y = 2 (private)", "__hash__ = None (private)"]
        );
        assert!(plain.variables.iter().all(|v| v.badges.is_empty()));

        let color =
            class_of("class Color(enum.Enum):\n    RED = 1\n    GREEN = 2\n    _ignore_ = []\n");
        let badges: Vec<_> = color.variables.iter().map(|v| v.badges.clone()).collect();
        assert_eq!(badges, [vec!["enum"], vec!["enum"], vec![]]);

        let point = class_of(
            "@dataclasses.dataclass(frozen=True)\nclass P:\n    x: int\n    y: int = 0\n\
             \x20   z = 1\n    k: ClassVar[int] = 0\n",
        );
        let badges: Vec<_> = point.variables.iter().map(|v| v.badges.clone()).collect();
        assert_eq!(badges, [vec!["field"], vec!["field"], vec![], vec![]]);
    }

    #[test]
    fn branches_keep_the_first_definition() {
        let (functions, _, variables) = members_of(
            "if A:\n    def f(): pass\n    X = 1\nelif B:\n    def f(): pass\nelse:\n    def f(): pass\n    X = 2\n\
             def g(): pass\nif A:\n    def g(): pass\n",
        );
        assert_eq!(functions, ["f[conditional]", "g[conditional]"]);
        assert_eq!(variables, ["X[conditional]"]);
    }

    #[test]
    fn type_checking_definitions_never_replace_runtime_ones() {
        let (functions, _, variables) = members_of(
            "if TYPE_CHECKING:\n    def a(): pass\n    Alias = int\n\
             def a(): pass\n\
             def b(): pass\n\
             if typing.TYPE_CHECKING:\n    def b(): pass\n\
             if not TYPE_CHECKING:\n    def c(): pass\nelse:\n    def d(): pass\n",
        );
        assert_eq!(functions, ["a", "b", "c[conditional]", "d[type-checking]"]);
        assert_eq!(variables, ["Alias[type-checking]"]);
    }

    #[test]
    fn overloads_in_one_suite_are_all_kept() {
        let (functions, _, _) = members_of(
            "if TYPE_CHECKING:\n    @overload\n    def f(x: int): ...\n    @overload\n    def f(x: str): ...\n\
             def f(x): pass\n",
        );
        // The runtime definition replaces the type-checking overloads.
        assert_eq!(functions, ["f"]);

        let (functions, _, _) = members_of(
            "@overload\ndef f(x: int): ...\n@overload\ndef f(x: str): ...\ndef f(x): pass\n",
        );
        assert_eq!(functions, ["f[overload]", "f[overload]", "f"]);
    }

    #[test]
    fn try_with_and_skipped_blocks() {
        let (functions, classes, variables) = members_of(
            "try:\n    import fast\nexcept ImportError:\n    def fast(): pass\nelse:\n    OK = 1\nfinally:\n    DONE = 1\n\
             with lock:\n    INSIDE = 1\n\
             def outer():\n    def hidden(): pass\n    class Hidden: pass\n\
             if __name__ == '__main__':\n    def main(): pass\n    SCRIPT = 1\n\
             if '__main__' == __name__:\n    def main2(): pass\n",
        );
        assert_eq!(functions, ["fast[conditional]", "outer"]);
        assert!(classes.is_empty());
        assert_eq!(variables, ["OK[conditional]", "DONE", "INSIDE"]);
    }

    #[test]
    fn nested_classes() {
        let outer = class_of(
            "class Outer:\n    class Inner:\n        class Deep:\n            def ping(self): pass\n\
             \x20   class _Private: pass\n\
             \x20   if X:\n        class Meta: pass\n    else:\n        class Meta: pass\n",
        );
        let names: Vec<_> = outer
            .classes
            .iter()
            .map(|c| (c.qualname.as_str(), c.is_private, c.badges.clone()))
            .collect();
        assert_eq!(
            names,
            [
                ("Outer.Inner", false, vec![]),
                ("Outer._Private", true, vec![]),
                ("Outer.Meta", false, vec!["conditional".to_string()]),
            ]
        );
        let deep = &outer.classes[0].classes[0];
        assert_eq!(deep.qualname, "Outer.Inner.Deep");
        assert_eq!(deep.functions[0].name, "ping");
    }
}
