use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct PythonPackage {
    pub name: String,
    pub modules: Vec<PythonModule>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonModule {
    pub name: String,
    /// Path relative to the source root, with forward slashes (e.g. "01_core/engine.py").
    pub source_path: String,
    /// Dotted display path with ordering prefixes stripped (e.g. "core.engine").
    pub qualname: String,
    pub docstring: Option<String>,
    /// First meaningful line of the docstring, as plain text.
    pub summary: Option<String>,
    pub classes: Vec<PythonClass>,
    pub functions: Vec<PythonFunction>,
    pub z_index: i32,
    /// Display path of the containing folder, ordering prefixes stripped (e.g. "core/sub").
    pub folder: Option<String>,
    /// Raw directory relative to the source root (e.g. "01_core/sub"); the grouping key.
    #[serde(skip)]
    pub dir: String,
    pub link_path: String,
    /// True for doc-only modules (`#pd-write` docstring, no code), rendered with document.html.
    pub is_document: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonClass {
    pub name: String,
    pub bases: Vec<String>,
    pub line: usize,
    pub docstring: Option<String>,
    pub summary: Option<String>,
    pub functions: Vec<PythonFunction>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonFunction {
    pub name: String,
    pub args: Vec<String>,
    pub return_type: Option<String>,
    pub decorators: Vec<String>,
    /// Short labels derived from well-known decorators ("property", "static", ...).
    pub badges: Vec<String>,
    pub is_async: bool,
    pub line: usize,
    pub docstring: Option<String>,
    pub summary: Option<String>,
}
