//! End-to-end tests: run the real binary against the fixture projects in
//! `tests/fixtures` and check the generated site.

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

/// Generates the sample fixture and returns its output directory.
fn generate_sample(test: &str, extra: &[&str]) -> PathBuf {
    let out = out_dir(test);
    let output = run(&fixture("sample"), &out, extra);
    assert!(
        output.status.success(),
        "py-doc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    out
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {}: {}", file, e))
}

#[test]
fn writes_every_file_the_pages_load() {
    let out = generate_sample("files", &[]);
    for file in [
        "index.html",
        "__init__.html",
        "guide.html",
        "01_core/index.html",
        "01_core/engine.html",
        "01_core/sub/engine.html",
        "utils/helpers.html",
        "nav-data.js",
        "search-index.js",
        "style.css",
        "app.js",
    ] {
        assert!(out.join(file).is_file(), "missing {}", file);
    }
}

#[test]
fn landing_page_survives_a_root_init_module() {
    let out = generate_sample("landing", &[]);
    let index = read(&out, "index.html");
    assert!(index.contains("<title>demo - Documentation Index</title>"));
    assert!(index.contains("Demo package root docstring."));
    assert!(index.contains("class=\"module-card"));
}

#[test]
fn signatures_keep_every_parameter_kind() {
    let out = generate_sample("signatures", &[]);
    let engine = read(&out, "01_core/engine.html");
    assert!(
        engine.contains(
            "(self, name: str = &quot;x&quot;, *args, retries: int = 3, **kwargs) -&gt; None:"
        ) || engine.contains(
            "(self, name: str = &quot;x&quot;, *args, retries: int = 3, **kwargs) -> None:"
        )
    );
    assert!(engine.contains("c: int | None = None, d: os.PathLike = &quot;.&quot;"));
    assert!(engine.contains("e: Callable[[int], str] = str"));
    assert!(engine.contains("(self, /, x, *, y: int = 1)"));
    assert!(engine.contains("dict[str, int]"));

    let helpers = read(&out, "utils/helpers.html");
    assert!(helpers.contains("(a, b=2)"));
}

#[test]
fn classes_show_bases_badges_anchors_and_source_lines() {
    let out = generate_sample("classes", &[]);
    let engine = read(&out, "01_core/engine.html");
    assert!(engine.contains("<span class=\"entity-name\">Engine</span>(Base):"));
    assert!(engine.contains("<span class=\"badge\">property</span>"));
    assert!(engine.contains("<span class=\"badge\">static</span>"));
    assert!(engine.contains("<span class=\"keyword\">async</span>"));
    assert!(engine.contains("id=\"method.Engine.arun\""));
    assert!(engine.contains("01_core/engine.py:13"));
    assert!(engine.contains("class=\"page-outline\""));
}

#[test]
fn doctests_render_as_code_blocks() {
    let out = generate_sample("doctests", &[]);
    let engine = read(&out, "01_core/engine.html");
    assert!(engine.contains("<p>Example:</p>"));
    assert!(engine.contains("<pre><code class=\"language-pycon\">&gt;&gt;&gt; Engine().speed\n"));
}

#[test]
fn module_pages_carry_breadcrumb_description_and_resizer() {
    let out = generate_sample("page_chrome", &[]);
    let page = read(&out, "01_core/sub/engine.html");
    assert!(page.contains("<span>core.sub.engine</span>"));
    assert!(page.contains(
        "<meta name=\"description\" content=\"A second module that is also named engine.\">"
    ));
    assert!(page.contains("<div class=\"sidebar-resizer\"></div>"));
    assert!(page.contains("rel=\"icon\""));
}

#[test]
fn source_url_turns_line_references_into_links() {
    let out = generate_sample(
        "source_url",
        &["--source-url", "https://example.test/tree/main/"],
    );
    let engine = read(&out, "01_core/engine.html");
    assert!(engine.contains("href=\"https://example.test/tree/main/01_core/engine.py#L13\""));
}

#[test]
fn pd_write_modules_use_the_document_template() {
    let out = generate_sample("document", &[]);
    let guide = read(&out, "guide.html");
    assert!(guide.contains("document-canvas"));
    assert!(!guide.contains("#pd-write"));
    assert!(guide.contains("alert-note"));
    assert!(guide.contains("alert-warning"));
    assert!(guide.contains("<table>"));
    assert!(guide.contains("<span class=\"tok-kw\">def</span>"));
}

#[test]
fn doc_links_and_images_resolve_to_real_files() {
    let out = generate_sample("links", &[]);
    let guide = read(&out, "guide.html");
    assert!(guide.contains("href=\"./utils/helpers.html#fn.slugify\""));
    assert!(guide.contains("href=\"./01_core/engine.html#method.Engine.arun\""));
    assert!(guide.contains("src=\"./assets/img/logo.png\""));
    assert!(out.join("assets/img/logo.png").is_file());
}

#[test]
fn nested_pages_reach_the_site_root() {
    let out = generate_sample("nested", &[]);
    let page = read(&out, "01_core/sub/engine.html");
    assert!(page.contains("href=\"../../style.css\""));
    assert!(page.contains("src=\"../../nav-data.js\""));
    assert!(page.contains("data-root-path=\"../../\""));
}

#[test]
fn folders_are_grouped_by_full_path() {
    let out = generate_sample("folders", &[]);
    let nav = read(&out, "nav-data.js");
    assert!(nav.starts_with("const navData = ["));
    assert!(nav.contains("\"name\":\"core\""));
    assert!(nav.contains("\"name\":\"core/sub\""));
}

#[test]
fn layout_and_theme_are_baked_into_the_page() {
    let out = generate_sample("theme", &["--layout", "modern", "--theme", "light"]);
    let index = read(&out, "index.html");
    assert!(index.contains("<html lang=\"en\" data-layout=\"modern\" data-theme=\"light\">"));

    // The default theme follows the reader's system, so nothing is baked.
    let out = generate_sample("theme_auto", &[]);
    let index = read(&out, "index.html");
    assert!(index.contains("<html lang=\"en\" data-layout=\"classic\">"));
}

#[test]
fn generated_data_holds_no_build_machine_paths() {
    let out = generate_sample("paths", &[]);
    let manifest_dir = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    for file in ["search-index.js", "nav-data.js", "guide.html", "index.html"] {
        let body = read(&out, file).replace("\\\\", "/").replace('\\', "/");
        assert!(!body.contains(&manifest_dir), "{} leaks a local path", file);
        assert!(!body.contains("filepath"), "{} has a filepath field", file);
    }
    // The search index carries summaries, not rendered HTML.
    let search = read(&out, "search-index.js");
    assert!(search.contains("\"summary\":\"Turn text into a URL slug.\""));
    assert!(!search.contains("<p>"));
}

#[test]
fn environments_are_skipped_and_exclude_removes_more() {
    let out = generate_sample("excludes_default", &[]);
    assert!(!out.join("venv").exists());

    let out = generate_sample("excludes_user", &["--exclude", "utils"]);
    assert!(!out.join("utils").exists());
    assert!(out.join("01_core/engine.html").is_file());
}

#[test]
fn strict_fails_on_parse_errors_and_unresolved_references() {
    let lenient = run(&fixture("broken"), &out_dir("strict_off"), &[]);
    assert!(lenient.status.success());
    let stderr = String::from_utf8_lossy(&lenient.stderr);
    assert!(stderr.contains("bad.py: line 1"));
    assert!(stderr.contains("nowhere.missing"));
    assert!(stderr.contains("img/missing.png"));

    let out = out_dir("strict_on");
    let strict = run(&fixture("broken"), &out, &["--strict"]);
    assert!(!strict.status.success());
    // The site is still written, so the problems can be inspected.
    assert!(out.join("links.html").is_file());
    // Error boxes must not reveal where the project lives on disk.
    let manifest_dir = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    assert!(
        !read(&out, "links.html")
            .replace('\\', "/")
            .contains(&manifest_dir)
    );
}

#[test]
fn a_single_file_can_be_documented() {
    let out = out_dir("single_file");
    let output = run(
        &fixture("sample").join("utils").join("helpers.py"),
        &out,
        &[],
    );
    assert!(output.status.success());
    assert!(out.join("helpers.html").is_file());
    assert!(read(&out, "index.html").contains("helpers"));
}

#[test]
fn custom_templates_override_only_the_files_they_provide() {
    let templates = out_dir("custom_templates_src");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("style.css"), "/* custom theme */").unwrap();

    let out = generate_sample(
        "custom_templates",
        &["--templates", templates.to_str().unwrap()],
    );
    assert_eq!(read(&out, "style.css"), "/* custom theme */");
    assert!(read(&out, "app.js").contains("renderNavTree"));
}

/// Pages of the sample fixture, as site-relative paths, sorted.
fn html_pages(out: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, found);
            } else if path.extension().is_some_and(|ext| ext == "html") {
                let rel = path.strip_prefix(root).unwrap();
                found.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut found = Vec::new();
    walk(out, out, &mut found);
    found.sort();
    found
}

/// Collapses every run of whitespace to one space, so template
/// indentation can change without touching the snapshots.
fn normalize_whitespace(html: &str) -> String {
    html.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compares every page of a generated sample site with the snapshots in
/// `tests/snapshots/sample`. Set `PY_DOC_UPDATE_SNAPSHOTS=1` to rewrite them
/// after an intended change to the default theme.
fn assert_matches_sample_snapshots(out: &Path) {
    let snapshots = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join("sample");
    let pages = html_pages(out);
    if std::env::var_os("PY_DOC_UPDATE_SNAPSHOTS").is_some() {
        let _ = fs::remove_dir_all(&snapshots);
        for page in &pages {
            let target = snapshots.join(page);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, normalize_whitespace(&read(out, page)) + "\n").unwrap();
        }
        return;
    }
    assert_eq!(pages, html_pages(&snapshots), "the set of pages changed");
    for page in &pages {
        let expected = fs::read_to_string(snapshots.join(page)).unwrap();
        assert_eq!(
            normalize_whitespace(&read(out, page)),
            expected.trim_end(),
            "{} differs from tests/snapshots/sample/{}",
            page,
            page
        );
    }
}

#[test]
fn default_output_matches_the_snapshots() {
    let out = generate_sample("default_snapshots", &[]);
    assert_matches_sample_snapshots(&out);
}

#[test]
fn whole_file_template_overrides_still_work() {
    // Copies of the theme from before it used template inheritance: each
    // file is a complete page and must keep rendering the same site.
    let templates = fixture("templates").join("legacy");
    let out = out_dir("legacy_templates");
    let output = run(
        &fixture("sample"),
        &out,
        &["--templates", templates.to_str().unwrap()],
    );
    assert!(
        output.status.success(),
        "py-doc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("document.html, index.html, module.html, sidebar.html"),
        "the custom templates were not used:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_matches_sample_snapshots(&out);

    // A standalone page that shares nothing with the bundled theme.
    let templates = out_dir("standalone_templates_src");
    fs::create_dir_all(&templates).unwrap();
    fs::write(
        templates.join("module.html"),
        "<p class=\"standalone\">{{ display_module_name }} in {{ package.name }}</p>",
    )
    .unwrap();
    let out = generate_sample(
        "standalone_templates",
        &["--templates", templates.to_str().unwrap()],
    );
    assert_eq!(
        read(&out, "utils/helpers.html"),
        "<p class=\"standalone\">helpers in demo</p>"
    );
    assert!(read(&out, "index.html").contains("module-grid"));
}

#[test]
fn a_custom_template_can_extend_the_bundled_one_and_override_a_block() {
    let templates = fixture("templates").join("blocks");
    let out = generate_sample(
        "block_templates",
        &["--templates", templates.to_str().unwrap()],
    );
    let page = read(&out, "utils/helpers.html");
    // The overridden block...
    assert!(page.contains("<h1 class=\"custom-header\">API: helpers</h1>"));
    assert!(!page.contains("breadcrumb-row"));
    // ...while every other block keeps the bundled default.
    assert!(page.contains("<title>Module helpers - demo</title>"));
    assert!(page.contains("class=\"sidebar\""));
    assert!(page.contains("id=\"fn.slugify\""));
    assert!(page.contains("class=\"page-outline\""));
    // super() keeps the bundled scripts and appends one.
    let app = page.find("app.js").expect("bundled scripts kept");
    let extra = page.find("extra.js").expect("extra script added");
    assert!(app < extra);
    // A user partial included from a user template.
    assert!(page.contains("<footer class=\"custom-footer\">Docs for demo"));

    // Pages built from templates the directory does not provide are unchanged.
    assert!(!read(&out, "guide.html").contains("custom-footer"));
    assert!(read(&out, "index.html").contains("module-grid"));
}

#[test]
fn a_custom_base_template_reaches_every_page() {
    let templates = fixture("templates").join("base-footer");
    let out = generate_sample(
        "base_template",
        &["--templates", templates.to_str().unwrap()],
    );
    for page in ["index.html", "utils/helpers.html", "guide.html"] {
        let html = read(&out, page);
        assert!(
            html.contains("<footer class=\"site-footer\">Every page</footer>"),
            "{} lacks the custom footer",
            page
        );
        assert!(html.contains("app.js"), "{} lost the bundled scripts", page);
    }
}

#[test]
fn version_flag_reports_the_tool_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--version")
        .output()
        .expect("failed to run py-doc");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("py-doc {}", env!("CARGO_PKG_VERSION"))
    );
}
