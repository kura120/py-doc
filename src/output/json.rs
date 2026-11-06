//! `--format json`: the parsed API as `api.json`.
//!
//! The shape is defined here, not by the internal models, so a model change
//! cannot silently change the file. `docs/api.schema.json` describes it and
//! `tests/api_json.rs` validates real output against that schema.
//!
//! Every module is included after the public-API filtering (everything with
//! `--include-private`). Modules are sorted by dotted name, then path; their
//! members keep source order. With the same input, the bytes are the same.

use serde::Serialize;

use crate::models::{PythonClass, PythonFunction, PythonModule, PythonPackage, PythonVariable};

/// Version of the file's shape. Bump it when a field is renamed, removed or
/// changes type; adding a field does not change it.
pub const SCHEMA_VERSION: u32 = 1;

/// Where the file is written, relative to `--out`.
pub const FILE_NAME: &str = "api.json";

#[derive(Serialize)]
struct Api<'a> {
    schema_version: u32,
    py_doc_version: &'a str,
    package: Package<'a>,
    include_private: bool,
    modules: Vec<Module<'a>>,
}

#[derive(Serialize)]
struct Package<'a> {
    name: &'a str,
    version: &'a str,
}

/// The three forms of a docstring. `docstring` is exactly as written in the
/// source; `docstring_html` is what the HTML site shows (its links are
/// relative to the module's `page`); `summary` is its first sentence or line
/// as plain text.
#[derive(Serialize)]
struct Doc<'a> {
    docstring: Option<&'a str>,
    docstring_html: Option<&'a str>,
    summary: Option<&'a str>,
}

impl<'a> Doc<'a> {
    fn new(raw: &'a Option<String>, html: &'a Option<String>, summary: &'a Option<String>) -> Self {
        Doc {
            docstring: raw.as_deref(),
            docstring_html: html.as_deref(),
            summary: summary.as_deref(),
        }
    }
}

#[derive(Serialize)]
struct Module<'a> {
    name: &'a str,
    /// Dotted name; empty for a root `__init__.py`.
    qualname: &'a str,
    path: &'a str,
    /// The module's page in the HTML site.
    page: &'a str,
    #[serde(flatten)]
    doc: Doc<'a>,
    is_document: bool,
    is_private: bool,
    all: Option<&'a [String]>,
    classes: Vec<Class<'a>>,
    functions: Vec<Function<'a>>,
    /// Module-level variables, constants and type aliases.
    variables: Vec<Variable<'a>>,
}

#[derive(Serialize)]
struct Class<'a> {
    name: &'a str,
    qualname: String,
    line: usize,
    bases: &'a [String],
    #[serde(flatten)]
    doc: Doc<'a>,
    is_private: bool,
    methods: Vec<Function<'a>>,
    /// Class attributes, dataclass fields and enum members.
    variables: Vec<Variable<'a>>,
}

#[derive(Serialize)]
struct Function<'a> {
    name: &'a str,
    qualname: String,
    line: usize,
    /// Each parameter as written, with its annotation and default.
    args: &'a [String],
    return_type: Option<&'a str>,
    decorators: &'a [String],
    badges: &'a [String],
    is_async: bool,
    #[serde(flatten)]
    doc: Doc<'a>,
    is_private: bool,
}

#[derive(Serialize)]
struct Variable<'a> {
    name: &'a str,
    qualname: String,
    line: usize,
    annotation: Option<&'a str>,
    /// The value as written, whitespace collapsed, cut short if long.
    value: Option<&'a str>,
    value_truncated: bool,
    is_type_alias: bool,
    badges: &'a [String],
    #[serde(flatten)]
    doc: Doc<'a>,
    is_private: bool,
}

fn qualify(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}.{name}")
    }
}

fn function<'a>(f: &'a PythonFunction, parent: &str) -> Function<'a> {
    Function {
        name: &f.name,
        qualname: qualify(parent, &f.name),
        line: f.line,
        args: &f.args,
        return_type: f.return_type.as_deref(),
        decorators: &f.decorators,
        badges: &f.badges,
        is_async: f.is_async,
        doc: Doc::new(&f.raw_docstring, &f.docstring, &f.summary),
        is_private: f.is_private,
    }
}

fn variable<'a>(v: &'a PythonVariable, parent: &str) -> Variable<'a> {
    Variable {
        name: &v.name,
        qualname: qualify(parent, &v.name),
        line: v.line,
        annotation: v.annotation.as_deref(),
        value: v.value.as_deref(),
        value_truncated: v.value_truncated,
        is_type_alias: v.is_type_alias,
        badges: &v.badges,
        doc: Doc::new(&v.raw_docstring, &v.docstring, &v.summary),
        is_private: v.is_private,
    }
}

fn class<'a>(c: &'a PythonClass, module: &str) -> Class<'a> {
    let qualname = qualify(module, &c.name);
    Class {
        name: &c.name,
        line: c.line,
        bases: &c.bases,
        doc: Doc::new(&c.raw_docstring, &c.docstring, &c.summary),
        is_private: c.is_private,
        methods: c.functions.iter().map(|f| function(f, &qualname)).collect(),
        variables: c.variables.iter().map(|v| variable(v, &qualname)).collect(),
        qualname,
    }
}

fn module(m: &PythonModule) -> Module<'_> {
    Module {
        name: &m.name,
        qualname: &m.qualname,
        path: &m.source_path,
        page: &m.link_path,
        doc: Doc::new(&m.raw_docstring, &m.docstring, &m.summary),
        is_document: m.is_document,
        is_private: m.is_private,
        all: m.all.as_deref(),
        classes: m.classes.iter().map(|c| class(c, &m.qualname)).collect(),
        functions: m
            .functions
            .iter()
            .map(|f| function(f, &m.qualname))
            .collect(),
        variables: m
            .variables
            .iter()
            .map(|v| variable(v, &m.qualname))
            .collect(),
    }
}

/// The `api.json` text for a package that `SiteGenerator::render` prepared.
pub fn to_string(package: &PythonPackage, version: &str, include_private: bool) -> String {
    let mut modules: Vec<&PythonModule> = package.modules.iter().collect();
    modules.sort_by(|a, b| {
        a.qualname
            .cmp(&b.qualname)
            .then_with(|| a.source_path.cmp(&b.source_path))
    });
    let api = Api {
        schema_version: SCHEMA_VERSION,
        py_doc_version: env!("CARGO_PKG_VERSION"),
        package: Package {
            name: &package.name,
            version,
        },
        include_private,
        modules: modules.into_iter().map(module).collect(),
    };
    let mut text = serde_json::to_string_pretty(&api).unwrap_or_default();
    text.push('\n');
    text
}
