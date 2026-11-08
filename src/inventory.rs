//! Sphinx `objects.inv` inventories (version 2), so Sphinx projects (and
//! anything else that reads intersphinx inventories) can link to a
//! swiftdoc site.

use std::fmt::Write as _;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::models::{PythonClass, PythonPackage};

pub const FILE_NAME: &str = "objects.inv";

/// The inventory of `package`. `import_prefix` is the import name of the
/// source root ("pkg" when `--src` is a package folder, "" otherwise), so
/// names are the ones users import: `pkg.engine.Engine`.
pub fn build(package: &PythonPackage, version: &str, import_prefix: &str) -> Vec<u8> {
    let mut lines = String::new();
    let mut add = |name: &str, role: &str, uri: String| {
        // One entry per line, space-separated: names never contain spaces.
        let _ = writeln!(lines, "{name} py:{role} 1 {uri} -");
    };
    let join = |module: &str, rest: &str| -> String {
        [import_prefix, module, rest]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(".")
    };

    for module in package.modules.iter().filter(|m| !m.is_document) {
        let page = &module.link_path;
        let module_name = join(&module.qualname, "");
        if !module_name.is_empty() {
            add(&module_name, "module", page.clone());
        }
        for var in &module.variables {
            add(
                &join(&module.qualname, &var.name),
                "data",
                format!("{page}#var.{}", var.name),
            );
        }
        for func in &module.functions {
            add(
                &join(&module.qualname, &func.name),
                "function",
                format!("{page}#fn.{}", func.name),
            );
        }
        for class in &module.classes {
            add_class(&mut add, &join, &module.qualname, page, class);
        }
    }

    // The fastest level: on SQLAlchemy it takes 3 ms instead of 20 for a
    // file a third larger (150 KB).
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(lines.as_bytes())
        .expect("writing to memory cannot fail");
    let compressed = encoder.finish().expect("writing to memory cannot fail");

    let mut out = format!(
        "# Sphinx inventory version 2\n\
         # Project: {}\n\
         # Version: {}\n\
         # The remainder of this file is compressed using zlib.\n",
        package.name, version
    )
    .into_bytes();
    out.extend(compressed);
    out
}

fn add_class(
    add: &mut impl FnMut(&str, &str, String),
    join: &impl Fn(&str, &str) -> String,
    module: &str,
    page: &str,
    class: &PythonClass,
) {
    let qualname = if class.qualname.is_empty() {
        &class.name
    } else {
        &class.qualname
    };
    add(
        &join(module, qualname),
        "class",
        format!("{page}#class.{qualname}"),
    );
    for var in &class.variables {
        add(
            &join(module, &format!("{qualname}.{}", var.name)),
            "attribute",
            format!("{page}#attr.{qualname}.{}", var.name),
        );
    }
    for func in &class.functions {
        add(
            &join(module, &format!("{qualname}.{}", func.name)),
            "method",
            format!("{page}#method.{qualname}.{}", func.name),
        );
    }
    for nested in &class.classes {
        add_class(add, join, module, page, nested);
    }
}
