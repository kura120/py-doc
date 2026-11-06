//! Module variables, class attributes, dataclass fields and enum members.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn generate(test: &str, extra: &[&str]) -> PathBuf {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("variables")
        .join(test);
    let _ = fs::remove_dir_all(&out);
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("variables");
    // An empty --config keeps the repository's own pyproject.toml (the PyPI
    // packaging metadata) out of these runs.
    let empty_config = Path::new(env!("CARGO_TARGET_TMPDIR")).join("empty-pyproject.toml");
    if !empty_config.exists() {
        fs::write(&empty_config, "").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--config")
        .arg(&empty_config)
        .arg("--src")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .args(["--name", "demo", "--strict"])
        .args(extra)
        .output()
        .expect("failed to run py-doc");
    assert!(
        output.status.success(),
        "py-doc failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {}: {}", file, e))
}

/// The text of the signature in the card with `id`, tags removed.
fn signature(html: &str, id: &str) -> String {
    let start = html
        .find(&format!("id=\"{}\"", id))
        .unwrap_or_else(|| panic!("no {}", id));
    let rest = &html[start..];
    let code = rest.find("class=\"signature-code\">").unwrap() + 23;
    let end = rest[code..]
        .find("<span class=\"signature-meta\">")
        .unwrap();
    let mut text = String::new();
    let mut in_tag = false;
    for c in rest[code..code + end].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("&quot;", "\"")
}

#[test]
fn module_variables_render_in_every_form() {
    let out = generate("module", &[]);
    let page = read(&out, "config.html");
    assert!(page.contains("<h2>Variables</h2>"));
    assert_eq!(signature(&page, "var.MAX_RETRIES"), "MAX_RETRIES = 3");
    assert_eq!(signature(&page, "var.TIMEOUT"), "TIMEOUT: float = 2.5");
    assert_eq!(signature(&page, "var.pending"), "pending: int");
    assert_eq!(signature(&page, "var.Number"), "Number = Union[int, float]");
    assert_eq!(
        signature(&page, "var.JsonValue"),
        "type JsonValue = dict[str, \"JsonValue\"] | list[\"JsonValue\"] | str | int | None"
    );
    assert_eq!(signature(&page, "var.first"), "first");
    // Not in `__all__`, and `__all__` itself is never documented.
    assert!(!page.contains("NOT_EXPORTED"));
    assert!(!page.contains("var.__all__"));
}

#[test]
fn long_values_are_truncated_and_marked() {
    let out = generate("truncated", &[]);
    let page = read(&out, "config.html");
    let palette = signature(&page, "var.PALETTE");
    assert!(palette.starts_with("PALETTE = {\"red\": \"#ff0000\""));
    assert!(palette.ends_with('…'), "{}", palette);
    assert!(!palette.contains("#ffffff"));
    assert!(page.contains("class=\"value-truncated\""));
    assert!(!signature(&page, "var.MAX_RETRIES").contains('…'));
}

#[test]
fn attribute_docstrings_and_comments() {
    let out = generate("docs", &[]);
    let page = read(&out, "config.html");
    for doc in [
        "How often a request is retried.",
        "Seconds to wait for a reply.",
        "Applies to every request.",
        "Name used when none is given.",
        "Horizontal position.",
        "Go fast.",
        "Where logs go.",
    ] {
        assert!(page.contains(doc), "missing docstring {:?}", doc);
    }
}

#[test]
fn enum_members_and_dataclass_fields_get_badges() {
    let out = generate("badges", &[]);
    let page = read(&out, "config.html");
    // The badges shown in the meta part of the card with `id`.
    let badges = |id: &str| {
        let start = page.find(&format!("id=\"{}\"", id)).unwrap();
        let rest = &page[start..];
        let meta = rest.find("signature-meta").unwrap();
        let end = meta + rest[meta..].find("source-ref").unwrap();
        ["enum", "field"]
            .into_iter()
            .filter(|b| rest[meta..end].contains(&format!("<span class=\"badge\">{}</span>", b)))
            .collect::<Vec<_>>()
    };
    assert!(page.contains("<h3 class=\"method-list-title\">Attributes</h3>"));
    assert_eq!(badges("attr.Mode.FAST"), ["enum"]);
    assert_eq!(badges("attr.Level.LOW"), ["enum"]);
    assert_eq!(badges("attr.Point.x"), ["field"]);
    assert_eq!(badges("attr.Point.tags"), ["field"]);
    assert!(
        badges("attr.Point.origin").is_empty(),
        "ClassVar is not a field"
    );
    assert!(badges("attr.Settings.debug").is_empty());
    // Private names and noisy dunders are left out.
    for hidden in [
        "_ignore_",
        "_cache",
        "__slots__",
        "__match_args__",
        "__hash__",
    ] {
        assert!(!page.contains(hidden), "{} shown", hidden);
    }
}

#[test]
fn variables_are_searchable_and_in_the_outline() {
    let out = generate("search", &[]);
    let search = read(&out, "search-index.js");
    for name in [
        "\"MAX_RETRIES\"",
        "\"JsonValue\"",
        "\"FAST\"",
        "\"log_path\"",
    ] {
        assert!(search.contains(name), "{} not in search", name);
    }
    assert!(search.contains("\"summary\":\"How often a request is retried.\""));
    assert!(read(&out, "app.js").contains("#var.${v.name}"));

    let page = read(&out, "config.html");
    let outline = &page[page.find("class=\"page-outline\"").unwrap()..];
    for link in ["#var.TIMEOUT", "#attr.Point.x", "#attr.Mode.FAST"] {
        assert!(outline.contains(&format!("href=\"{}\"", link)), "{}", link);
    }
}

#[test]
fn doc_links_reach_variables() {
    // --strict: an unresolved link would fail the run.
    let out = generate("links", &[]);
    let page = read(&out, "links.html");
    for link in [
        "./config.html#var.TIMEOUT",
        "./config.html#attr.Point.x",
        "./config.html#attr.Mode.FAST",
    ] {
        assert!(page.contains(&format!("href=\"{}\"", link)), "{}", link);
    }
}

#[test]
fn include_private_shows_private_variables_but_not_noise() {
    let out = generate("private", &["--include-private"]);
    let page = read(&out, "config.html");
    for shown in [
        "var.NOT_EXPORTED",
        "attr.Point._cache",
        "attr.Mode._ignore_",
        "attr.Settings.__hash__",
    ] {
        assert!(
            page.contains(&format!("id=\"{}\"", shown)),
            "{} missing",
            shown
        );
    }
    for never in ["__slots__", "__match_args__", "var.__all__"] {
        assert!(!page.contains(never), "{} shown", never);
    }
}
