//! End-to-end tests for `--pages`, against `tests/fixtures/pages`:
//! `docs/` holds two numbered pages, one with front matter, a nested page
//! and an `index.md` for the landing page.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pages")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("pages")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("empty.toml"), "").unwrap();
    dir
}

fn swiftdoc(dir: &Path, pages: &Path, extra: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .current_dir(dir)
        .args(["--config", "empty.toml", "--out", "site", "--name", "demo"])
        .arg("--src")
        .arg(fixture().join("pkg"))
        .arg("--pages")
        .arg(pages)
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

fn read(path: PathBuf) -> String {
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn pages_render_in_order_with_working_links() {
    let dir = scratch("render");
    swiftdoc(&dir, &fixture().join("docs"), &[]);
    let site = dir.join("site");

    // Links between pages, including across folders and with an anchor.
    let install = read(site.join("docs/01_install.html"));
    assert!(
        install.contains(r#"href="02_guide.html#usage""#),
        "{install}"
    );
    let guide = read(site.join("docs/02_guide.html"));
    assert!(guide.contains(r#"href="advanced/tuning.html""#), "{guide}");
    let tuning = read(site.join("docs/advanced/tuning.html"));
    assert!(tuning.contains(r#"href="../01_install.html""#), "{tuning}");

    // #pd-doc-link works from a Markdown page.
    assert!(
        install.contains(r#"href="../engine.html#class.Engine""#),
        "{install}"
    );

    // index.md is the landing page's intro (with its links relative to the
    // site root), not a page of its own.
    let index = read(site.join("index.html"));
    assert!(index.contains("Welcome"), "{index}");
    assert!(index.contains(r#"href="docs/01_install.html""#), "{index}");
    assert!(!site.join("docs/index.html").exists());

    // Navigation: the Guides group comes before the API reference, pages in
    // prefix / front-matter order, titled by front matter or first heading.
    let at = |name: &str| {
        index
            .find(name)
            .unwrap_or_else(|| panic!("{name} missing from the landing page"))
    };
    assert!(at("Guides") < at(">engine<"));
    assert!(at("Installing") < at("User guide") && at("User guide") < at("Tuning"));
}

#[test]
fn pages_are_searchable_and_cleaned_up() {
    let dir = scratch("clean");
    let pages = dir.join("docs");
    fs::create_dir_all(&pages).unwrap();
    fs::write(pages.join("keep.md"), "# Keep me\n").unwrap();
    fs::write(pages.join("drop.md"), "# Drop me\n").unwrap();
    swiftdoc(&dir, &pages, &[]);

    let site = dir.join("site");
    let search = read(site.join("search-index.json"));
    assert!(
        search.contains("Keep me") && search.contains("docs/keep.html"),
        "{search}"
    );
    let manifest = read(site.join(".swiftdoc-manifest"));
    assert!(manifest.contains("docs/drop.html"), "{manifest}");

    fs::remove_file(pages.join("drop.md")).unwrap();
    swiftdoc(&dir, &pages, &["--clean"]);
    assert!(site.join("docs/keep.html").is_file());
    assert!(
        !site.join("docs/drop.html").exists(),
        "--clean left a removed page"
    );
}
