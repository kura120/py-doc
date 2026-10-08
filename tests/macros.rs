//! End-to-end tests for the `#pd-` docstring macros and for `--clean`,
//! run against `tests/fixtures/macros` and `tests/fixtures/sample`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// A fresh, empty output directory unique to one test.
fn out_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn run(src: &Path, out: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--src")
        .arg(src)
        .arg("--out")
        .arg(out)
        .args(["--name", "demo"])
        .args(extra)
        .output()
        .expect("failed to run py-doc")
}

fn generate(fixture_name: &str, out: &Path, extra: &[&str]) -> String {
    let output = run(&fixture(fixture_name), out, extra);
    assert!(
        output.status.success(),
        "py-doc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {}: {}", file, e))
}

// ---------------------------------------------------------------- macros

#[test]
fn note_and_warning_render_boxes_with_inline_markdown() {
    let out = out_dir("macro_alerts");
    generate("macros", &out, &[]);
    let zeta = read(&out, "zeta.html");
    assert!(zeta.contains("class=\"alert alert-note\""));
    assert!(zeta.contains("Note:</strong> Use <strong>care</strong> with <code>Widget</code>."));
    assert!(zeta.contains("class=\"alert alert-warning\""));
    assert!(zeta.contains("Warning:</strong> Not thread safe."));
    // The macro source never leaks into the page.
    assert!(!zeta.contains("#pd-note"));
    assert!(!zeta.contains("#pd-warning"));
}

#[test]
fn macros_work_directly_after_a_paragraph_line() {
    let out = out_dir("macro_adjacent");
    generate("macros", &out, &[]);
    let handbook = read(&out, "handbook.html");
    assert!(handbook.contains("<p>Intro paragraph.</p>"));
    assert!(handbook.contains("Note:</strong> Macros work directly after a paragraph line."));
}

#[test]
fn write_macro_makes_a_document_page() {
    let out = out_dir("macro_write");
    generate("macros", &out, &[]);
    let handbook = read(&out, "handbook.html");
    assert!(handbook.contains("document-canvas"));
    assert!(handbook.contains("<h1>Handbook</h1>"));
    assert!(!handbook.contains("#pd-write"));
    // A module with code stays a module page.
    assert!(!read(&out, "zeta.html").contains("document-canvas"));
}

#[test]
fn code_macro_accepts_both_spellings_and_highlights_python() {
    let out = out_dir("macro_code");
    generate("macros", &out, &[]);
    let handbook = read(&out, "handbook.html");
    // "#pd-code python"
    assert!(handbook.contains(
        "<pre><code class=\"language-python\"><span class=\"tok-kw\">def</span> <span class=\"tok-fn\">hello</span>():"
    ));
    // "#pd-code: python"
    assert!(
        handbook
            .contains("<pre><code class=\"language-python\">x = <span class=\"tok-num\">1</span>")
    );
    assert!(!handbook.contains("#pd-code"));
}

#[test]
fn macros_inside_a_fenced_block_stay_literal() {
    let out = out_dir("macro_fence");
    generate("macros", &out, &[]);
    let handbook = read(&out, "handbook.html");
    assert!(
        handbook
            .contains("<pre><code class=\"language-text\">#pd-note: this stays as text\n</code>")
    );
}

#[test]
fn image_macro_and_markdown_images_are_copied_once_and_prefixed() {
    let out = out_dir("macro_image");
    let stderr = generate("macros", &out, &[]);
    // Three pages reference the same picture; none of them may fail.
    assert!(!stderr.contains("pic.png"), "unexpected warning:\n{stderr}");
    assert!(out.join("assets/img/pic.png").is_file());

    let handbook = read(&out, "handbook.html");
    assert!(handbook.contains("<img src=\"./assets/img/pic.png\" alt=\"\">"));
    assert!(handbook.contains("<img src=\"./assets/img/pic.png\" alt=\"inline picture\""));

    // From a nested page the same file is one level up.
    assert!(read(&out, "pkg/deep.html").contains("<img src=\"../assets/img/pic.png\" alt=\"\">"));
}

#[test]
fn doc_links_reach_modules_classes_functions_and_methods() {
    let out = out_dir("macro_doclink");
    generate("macros", &out, &[]);

    let deep = read(&out, "pkg/deep.html");
    for href in [
        "../alpha.html#fn.target",
        "../zeta.html#class.Widget",
        "../zeta.html#method.Widget.spin",
        "../handbook.html",
    ] {
        assert!(
            deep.contains(&format!("<a href=\"{href}\" class=\"doc-internal-link\">")),
            "missing link to {href}"
        );
    }

    // Inside a method docstring on a root-level page.
    assert!(read(&out, "zeta.html").contains("<a href=\"./alpha.html#fn.target\""));

    // Every link target exists, including the anchors.
    assert!(read(&out, "alpha.html").contains("id=\"fn.target\""));
    let zeta = read(&out, "zeta.html");
    assert!(zeta.contains("id=\"class.Widget\""));
    assert!(zeta.contains("id=\"method.Widget.spin\""));
}

#[test]
fn z_index_macro_orders_the_navigation_and_is_hidden() {
    let out = out_dir("macro_zindex");
    generate("macros", &out, &[]);
    let nav = read(&out, "nav-data.js");
    let zeta = nav
        .find("\"link_path\":\"zeta.html\"")
        .expect("zeta in nav");
    let alpha = nav
        .find("\"link_path\":\"alpha.html\"")
        .expect("alpha in nav");
    assert!(zeta < alpha, "zeta has #pd-z-index: 1 and must come first");
    assert!(!read(&out, "zeta.html").contains("pd-z-index"));
}

#[test]
fn broken_macros_warn_and_never_drop_content() {
    let out = out_dir("macro_problems");
    let stderr = generate("macros", &out, &[]);
    for expected in [
        "pkg/problems.py: '#pd-warn' is not a macro that works here",
        "pkg/problems.py: #pd-doc-link target 'nowhere.at_all' was not found",
        "pkg/problems.py: Image file not found: '../outside.png'",
        "pkg/problems.py: #pd-code block is not closed",
        "pkg/problems.py: '#pd-z-index' is not a macro that works here (it only applies in a module docstring)",
    ] {
        assert!(
            stderr.contains(expected),
            "missing warning: {expected}\n{stderr}"
        );
    }

    let page = read(&out, "pkg/problems.html");
    assert!(page.contains("<code>nowhere.at_all</code> <em>(not found)</em>"));
    assert!(page.contains("class=\"alert alert-error\""));
    // The unterminated code block is still shown.
    assert!(page.contains("never closed"));

    // The same problems fail a strict build.
    let strict = run(
        &fixture("macros"),
        &out_dir("macro_problems_strict"),
        &["--strict"],
    );
    assert!(!strict.status.success());

    // Without the problem file the fixture is clean, even under --strict.
    let clean = run(
        &fixture("macros"),
        &out_dir("macro_no_problems_strict"),
        &["--strict", "--exclude", "problems.py"],
    );
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
}

// ----------------------------------------------------------------- clean

#[test]
fn clean_removes_only_files_an_earlier_run_wrote() {
    let out = out_dir("clean_stale");
    generate("sample", &out, &[]);
    assert!(out.join("utils/helpers.html").is_file());
    assert!(out.join("assets/img/logo.png").is_file());

    // Files the user keeps in the output folder.
    fs::write(out.join("CNAME"), "docs.example.test").unwrap();
    fs::write(out.join("utils/notes.txt"), "mine").unwrap();

    // Without --clean, pages for removed modules are left behind.
    generate("sample", &out, &["--exclude", "utils"]);
    assert!(out.join("utils/helpers.html").is_file());

    // With --clean they go; the user's files stay.
    generate(
        "sample",
        &out,
        &["--exclude", "utils", "--exclude", "guide.py", "--clean"],
    );
    assert!(!out.join("utils/helpers.html").exists());
    assert!(!out.join("guide.html").exists());
    // guide.py was the only user of the image.
    assert!(!out.join("assets").exists());
    assert!(out.join("CNAME").is_file());
    assert!(out.join("utils/notes.txt").is_file());
    assert!(out.join("01_core/engine.html").is_file());
    assert!(out.join("index.html").is_file());
}

#[test]
fn clean_removes_emptied_folders_and_tolerates_a_first_run() {
    let out = out_dir("clean_first_run");
    // First run with --clean: nothing to compare against, nothing removed.
    generate("sample", &out, &["--clean"]);
    assert!(out.join("utils/helpers.html").is_file());

    generate("sample", &out, &["--exclude", "utils", "--clean"]);
    assert!(!out.join("utils").exists());
}

#[test]
fn clean_ignores_manifest_entries_that_point_outside_the_site() {
    let out = out_dir("clean_escape");
    let outside = out.parent().unwrap().join("clean_escape_outside.txt");
    fs::write(&outside, "keep me").unwrap();

    generate("sample", &out, &[]);
    let manifest = out.join(".py-doc-manifest");
    let mut body = fs::read_to_string(&manifest).unwrap();
    body.push_str("../clean_escape_outside.txt\n");
    fs::write(&manifest, body).unwrap();

    generate("sample", &out, &["--clean"]);
    assert!(outside.is_file());
    fs::remove_file(outside).unwrap();
}
