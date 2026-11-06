use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct PythonPackage {
    pub name: String,
    pub modules: Vec<PythonModule>,
    /// Private items removed by [`PythonPackage::hide_private`]: whole private
    /// modules, plus stubs of public modules holding only their hidden
    /// symbols. Never rendered; the resolver uses it to explain why a
    /// doc-link target is missing.
    #[serde(skip)]
    pub hidden: Vec<PythonModule>,
}

/// Splits `items` into (public, private) by their `is_private` flag.
fn split_private<T>(items: Vec<T>, is_private: impl Fn(&T) -> bool) -> (Vec<T>, Vec<T>) {
    items.into_iter().partition(|item| !is_private(item))
}

impl PythonPackage {
    /// Moves every private module, class, function and variable out of
    /// `modules` and into `hidden`, so pages, navigation and search only see
    /// the public API.
    pub fn hide_private(&mut self) {
        let mut public = Vec::with_capacity(self.modules.len());
        for mut module in std::mem::take(&mut self.modules) {
            if module.is_private {
                self.hidden.push(module);
                continue;
            }

            let mut stub = module.empty_like();
            (module.classes, stub.classes) = split_private(module.classes, |c| c.is_private);
            stub.classes.extend(
                module
                    .classes
                    .iter_mut()
                    .filter_map(PythonClass::hide_private),
            );
            (module.functions, stub.functions) = split_private(module.functions, |f| f.is_private);
            (module.variables, stub.variables) = split_private(module.variables, |v| v.is_private);

            if !stub.classes.is_empty() || !stub.functions.is_empty() || !stub.variables.is_empty()
            {
                self.hidden.push(stub);
            }
            public.push(module);
        }
        self.modules = public;
    }
}

impl PythonClass {
    /// Removes private members, recursively through nested classes, and
    /// returns a stub holding them (None if there were none).
    fn hide_private(&mut self) -> Option<PythonClass> {
        let functions;
        let variables;
        let mut classes;
        (self.functions, functions) =
            split_private(std::mem::take(&mut self.functions), |f| f.is_private);
        (self.variables, variables) =
            split_private(std::mem::take(&mut self.variables), |v| v.is_private);
        (self.classes, classes) =
            split_private(std::mem::take(&mut self.classes), |c| c.is_private);
        classes.extend(
            self.classes
                .iter_mut()
                .filter_map(PythonClass::hide_private),
        );
        if functions.is_empty() && variables.is_empty() && classes.is_empty() {
            return None;
        }
        Some(PythonClass {
            name: self.name.clone(),
            qualname: self.qualname.clone(),
            bases: Vec::new(),
            line: self.line,
            docstring: None,
            raw_docstring: None,
            summary: None,
            badges: Vec::new(),
            functions,
            classes,
            variables,
            is_private: false,
        })
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonModule {
    pub name: String,
    /// Path relative to the source root, with forward slashes (e.g. "01_core/engine.py").
    pub source_path: String,
    /// Dotted display path with ordering prefixes stripped (e.g. "core.engine").
    pub qualname: String,
    pub docstring: Option<String>,
    /// The docstring as written, kept when rendering replaces `docstring`
    /// with HTML. Only for `--format json`; not in the template context.
    #[serde(skip)]
    pub raw_docstring: Option<String>,
    /// First meaningful line of the docstring, as plain text.
    pub summary: Option<String>,
    pub classes: Vec<PythonClass>,
    pub functions: Vec<PythonFunction>,
    /// Module-level variables, constants and type aliases, in source order.
    pub variables: Vec<PythonVariable>,
    pub z_index: i32,
    /// Display path of the containing folder, ordering prefixes stripped (e.g. "core/sub").
    pub folder: Option<String>,
    /// Raw directory relative to the source root (e.g. "01_core/sub"); the grouping key.
    #[serde(skip)]
    pub dir: String,
    pub link_path: String,
    /// True for doc-only modules (`#pd-write` docstring, no code), rendered with document.html.
    pub is_document: bool,
    /// Names listed in a literal `__all__`, in order; None when the module
    /// does not define one (or builds it dynamically).
    pub all: Option<Vec<String>>,
    /// The module or one of its packages has a name starting with `_`.
    pub is_private: bool,
}

impl PythonModule {
    /// A copy with the same identity (names, paths) and no docs or symbols.
    fn empty_like(&self) -> Self {
        Self {
            name: self.name.clone(),
            source_path: self.source_path.clone(),
            qualname: self.qualname.clone(),
            docstring: None,
            raw_docstring: None,
            summary: None,
            classes: Vec::new(),
            functions: Vec::new(),
            variables: Vec::new(),
            z_index: self.z_index,
            folder: self.folder.clone(),
            dir: self.dir.clone(),
            link_path: self.link_path.clone(),
            is_document: false,
            all: None,
            is_private: self.is_private,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct PythonClass {
    pub name: String,
    /// Dotted name inside its module: "Outer.Inner" for a nested class,
    /// the same as `name` otherwise. Used in anchors and search.
    pub qualname: String,
    pub bases: Vec<String>,
    pub line: usize,
    pub docstring: Option<String>,
    /// The docstring as written; see `PythonModule::raw_docstring`.
    #[serde(skip)]
    pub raw_docstring: Option<String>,
    pub summary: Option<String>,
    /// "conditional" or "type-checking" (see `parser::dedupe`).
    pub badges: Vec<String>,
    pub functions: Vec<PythonFunction>,
    /// Classes defined in the class body.
    pub classes: Vec<PythonClass>,
    /// Class attributes, including enum members and dataclass fields.
    pub variables: Vec<PythonVariable>,
    /// Hidden unless `--include-private` (see `parser::is_public_name`).
    pub is_private: bool,
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
    /// The docstring as written; see `PythonModule::raw_docstring`.
    #[serde(skip)]
    pub raw_docstring: Option<String>,
    pub summary: Option<String>,
    /// Hidden unless `--include-private` (see `parser::is_public_name`).
    pub is_private: bool,
}

/// A module variable or class attribute: `NAME = value`, `name: T = value`,
/// `name: T` or `type Alias = ...`.
#[derive(Debug, Serialize, Clone)]
pub struct PythonVariable {
    pub name: String,
    pub annotation: Option<String>,
    /// The value as written, whitespace collapsed, cut short if it is long.
    pub value: Option<String>,
    /// `value` was cut short; templates mark it with an ellipsis.
    pub value_truncated: bool,
    /// Declared with the `type` statement (PEP 695).
    pub is_type_alias: bool,
    /// "enum" for enum members, "field" for dataclass fields.
    pub badges: Vec<String>,
    pub line: usize,
    /// From a string literal right after the assignment, or `#:` comments.
    pub docstring: Option<String>,
    /// The docstring as written; see `PythonModule::raw_docstring`.
    #[serde(skip)]
    pub raw_docstring: Option<String>,
    pub summary: Option<String>,
    /// Hidden unless `--include-private` (see `parser::is_public_name`).
    pub is_private: bool,
}
