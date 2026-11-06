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

impl PythonPackage {
    /// Moves every private module, class and function out of `modules` and
    /// into `hidden`, so pages, navigation and search only see the public API.
    pub fn hide_private(&mut self) {
        let mut public = Vec::with_capacity(self.modules.len());
        for mut module in std::mem::take(&mut self.modules) {
            if module.is_private {
                self.hidden.push(module);
                continue;
            }

            let mut stub = module.empty_like();
            let (classes, hidden_classes) = module.classes.into_iter().partition(|c| !c.is_private);
            module.classes = classes;
            stub.classes = hidden_classes;
            for class in module.classes.iter_mut() {
                let (functions, hidden_methods): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut class.functions)
                        .into_iter()
                        .partition(|f| !f.is_private);
                class.functions = functions;
                if !hidden_methods.is_empty() {
                    let mut class_stub = class.clone();
                    class_stub.functions = hidden_methods;
                    stub.classes.push(class_stub);
                }
            }
            let (functions, hidden_functions) =
                module.functions.into_iter().partition(|f| !f.is_private);
            module.functions = functions;
            stub.functions = hidden_functions;

            if !stub.classes.is_empty() || !stub.functions.is_empty() {
                self.hidden.push(stub);
            }
            public.push(module);
        }
        self.modules = public;
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
            summary: None,
            classes: Vec::new(),
            functions: Vec::new(),
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
    pub bases: Vec<String>,
    pub line: usize,
    pub docstring: Option<String>,
    pub summary: Option<String>,
    pub functions: Vec<PythonFunction>,
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
    pub summary: Option<String>,
    /// Hidden unless `--include-private` (see `parser::is_public_name`).
    pub is_private: bool,
}
