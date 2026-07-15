use crate::models::PythonPackage;
use std::collections::HashMap;

pub struct Resolver {
    pub symbol_map: HashMap<String, String>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            symbol_map: HashMap::new(),
        }
    }

    /// Indexes all modules, classes, and functions to build a global symbol map.
    pub fn build_index(&mut self, package: &PythonPackage) {
        for module in &package.modules {
            let mod_path = format!("{}.{}", package.name, module.name);
            self.symbol_map.insert(module.name.clone(), format!("{}.html", mod_path));

            for class in &module.classes {
                // Now using `class_path` to build the correct internal documentation anchor!
                let class_path = format!("{}.html#class.{}", mod_path, class.name);
                self.symbol_map.insert(class.name.clone(), class_path);
            }
        }
    }

    /// Suppress the dead_code warning until we wire this into the template generator
    #[allow(dead_code)]
    pub fn resolve_link(&self, type_hint: &str) -> Option<String> {
        self.symbol_map.get(type_hint).cloned()
    }
}