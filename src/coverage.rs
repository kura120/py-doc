//! Documentation coverage: how many public modules, classes, functions and
//! methods have a docstring.
//!
//! "Public" is whatever is left in [`PythonPackage::modules`] after
//! `hide_private` (the `__all__`, underscore and private-module rules), or
//! everything with `--include-private`. "Documented" means a docstring that
//! is not empty once whitespace is stripped. Doc-only `#pd-write` modules are
//! pages, not API, so they are not counted.

use serde::Serialize;
use std::fmt::Write as _;

use crate::generator::is_document_docstring;
use crate::models::{PythonModule, PythonPackage};

/// Version of the JSON report's shape. Bump it when a field is renamed or
/// removed; adding a field does not change it.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Module,
    Class,
    Function,
    Method,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Count {
    pub documented: usize,
    pub total: usize,
}

impl Count {
    fn add(&mut self, documented: bool) {
        self.total += 1;
        if documented {
            self.documented += 1;
        }
    }

    /// 100 when there is nothing to document.
    pub fn percent(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            self.documented as f64 * 100.0 / self.total as f64
        }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct KindCounts {
    pub module: Count,
    pub class: Count,
    pub function: Count,
    pub method: Count,
}

impl KindCounts {
    fn get_mut(&mut self, kind: Kind) -> &mut Count {
        match kind {
            Kind::Module => &mut self.module,
            Kind::Class => &mut self.class,
            Kind::Function => &mut self.function,
            Kind::Method => &mut self.method,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleCoverage {
    /// Dotted name; empty for a root `__init__.py`.
    pub module: String,
    pub path: String,
    pub count: Count,
    /// Qualified names of the undocumented items, in source order.
    pub undocumented: Vec<String>,
}

impl ModuleCoverage {
    /// The name shown in reports: the dotted name, or the file for a root
    /// `__init__.py`, which has none.
    pub fn label(&self) -> &str {
        if self.module.is_empty() {
            &self.path
        } else {
            &self.module
        }
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub include_private: bool,
    pub total: Count,
    pub kinds: KindCounts,
    /// Sorted by module name.
    pub modules: Vec<ModuleCoverage>,
}

fn has_doc(docstring: &Option<String>) -> bool {
    docstring.as_deref().is_some_and(|d| !d.trim().is_empty())
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else {
        format!("{module}.{name}")
    }
}

/// Every countable item of a module, as (kind, qualified name, documented),
/// in source order. Module-level variables (#13) are the next kind to add
/// here, as `Kind::Variable`, once the parser records them.
fn items(module: &PythonModule) -> Vec<(Kind, String, bool)> {
    let base = &module.qualname;
    let mut items = vec![(Kind::Module, base.clone(), has_doc(&module.docstring))];
    for class in &module.classes {
        let class_name = qualify(base, &class.name);
        items.push((Kind::Class, class_name.clone(), has_doc(&class.docstring)));
        for method in &class.functions {
            items.push((
                Kind::Method,
                qualify(&class_name, &method.name),
                has_doc(&method.docstring),
            ));
        }
    }
    for function in &module.functions {
        items.push((
            Kind::Function,
            qualify(base, &function.name),
            has_doc(&function.docstring),
        ));
    }
    items
}

fn is_document(module: &PythonModule) -> bool {
    module.classes.is_empty()
        && module.functions.is_empty()
        && module
            .docstring
            .as_deref()
            .is_some_and(is_document_docstring)
}

impl Report {
    /// Measures `package` as it is: call it after `hide_private` unless
    /// private names should count, and before rendering, which replaces the
    /// docstrings with HTML.
    pub fn of(package: &PythonPackage, include_private: bool) -> Report {
        let mut report = Report {
            include_private,
            total: Count::default(),
            kinds: KindCounts::default(),
            modules: Vec::new(),
        };
        for module in package.modules.iter().filter(|m| !is_document(m)) {
            let mut entry = ModuleCoverage {
                module: module.qualname.clone(),
                path: module.source_path.clone(),
                count: Count::default(),
                undocumented: Vec::new(),
            };
            for (kind, name, documented) in items(module) {
                entry.count.add(documented);
                report.total.add(documented);
                report.kinds.get_mut(kind).add(documented);
                if !documented {
                    entry.undocumented.push(if name.is_empty() {
                        module.source_path.clone()
                    } else {
                        name
                    });
                }
            }
            report.modules.push(entry);
        }
        report
            .modules
            .sort_by(|a, b| a.module.cmp(&b.module).then_with(|| a.path.cmp(&b.path)));
        report
    }

    /// A table, worst-covered module first, then the total.
    pub fn to_text(&self) -> String {
        let mut rows: Vec<&ModuleCoverage> = self.modules.iter().collect();
        rows.sort_by(|a, b| {
            a.count
                .percent()
                .total_cmp(&b.count.percent())
                .then_with(|| a.label().cmp(b.label()))
        });

        let width = rows
            .iter()
            .map(|m| m.label().chars().count())
            .chain(["Module".len(), "Total".len()])
            .max()
            .unwrap_or(0);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<width$}  {:>11}  {:>7}",
            "Module", "Documented", "Cover"
        );
        let _ = writeln!(out, "{}", "-".repeat(width + 22));
        for m in &rows {
            let _ = writeln!(
                out,
                "{:<width$}  {:>11}  {:>6.1}%",
                m.label(),
                format!("{}/{}", m.count.documented, m.count.total),
                m.count.percent()
            );
        }
        let _ = writeln!(out, "{}", "-".repeat(width + 22));
        let _ = writeln!(
            out,
            "{:<width$}  {:>11}  {:>6.1}%",
            "Total",
            format!("{}/{}", self.total.documented, self.total.total),
            self.total.percent()
        );
        out
    }

    /// The JSON report. Its schema is documented in the README.
    pub fn to_json(&self, fail_under: Option<f64>) -> String {
        #[derive(Serialize)]
        struct Totals {
            documented: usize,
            total: usize,
            percent: f64,
        }
        #[derive(Serialize)]
        struct Module<'a> {
            module: &'a str,
            path: &'a str,
            documented: usize,
            total: usize,
            percent: f64,
            undocumented: &'a [String],
        }
        #[derive(Serialize)]
        struct Json<'a> {
            schema_version: u32,
            include_private: bool,
            fail_under: Option<f64>,
            passed: bool,
            totals: Totals,
            kinds: &'a KindCounts,
            modules: Vec<Module<'a>>,
            undocumented: Vec<&'a str>,
        }

        let round = |v: f64| (v * 100.0).round() / 100.0;
        let json = Json {
            schema_version: SCHEMA_VERSION,
            include_private: self.include_private,
            fail_under,
            passed: self.passes(fail_under),
            totals: Totals {
                documented: self.total.documented,
                total: self.total.total,
                percent: round(self.total.percent()),
            },
            kinds: &self.kinds,
            modules: self
                .modules
                .iter()
                .map(|m| Module {
                    module: &m.module,
                    path: &m.path,
                    documented: m.count.documented,
                    total: m.count.total,
                    percent: round(m.count.percent()),
                    undocumented: &m.undocumented,
                })
                .collect(),
            undocumented: self
                .modules
                .iter()
                .flat_map(|m| m.undocumented.iter().map(String::as_str))
                .collect(),
        };
        serde_json::to_string_pretty(&json).unwrap_or_default()
    }

    pub fn passes(&self, fail_under: Option<f64>) -> bool {
        fail_under.is_none_or(|threshold| self.total.percent() >= threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PythonClass, PythonFunction};

    fn function(name: &str, doc: Option<&str>) -> PythonFunction {
        PythonFunction {
            name: name.into(),
            args: vec![],
            return_type: None,
            decorators: vec![],
            badges: vec![],
            is_async: false,
            line: 1,
            docstring: doc.map(str::to_string),
            raw_docstring: None,
            summary: None,
            is_private: false,
        }
    }

    fn module(qualname: &str, doc: Option<&str>) -> PythonModule {
        PythonModule {
            name: qualname.rsplit('.').next().unwrap_or("").into(),
            source_path: format!("{}.py", qualname.replace('.', "/")),
            qualname: qualname.into(),
            docstring: doc.map(str::to_string),
            raw_docstring: None,
            summary: None,
            classes: vec![],
            functions: vec![],
            variables: vec![],
            z_index: 0,
            folder: None,
            dir: String::new(),
            link_path: String::new(),
            is_document: false,
            all: None,
            is_private: false,
        }
    }

    fn package(modules: Vec<PythonModule>) -> PythonPackage {
        PythonPackage {
            name: "p".into(),
            modules,
            hidden: vec![],
        }
    }

    #[test]
    fn counts_every_kind_and_names_what_is_missing() {
        let mut m = module("pkg.core", Some("Core."));
        m.classes.push(PythonClass {
            name: "Engine".into(),
            bases: vec![],
            line: 1,
            docstring: None,
            raw_docstring: None,
            summary: None,
            functions: vec![function("run", Some("Runs.")), function("stop", None)],
            variables: vec![],
            is_private: false,
        });
        m.functions.push(function("helper", Some("  \n\t ")));
        let report = Report::of(&package(vec![m]), false);

        assert_eq!(
            report.total,
            Count {
                documented: 2,
                total: 5
            }
        );
        assert_eq!(
            report.kinds.module,
            Count {
                documented: 1,
                total: 1
            }
        );
        assert_eq!(
            report.kinds.class,
            Count {
                documented: 0,
                total: 1
            }
        );
        assert_eq!(
            report.kinds.method,
            Count {
                documented: 1,
                total: 2
            }
        );
        assert_eq!(
            report.kinds.function,
            Count {
                documented: 0,
                total: 1
            }
        );
        assert_eq!(
            report.modules[0].undocumented,
            ["pkg.core.Engine", "pkg.core.Engine.stop", "pkg.core.helper"]
        );
        assert!((report.total.percent() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn doc_only_pages_are_not_counted() {
        let guide = module("guide", Some("#pd-z-index: 1\n#pd-write\n# Guide"));
        let report = Report::of(&package(vec![guide, module("api", None)]), false);
        assert_eq!(report.modules.len(), 1);
        assert_eq!(
            report.total,
            Count {
                documented: 0,
                total: 1
            }
        );
    }

    #[test]
    fn empty_package_is_fully_covered() {
        let report = Report::of(&package(vec![]), false);
        assert_eq!(report.total.percent(), 100.0);
        assert!(report.passes(Some(100.0)));
    }

    #[test]
    fn threshold_is_inclusive() {
        let mut m = module("a", Some("A."));
        m.functions.push(function("f", None));
        let report = Report::of(&package(vec![m]), false);
        assert!(report.passes(Some(50.0)));
        assert!(!report.passes(Some(50.01)));
        assert!(report.passes(None));
    }

    #[test]
    fn text_puts_the_worst_module_first() {
        let mut low = module("b_low", None);
        low.functions.push(function("f", None));
        let report = Report::of(&package(vec![module("a_full", Some("A.")), low]), false);
        let text = report.to_text();
        let low_at = text.find("b_low").unwrap();
        let full_at = text.find("a_full").unwrap();
        assert!(low_at < full_at, "{text}");
        assert!(text.trim_end().ends_with("33.3%"), "{text}");
    }

    #[test]
    fn root_init_is_labelled_by_its_file() {
        let mut root = module("", None);
        root.source_path = "__init__.py".into();
        root.functions.push(function("top", None));
        let report = Report::of(&package(vec![root]), false);
        assert_eq!(report.modules[0].label(), "__init__.py");
        assert_eq!(report.modules[0].undocumented, ["__init__.py", "top"]);
    }
}
