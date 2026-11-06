//! Nested classes, and definitions inside if / try / with / TYPE_CHECKING blocks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Generates the `nested` fixture; returns the output dir and the run's
/// stdout + stderr.
fn generate(test: &str, extra: &[&str]) -> (PathBuf, String) {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("nested")
        .join(test);
    let _ = fs::remove_dir_all(&out);
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("nested");
    let output = Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--src")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .args(["--name", "demo"])
        .args(extra)
        .output()
        .expect("failed to run py-doc");
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "py-doc failed:\n{}", log);
    (out, log)
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {}: {}", file, e))
}

/// The badges shown on the card with `id`.
fn badges(page: &str, id: &str) -> Vec<String> {
    let start = page
        .find(&format!("id=\"{}\"", id))
        .unwrap_or_else(|| panic!("no {}", id));
    let rest = &page[start..];
    let meta = rest.find("signature-meta").unwrap();
    let end = meta + rest[meta..].find("source-ref").unwrap();
    rest[meta..end]
        .split("<span class=\"badge\">")
        .skip(1)
        .map(|b| b.split('<').next().unwrap().to_string())
        .collect()
}

fn ids(page: &str, prefix: &str) -> Vec<String> {
    page.match_indices(&format!("id=\"{}", prefix))
        .map(|(at, _)| {
            let rest = &page[at + 4..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

#[test]
fn definitions_in_branches_are_documented_once() {
    let (out, _) = generate("branches", &[]);
    let page = read(&out, "compat.html");
    assert_eq!(
        ids(&page, "fn."),
        [
            "fn.runtime_twin",
            "fn.read_config",
            "fn.dumps",
            "fn.scale",
            "fn.scale",
            "fn.scale",
            "fn.factory"
        ]
    );
    assert_eq!(
        ids(&page, "var."),
        [
            "var.Handler",
            "var.MODE",
            "var.BACKEND",
            "var.CLEANED_UP",
            "var.SOURCE_LENGTH"
        ]
    );
    // The first branch wins; later ones are dropped.
    assert!(page.contains("Uses tomllib."));
    assert!(!page.contains("dropped"));
    assert_eq!(badges(&page, "fn.read_config"), ["conditional"]);
    assert_eq!(badges(&page, "var.MODE"), ["conditional"]);
    assert_eq!(badges(&page, "fn.dumps"), ["conditional"]);
    assert_eq!(badges(&page, "var.BACKEND"), ["conditional"]);
    // `finally` and `with` always run.
    assert!(badges(&page, "var.CLEANED_UP").is_empty());
    assert!(badges(&page, "var.SOURCE_LENGTH").is_empty());
}

#[test]
fn type_checking_definitions_do_not_replace_runtime_ones() {
    let (out, _) = generate("type_checking", &[]);
    let page = read(&out, "compat.html");
    assert_eq!(badges(&page, "var.Handler"), ["type-checking"]);
    assert!(page.contains("Only for type checkers."));
    assert!(page.contains("The runtime definition."));
    assert!(!page.contains("Type-checking stub"));
    assert!(badges(&page, "fn.runtime_twin").is_empty());
}

#[test]
fn function_bodies_and_main_blocks_are_skipped() {
    let (out, _) = generate("skipped", &[]);
    let page = read(&out, "compat.html");
    for never in [
        "inner_helper",
        "InnerClass",
        "Script code",
        "SCRIPT_ONLY",
        "fn.main",
    ] {
        assert!(!page.contains(never), "{} documented", never);
    }
    // Overloads in one suite are all kept.
    assert_eq!(badges(&page, "fn.scale"), ["overload"]);
}

#[test]
fn nested_classes_render_inside_their_parent() {
    let (out, _) = generate("nested_classes", &[]);
    let page = read(&out, "compat.html");
    let outer = &page[page.find("id=\"class.Outer\"").unwrap()..];
    let outer = &outer[..outer.find("id=\"fn.runtime_twin\"").unwrap_or(outer.len())];
    for id in [
        "class.Outer.Inner",
        "attr.Outer.Inner.LIMIT",
        "class.Outer.Inner.Deep",
        "method.Outer.Inner.Deep.ping",
        "class.Outer.Meta",
        "attr.Outer.Meta.ordering",
        "method.Outer.path",
    ] {
        assert!(
            outer.contains(&format!("id=\"{}\"", id)),
            "{} not in Outer",
            id
        );
    }
    assert!(page.contains("Nested classes"));
    assert!(page.contains("Windows."));
    assert_eq!(badges(&page, "method.Outer.path"), ["conditional"]);
    assert!(!page.contains("id=\"class.Outer._Private\""));
    assert!(!page.contains("Hidden by default."));

    let outline = &page[page.find("class=\"page-outline\"").unwrap()..];
    for link in [
        "#class.Outer.Inner",
        "#class.Outer.Inner.Deep",
        "#method.Outer.Inner.Deep.ping",
        "#attr.Outer.Inner.LIMIT",
    ] {
        assert!(outline.contains(&format!("href=\"{}\"", link)), "{}", link);
    }
    assert!(outline.contains(">Outer.Inner.Deep<"));

    let search = read(&out, "search-index.js");
    for name in ["\"Outer.Inner\"", "\"Outer.Inner.Deep\"", "\"Outer.Meta\""] {
        assert!(search.contains(name), "{} not in search", name);
    }
    assert!(!search.contains("_Private"));
}

#[test]
fn nested_classes_are_linkable() {
    let (out, log) = generate("links", &[]);
    let page = read(&out, "compat.html");
    for link in [
        "./compat.html#class.Outer.Inner",
        "./compat.html#method.Outer.Inner.Deep.ping",
        "./compat.html#attr.Outer.Inner.LIMIT",
        "./compat.html#class.Outer.Meta",
    ] {
        assert!(page.contains(&format!("href=\"{}\"", link)), "{}", link);
    }
    assert!(!log.contains("was not found"), "{}", log);
    assert!(
        log.contains("'compat.Outer._Private' is private and hidden"),
        "{}",
        log
    );
}

#[test]
fn include_private_shows_private_nested_classes() {
    let (out, log) = generate("private", &["--include-private", "--strict"]);
    let page = read(&out, "compat.html");
    assert!(page.contains("id=\"class.Outer._Private\""));
    assert!(page.contains("href=\"./compat.html#class.Outer._Private\""));
    assert!(!log.contains('⚠'), "{}", log);
}
