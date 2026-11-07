//! End-to-end tests for `py-doc theme eject`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The bundled theme files, as they are in `assets/`.
const THEME_FILES: [&str; 7] = [
    "base.html",
    "sidebar.html",
    "index.html",
    "module.html",
    "document.html",
    "style.css",
    "app.js",
];

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh, empty directory unique to one test.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("theme")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn py_doc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("failed to run py-doc")
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "py-doc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Every file under `dir`, as sorted site-relative paths.
fn files(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, found);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// Same whitespace normalization as the snapshot tests in site.rs.
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn ejected_files_are_the_bundled_theme_byte_for_byte() {
    let cwd = scratch("byte_identical");
    assert_ok(&py_doc(&cwd, &["theme", "eject"]));

    // The default folder name.
    let theme = cwd.join("py-doc-theme");
    let mut expected: Vec<String> = THEME_FILES.iter().map(|s| s.to_string()).collect();
    expected.push("README.md".into());
    expected.sort();
    assert_eq!(files(&theme), expected);

    for name in THEME_FILES {
        assert_eq!(
            fs::read(theme.join(name)).unwrap(),
            fs::read(manifest_dir().join("assets").join(name)).unwrap(),
            "{name} differs from the bundled copy"
        );
    }
    let readme = fs::read_to_string(theme.join("README.md")).unwrap();
    for topic in [
        "--templates",
        "py-doc/module.html",
        "{% block",
        "--css",
        "extra_css",
    ] {
        assert!(readme.contains(topic), "README.md does not mention {topic}");
    }
    // The --css overrides rely on zero-specificity theme tokens.
    assert!(
        fs::read_to_string(theme.join("style.css"))
            .unwrap()
            .contains(":where(:root, :root[data-theme=\"dark\"])")
    );
}

#[test]
fn the_ejected_theme_renders_the_default_site() {
    let cwd = scratch("same_output");
    assert_ok(&py_doc(&cwd, &["theme", "eject", "theme"]));
    let sample = manifest_dir().join("tests/fixtures/sample");
    let sample = sample.to_str().unwrap();

    let default_out = cwd.join("default");
    let ejected_out = cwd.join("ejected");
    // An empty --config keeps the repository's own pyproject.toml (the PyPI
    // packaging metadata) out of these runs.
    let empty_config = cwd.join("empty-pyproject.toml");
    fs::write(&empty_config, "").unwrap();
    let base = [
        "--config",
        empty_config.to_str().unwrap(),
        "--src",
        sample,
        "--name",
        "demo",
    ];
    let default_run = py_doc(
        &cwd,
        &[&base[..], &["--out", default_out.to_str().unwrap()]].concat(),
    );
    assert_ok(&default_run);
    let ejected_run = py_doc(
        &cwd,
        &[
            &base[..],
            &[
                "--out",
                ejected_out.to_str().unwrap(),
                "--templates",
                "theme",
            ],
        ]
        .concat(),
    );
    assert_ok(&ejected_run);
    assert!(
        String::from_utf8_lossy(&ejected_run.stdout)
            .contains("base.html, document.html, index.html, module.html, sidebar.html"),
        "the ejected templates were not used:\n{}",
        String::from_utf8_lossy(&ejected_run.stdout)
    );

    let pages = files(&default_out);
    assert_eq!(files(&ejected_out), pages);
    for page in &pages {
        // The build cache's key covers the template files, which differ.
        if page == ".py-doc-cache" {
            continue;
        }
        let default = fs::read(default_out.join(page)).unwrap();
        let ejected = fs::read(ejected_out.join(page)).unwrap();
        if page.ends_with(".html") {
            assert_eq!(
                normalize_whitespace(&String::from_utf8(ejected).unwrap()),
                normalize_whitespace(&String::from_utf8(default).unwrap()),
                "{page} differs"
            );
        } else {
            // style.css, app.js, the data files, images and the manifest.
            assert!(ejected == default, "{page} differs");
        }
    }
}

#[test]
fn existing_files_are_kept_unless_forced() {
    let cwd = scratch("no_overwrite");
    let theme = cwd.join("theme");
    fs::create_dir_all(&theme).unwrap();
    fs::write(theme.join("style.css"), "/* mine */").unwrap();
    fs::write(theme.join("module.html"), "mine").unwrap();

    let output = py_doc(&cwd, &["theme", "eject", "theme"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("style.css") && stderr.contains("module.html"),
        "{stderr}"
    );
    assert!(stderr.contains("--force"), "{stderr}");
    assert!(
        !stderr.contains("index.html"),
        "lists a file that does not exist: {stderr}"
    );
    // Nothing was written: the existing files are untouched and no others appeared.
    assert_eq!(files(&theme), ["module.html", "style.css"]);
    assert_eq!(
        fs::read_to_string(theme.join("style.css")).unwrap(),
        "/* mine */"
    );

    assert_ok(&py_doc(&cwd, &["theme", "eject", "theme", "--force"]));
    assert_eq!(files(&theme).len(), THEME_FILES.len() + 1);
    assert_eq!(
        fs::read(theme.join("style.css")).unwrap(),
        fs::read(manifest_dir().join("assets/style.css")).unwrap()
    );
}

#[test]
fn subcommands_do_not_mix_with_generation_flags() {
    let cwd = scratch("mixed");
    let output = py_doc(&cwd, &["--src", ".", "theme", "eject"]);
    assert!(!output.status.success());
    assert!(!cwd.join("py-doc-theme").exists());
}
