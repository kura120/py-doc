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

use crate::annotations::{LinkedTypes, TypeLink};
use crate::docstring::{DocBlock, DocItem, DocSections};
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
    /// "conditional" or "type-checking".
    badges: &'a [String],
    sections: Option<Sections<'a>>,
    /// Names in the annotations (or base classes) that link to their
    /// documentation; the annotations themselves stay as written.
    type_links: Vec<Link<'a>>,
    is_private: bool,
    methods: Vec<Function<'a>>,
    /// Class attributes, dataclass fields and enum members.
    variables: Vec<Variable<'a>>,
    /// Classes defined in the class body, in source order.
    classes: Vec<Class<'a>>,
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
    sections: Option<Sections<'a>>,
    /// Names in the annotations (or base classes) that link to their
    /// documentation; the annotations themselves stay as written.
    type_links: Vec<Link<'a>>,
    is_private: bool,
}

/// Docstring sections (`Args:`, `Parameters` + dashes, `:param x:`), taken
/// out of the docstring: `docstring` keeps the full text as written, while
/// `docstring_html` then holds only the rest of it.
#[derive(Serialize)]
struct Sections<'a> {
    /// "google", "numpy" or "rest".
    style: &'a str,
    params: Vec<Item<'a>>,
    keyword_params: Vec<Item<'a>>,
    other_params: Vec<Item<'a>>,
    attributes: Vec<Item<'a>>,
    returns: Vec<Item<'a>>,
    yields: Vec<Item<'a>>,
    raises: Vec<Item<'a>>,
    examples: Vec<Block<'a>>,
    notes: Vec<Block<'a>>,
    warnings: Vec<Block<'a>>,
    see_also: Vec<Block<'a>>,
}

#[derive(Serialize)]
struct Item<'a> {
    name: Option<&'a str>,
    /// As written in the docstring, else from the signature; for raises,
    /// the exception class.
    #[serde(rename = "type")]
    ty: Option<&'a str>,
    /// From the signature.
    default: Option<&'a str>,
    description: &'a str,
    description_html: &'a str,
    /// Names in `type` that link to their documentation.
    type_links: Vec<Link<'a>>,
}

#[derive(Serialize)]
struct Block<'a> {
    text: &'a str,
    html: &'a str,
}

/// A name in an annotation and the page and anchor it links to, from the
/// site root.
#[derive(Serialize)]
struct Link<'a> {
    name: &'a str,
    target: &'a str,
}

fn links(list: &[TypeLink]) -> Vec<Link<'_>> {
    list.iter()
        .map(|l| Link {
            name: &l.name,
            target: &l.target,
        })
        .collect()
}

fn linked_links(linked: &Option<LinkedTypes>) -> Vec<Link<'_>> {
    linked.as_ref().map(|l| links(&l.links)).unwrap_or_default()
}

fn items(list: &[DocItem]) -> Vec<Item<'_>> {
    list.iter()
        .map(|i| Item {
            name: i.name.as_deref(),
            ty: i.ty.as_deref(),
            default: i.default.as_deref(),
            description: &i.description,
            description_html: &i.description_html,
            type_links: links(&i.type_links),
        })
        .collect()
}

fn blocks(list: &[DocBlock]) -> Vec<Block<'_>> {
    list.iter()
        .map(|b| Block {
            text: &b.text,
            html: &b.html,
        })
        .collect()
}

fn sections(s: &Option<DocSections>) -> Option<Sections<'_>> {
    s.as_ref().map(|s| Sections {
        style: &s.style,
        params: items(&s.params),
        keyword_params: items(&s.keyword_params),
        other_params: items(&s.other_params),
        attributes: items(&s.attributes),
        returns: items(&s.returns),
        yields: items(&s.yields),
        raises: items(&s.raises),
        examples: blocks(&s.examples),
        notes: blocks(&s.notes),
        warnings: blocks(&s.warnings),
        see_also: blocks(&s.see_also),
    })
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
    /// Names in the annotations (or base classes) that link to their
    /// documentation; the annotations themselves stay as written.
    type_links: Vec<Link<'a>>,
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
        sections: sections(&f.sections),
        type_links: linked_links(&f.linked),
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
        type_links: linked_links(&v.linked),
        is_private: v.is_private,
    }
}

fn class<'a>(c: &'a PythonClass, module: &str) -> Class<'a> {
    // `c.qualname` is "Outer.Inner" for a nested class.
    let in_module = if c.qualname.is_empty() {
        &c.name
    } else {
        &c.qualname
    };
    let qualname = qualify(module, in_module);
    Class {
        name: &c.name,
        line: c.line,
        bases: &c.bases,
        doc: Doc::new(&c.raw_docstring, &c.docstring, &c.summary),
        badges: &c.badges,
        sections: sections(&c.sections),
        type_links: linked_links(&c.linked),
        is_private: c.is_private,
        methods: c.functions.iter().map(|f| function(f, &qualname)).collect(),
        variables: c.variables.iter().map(|v| variable(v, &qualname)).collect(),
        classes: c.classes.iter().map(|inner| class(inner, module)).collect(),
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
