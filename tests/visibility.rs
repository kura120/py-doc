//! Public API detection: `__all__`, private names, private modules and
//! packages, and `--include-private`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn out_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("visibility")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// An empty pyproject.toml passed as `--config`, so these runs never pick
/// up the repository's own pyproject.toml (the PyPI packaging metadata).
fn empty_config() -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("empty-pyproject.toml");
    if !path.exists() {
        fs::write(&path, "").unwrap();
    }
    path
}

fn run(src: &Path, out: &Path, extra: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .arg("--config")
        .arg(empty_config())
        .arg("--src")
        .arg(src)
        .arg("--out")
        .arg(out)
        .args(["--name", "demo"])
        .args(extra)
        .output()
        .expect("failed to run swiftdoc");
    assert!(
        output.status.success(),
        "swiftdoc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {}: {}", file, e))
}

/// Every file under `dir`, keyed by its path relative to `dir`.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let key = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(key, fs::read(&path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

fn pages(dir: &Path) -> Vec<String> {
    tree(dir)
        .into_keys()
        .filter(|k| k.ends_with(".html"))
        .collect()
}

#[test]
fn private_modules_and_packages_get_no_page() {
    let out = out_dir("pages");
    run(&fixture("visibility"), &out, &[]);
    assert_eq!(
        pages(&out),
        [
            "__init__.html",
            "api.html",
            "index.html",
            "plain.html",
            "shapes/circle.html",
            "shapes/index.html",
        ]
    );
    let nav = read(&out, "nav-data.js");
    let search = read(&out, "search-index.js");
    for hidden in ["_helpers", "_internal", "normalize", "boot", "\"run\""] {
        assert!(!nav.contains(hidden), "{} in nav", hidden);
        assert!(!search.contains(hidden), "{} in search", hidden);
    }
}

#[test]
fn dunder_all_decides_a_modules_public_names() {
    let out = out_dir("all");
    run(&fixture("visibility"), &out, &[]);
    let api = read(&out, "api.html");
    // Listed in `__all__`, including through `+=` and `.extend`.
    for shown in [
        "fn.connect",
        "class.Client",
        "class.Pool",
        "fn._exported_private",
    ] {
        assert!(
            api.contains(&format!("id=\"{}\"", shown)),
            "{} missing",
            shown
        );
    }
    // Left out of `__all__`, whatever their names.
    for hidden in ["fn.not_exported", "fn._make_pool", "class.Hidden"] {
        assert!(
            !api.contains(&format!("id=\"{}\"", hidden)),
            "{} shown",
            hidden
        );
    }
    assert!(!api.contains("not_exported"));
    // Members of an exported class follow the underscore rule.
    for shown in [
        "method.Client.__init__",
        "method.Client.__eq__",
        "method.Client.send",
    ] {
        assert!(
            api.contains(&format!("id=\"{}\"", shown)),
            "{} missing",
            shown
        );
    }
    assert!(!api.contains("_retry"));

    let search = read(&out, "search-index.js");
    assert!(search.contains("_exported_private"));
    assert!(!search.contains("not_exported"));
}

#[test]
fn without_dunder_all_underscores_are_private_but_dunders_are_not() {
    let out = out_dir("plain");
    run(&fixture("visibility"), &out, &[]);
    let plain = read(&out, "plain.html");
    for shown in ["fn.visible", "class.Widget", "method.Widget.__call__"] {
        assert!(
            plain.contains(&format!("id=\"{}\"", shown)),
            "{} missing",
            shown
        );
    }
    for hidden in ["_secret", "_paint", "_Base"] {
        assert!(!plain.contains(hidden), "{} shown", hidden);
    }
}

#[test]
fn doc_links_to_hidden_symbols_warn_instead_of_linking() {
    let out = out_dir("links");
    let output = run(&fixture("visibility"), &out, &[]);
    let stderr = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    for target in ["api._make_pool", "plain._secret", "_helpers.normalize"] {
        assert!(
            stderr.contains(&format!("'{}' is private and hidden", target)),
            "no warning for {}:\n{}",
            target,
            stderr
        );
    }
    let api = read(&out, "api.html");
    assert!(api.contains("href=\"./api.html#fn.connect\""));
    assert!(!api.contains("#fn._make_pool"));
    assert!(!api.contains("#fn._secret"));
    assert!(!api.contains("_helpers.html"));

    // The warnings count as problems under --strict.
    let strict = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .arg("--config")
        .arg(empty_config())
        .arg("--src")
        .arg(fixture("visibility"))
        .arg("--out")
        .arg(out_dir("links-strict"))
        .args(["--name", "demo", "--strict"])
        .output()
        .unwrap();
    assert!(!strict.status.success());
}

/// `--include-private` must reproduce the output swiftdoc gave before private
/// items were hidden. The golden files were generated by the release before
/// this feature, from the same fixture.
#[test]
fn include_private_matches_the_previous_output() {
    let out = out_dir("include-private");
    let output = run(&fixture("visibility"), &out, &["--include-private"]);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("⚠"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("⚠"));

    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("visibility-include-private");
    for file in ["nav-data.js", "search-index.js"] {
        assert_eq!(
            read(&out, file),
            read(&golden, file),
            "{} differs from the previous output",
            file
        );
    }
    assert_eq!(
        pages(&out),
        [
            "__init__.html",
            "_helpers.html",
            "_internal/core.html",
            "_internal/index.html",
            "api.html",
            "index.html",
            "plain.html",
            "shapes/circle.html",
            "shapes/index.html",
        ]
    );
}

/// On a project with no private names, hiding changes nothing: the default
/// and `--include-private` sites are byte-identical.
#[test]
fn projects_without_private_names_are_unchanged() {
    let default = out_dir("sample-default");
    let all = out_dir("sample-all");
    run(&fixture("sample"), &default, &[]);
    run(&fixture("sample"), &all, &["--include-private"]);
    let (default, all) = (tree(&default), tree(&all));
    assert_eq!(
        default.keys().collect::<Vec<_>>(),
        all.keys().collect::<Vec<_>>()
    );
    for (file, body) in &default {
        // The manifest lists the same files but is not byte-stable, and the
        // build cache's key includes the option.
        if file == ".swiftdoc-manifest" || file == ".swiftdoc-cache" {
            continue;
        }
        assert!(body == &all[file], "{} differs", file);
    }
}
