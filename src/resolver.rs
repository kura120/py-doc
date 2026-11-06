use crate::models::{PythonClass, PythonModule, PythonPackage};
use std::collections::HashMap;

struct ModuleEntry {
    /// Dotted paths this module answers to: with and without ordering
    /// prefixes ("01_core.engine" and "core.engine").
    paths: Vec<String>,
    link_path: String,
    /// Dotted path inside the module -> anchor kind; the anchor is
    /// "<kind>.<path>": "fn.f", "class.Outer.Inner", "method.Outer.run".
    symbols: HashMap<String, &'static str>,
}

/// Adds a class, its members and its nested classes to `symbols`. Members
/// are inserted before the nested classes, and attributes before methods,
/// so a class wins over a method of the same name, and a method over an
/// attribute.
fn add_class(symbols: &mut HashMap<String, &'static str>, class: &PythonClass) {
    let qualname = if class.qualname.is_empty() {
        &class.name
    } else {
        &class.qualname
    };
    for v in &class.variables {
        symbols.insert(format!("{}.{}", qualname, v.name), "attr");
    }
    for f in &class.functions {
        symbols.insert(format!("{}.{}", qualname, f.name), "method");
    }
    for nested in &class.classes {
        add_class(symbols, nested);
    }
    symbols.insert(qualname.clone(), "class");
}

/// Resolves `#pd-doc-link` targets against the modules that were actually parsed.
pub struct Resolver {
    package_name: String,
    modules: Vec<ModuleEntry>,
    /// The same lookup over the private items left out of the docs, used only
    /// to tell "hidden" apart from "does not exist". Never produces links.
    hidden: Option<Box<Resolver>>,
}

fn module_entries(modules: &[PythonModule]) -> Vec<ModuleEntry> {
    modules
        .iter()
        .map(|module| {
            let raw = module
                .source_path
                .strip_suffix(".py")
                .unwrap_or(&module.source_path)
                .replace('/', ".");
            let raw = raw
                .strip_suffix(".__init__")
                .map(str::to_string)
                .unwrap_or(raw);

            let mut paths = vec![module.qualname.clone()];
            if raw != module.qualname {
                paths.push(raw);
            }

            // Variables first, then functions, then classes: on a clash the
            // later kind wins, as before.
            let mut symbols = HashMap::new();
            for v in &module.variables {
                symbols.insert(v.name.clone(), "var");
            }
            for f in &module.functions {
                symbols.insert(f.name.clone(), "fn");
            }
            for class in &module.classes {
                add_class(&mut symbols, class);
            }

            ModuleEntry {
                paths,
                link_path: module.link_path.clone(),
                symbols,
            }
        })
        .collect()
}

impl Resolver {
    /// Only `package.modules` can be linked to; `package.hidden` is kept
    /// so that a link to a private symbol can be reported as such.
    pub fn build(package: &PythonPackage) -> Self {
        let hidden = (!package.hidden.is_empty()).then(|| {
            Box::new(Self {
                package_name: package.name.clone(),
                modules: module_entries(&package.hidden),
                hidden: None,
            })
        });
        Self {
            package_name: package.name.clone(),
            modules: module_entries(&package.modules),
            hidden,
        }
    }

    /// True when `target` does not resolve to a documented symbol but names
    /// a private one that was hidden from the docs.
    pub fn is_hidden(&self, target: &str) -> bool {
        self.resolve(target).is_none()
            && self
                .hidden
                .as_ref()
                .is_some_and(|hidden| hidden.resolve(target).is_some())
    }

    /// Finds the one module a dotted path refers to. An exact match wins;
    /// otherwise a unique trailing match ("engine" for "core.engine") is accepted.
    fn find_module(&self, path: &str) -> Option<&ModuleEntry> {
        if path.is_empty() || path == "__init__" {
            return None;
        }
        if let Some(exact) = self
            .modules
            .iter()
            .find(|m| m.paths.iter().any(|p| p == path))
        {
            return Some(exact);
        }

        let suffix = format!(".{}", path);
        let mut candidates = self
            .modules
            .iter()
            .filter(|m| m.paths.iter().any(|p| p.ends_with(&suffix)));
        let first = candidates.next()?;
        candidates.next().is_none().then_some(first)
    }

    fn resolve_inner(&self, target: &str) -> Option<String> {
        // module
        if let Some(module) = self.find_module(target) {
            return Some(module.link_path.clone());
        }

        // module.symbol, module.Class.member, module.Outer.Inner, ...: the
        // longest module path that matches wins.
        let mut split = target.len();
        while let Some(dot) = target[..split].rfind('.') {
            let (module_path, symbol) = (&target[..dot], &target[dot + 1..]);
            if let Some(module) = self.find_module(module_path)
                && let Some(kind) = module.symbols.get(symbol)
            {
                return Some(format!("{}#{}.{}", module.link_path, kind, symbol));
            }
            split = dot;
        }

        // A bare symbol path ("slugify", "Engine.run", "Outer.Inner"), when
        // exactly one module defines it.
        let mut hits = self.modules.iter().filter_map(|module| {
            module
                .symbols
                .get(target)
                .map(|kind| format!("{}#{}.{}", module.link_path, kind, target))
        });
        let first = hits.next()?;
        hits.next().is_none().then_some(first)
    }

    /// Returns the link (relative to the site root) for a dotted reference
    /// such as `core.engine`, `engine.Engine` or `helpers.slugify`.
    pub fn resolve(&self, target: &str) -> Option<String> {
        let target = target.trim();
        self.resolve_inner(target).or_else(|| {
            let prefix = format!("{}.", self.package_name);
            target
                .strip_prefix(&prefix)
                .and_then(|rest| self.resolve_inner(rest))
        })
    }
}
