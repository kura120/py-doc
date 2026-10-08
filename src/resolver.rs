use crate::models::PythonPackage;
use std::collections::{HashMap, HashSet};

struct ModuleEntry {
    /// Dotted paths this module answers to: with and without ordering
    /// prefixes ("01_core.engine" and "core.engine").
    paths: Vec<String>,
    link_path: String,
    functions: HashSet<String>,
    /// Class name -> method names.
    classes: HashMap<String, HashSet<String>>,
}

/// Resolves `#pd-doc-link` targets against the modules that were actually parsed.
pub struct Resolver {
    package_name: String,
    modules: Vec<ModuleEntry>,
}

impl Resolver {
    pub fn build(package: &PythonPackage) -> Self {
        let modules = package
            .modules
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

                ModuleEntry {
                    paths,
                    link_path: module.link_path.clone(),
                    functions: module.functions.iter().map(|f| f.name.clone()).collect(),
                    classes: module
                        .classes
                        .iter()
                        .map(|c| {
                            (
                                c.name.clone(),
                                c.functions.iter().map(|f| f.name.clone()).collect(),
                            )
                        })
                        .collect(),
                }
            })
            .collect();

        Self {
            package_name: package.name.clone(),
            modules,
        }
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

        // module.symbol
        if let Some((module_path, symbol)) = target.rsplit_once('.') {
            if let Some(module) = self.find_module(module_path) {
                if module.classes.contains_key(symbol) {
                    return Some(format!("{}#class.{}", module.link_path, symbol));
                }
                if module.functions.contains(symbol) {
                    return Some(format!("{}#fn.{}", module.link_path, symbol));
                }
            }

            // module.Class.method
            if let Some((module_path, class)) = module_path.rsplit_once('.')
                && let Some(module) = self.find_module(module_path)
                && module
                    .classes
                    .get(class)
                    .is_some_and(|m| m.contains(symbol))
            {
                return Some(format!("{}#method.{}.{}", module.link_path, class, symbol));
            }
        }

        // Bare "symbol" or "Class.method", when exactly one module defines it.
        let (class, member) = match target.split_once('.') {
            Some((class, member)) if !member.contains('.') => (class, Some(member)),
            Some(_) => return None,
            None => (target, None),
        };
        let mut hits = self.modules.iter().filter_map(|module| match member {
            Some(member) => module
                .classes
                .get(class)
                .filter(|methods| methods.contains(member))
                .map(|_| format!("{}#method.{}.{}", module.link_path, class, member)),
            None if module.classes.contains_key(class) => {
                Some(format!("{}#class.{}", module.link_path, class))
            }
            None if module.functions.contains(class) => {
                Some(format!("{}#fn.{}", module.link_path, class))
            }
            None => None,
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
