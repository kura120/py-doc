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

/// An empty pyproject.toml passed as `--config`, so runs against the
/// fixtures never pick up the repository's own pyproject.toml (the PyPI
/// packaging metadata), which py-doc would otherwise find by walking up
/// from `--src`.
fn empty_config() -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("empty-pyproject.toml");
    if !path.exists() {
        fs::write(&path, "").unwrap();
    }
    path
}

fn run(src: &Path, out: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--config")
        .arg(empty_config())
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
/// Pages listed in `skip` must exist but are not compared.
fn assert_matches_sample_snapshots(out: &Path, skip: &[&str]) {
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
    for page in pages.iter().filter(|p| !skip.contains(&p.as_str())) {
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

/// The `href` values inside a page's `<noscript>` block.
fn noscript_links(html: &str) -> Vec<String> {
    let start = html.find("<noscript>").expect("page has a noscript block");
    let end = start + html[start..].find("</noscript>").unwrap();
    html[start..end]
        .split("href=\"")
        .skip(1)
        .map(|rest| rest[..rest.find('"').unwrap()].to_string())
        .collect()
}

/// Resolves a relative link found on `page` to a site-relative path,
/// dropping any `#fragment`.
fn resolve_link(page: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap();
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for part in href.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop().expect("link leaves the site");
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[test]
fn without_javascript_every_module_is_two_clicks_away() {
    let out = generate_sample("noscript_fallback", &[]);
    let pages = html_pages(&out);

    // The index lists every other page once, in its sidebar fallback.
    let index = read(&out, "index.html");
    assert!(index.contains("id=\"all-modules\""));
    let mut listed: Vec<String> = noscript_links(&index)
        .iter()
        .map(|href| resolve_link("index.html", href))
        .collect();
    listed.sort();
    let others: Vec<String> = pages
        .iter()
        .filter(|page| *page != "index.html")
        .cloned()
        .collect();
    assert_eq!(listed, others);

    // Every other page carries a single link to that list, not the list itself.
    for page in &others {
        let links = noscript_links(&read(&out, page));
        assert_eq!(links.len(), 1, "{} repeats the module list", page);
        assert!(links[0].ends_with("index.html#all-modules"), "{}", page);
        assert_eq!(resolve_link(page, &links[0]), "index.html", "{}", page);
    }
}

#[test]
fn default_output_matches_the_snapshots() {
    let out = generate_sample("default_snapshots", &[]);
    assert_matches_sample_snapshots(&out, &[]);
}

#[test]
fn whole_file_template_overrides_still_work() {
    // Copies of the page templates from before the theme used template
    // inheritance: each file is a complete page and must keep rendering the
    // same site.
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
        String::from_utf8_lossy(&output.stdout).contains("document.html, index.html, module.html"),
        "the custom templates were not used:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    // engine.html has a class attribute, which the old module.html predates:
    // it still renders, just without the Attributes list.
    assert_matches_sample_snapshots(&out, &["01_core/engine.html"]);
    let engine = read(&out, "01_core/engine.html");
    assert!(engine.contains("id=\"class.Engine\""));
    assert!(!engine.contains("attr.Engine.speed"));

    // A standalone page that shares nothing with the bundled theme.
    let templates = out_dir("standalone_templates_src");
    fs::create_dir_all(&templates).unwrap();
    fs::write(
        templates.join("module.html"),
        "<p class=\"standalone\">{{ display_module_name }} in {{ package.name }}</p>",
    )
    .unwrap();
    fs::write(
        templates.join("sidebar.html"),
        "<nav class=\"custom-sidebar\">{{ package.name }}</nav>",
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
    // The bundled pages include the replacement sidebar.
    let index = read(&out, "index.html");
    assert!(index.contains("module-grid"));
    assert!(index.contains("<nav class=\"custom-sidebar\">demo</nav>"));
    assert!(!index.contains("nav-fallback"));
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

/// A 5-line stylesheet that only changes the accent color.
const ACCENT_CSS: &str = "/* brand.css: blue links, red keywords */\n:root {\n    --accent-blue: #2563eb;\n    --accent-red: #dc2626;\n}\n";

/// Writes `theme.css` and `more/theme.css` (same name, other folder) and
/// returns their paths.
fn write_extra_css(test: &str) -> (PathBuf, PathBuf) {
    let dir = out_dir(test);
    fs::create_dir_all(dir.join("more")).unwrap();
    let first = dir.join("theme.css");
    let second = dir.join("more").join("theme.css");
    fs::write(&first, ACCENT_CSS).unwrap();
    fs::write(&second, ".module-title { letter-spacing: 0.02em; }\n").unwrap();
    (first, second)
}

#[test]
fn css_files_are_copied_and_linked_after_the_theme_on_every_page() {
    let (first, second) = write_extra_css("extra_css_src");
    let out = generate_sample(
        "extra_css",
        &[
            "--css",
            first.to_str().unwrap(),
            "--css",
            second.to_str().unwrap(),
        ],
    );
    assert_eq!(read(&out, "custom/0-theme.css"), ACCENT_CSS);
    assert!(read(&out, "custom/1-theme.css").contains("letter-spacing"));

    // Index, module, nested module and document pages, each from its depth.
    for (page, root) in [
        ("index.html", "./"),
        ("utils/helpers.html", "../"),
        ("01_core/sub/engine.html", "../../"),
        ("guide.html", "./"),
    ] {
        let html = read(&out, page);
        let theme = html
            .find(&format!("href=\"{root}style.css\""))
            .unwrap_or_else(|| panic!("{page} lacks style.css"));
        let first = html
            .find(&format!("href=\"{root}custom/0-theme.css\""))
            .unwrap_or_else(|| panic!("{page} lacks the first --css file"));
        let second = html
            .find(&format!("href=\"{root}custom/1-theme.css\""))
            .unwrap_or_else(|| panic!("{page} lacks the second --css file"));
        assert!(theme < first && first < second, "{page}: wrong order");
    }

    // The copies are in the manifest, so --clean removes them once dropped.
    let manifest = read(&out, ".py-doc-manifest");
    assert!(manifest.lines().any(|l| l == "custom/0-theme.css"));
    assert!(manifest.lines().any(|l| l == "custom/1-theme.css"));
    let output = run(&fixture("sample"), &out, &["--clean"]);
    assert!(output.status.success());
    assert!(!out.join("custom").exists());
    assert!(!read(&out, "index.html").contains("custom/"));
}

/// A plain `:root` rule in a `--css` file must beat the theme's own token
/// rules, so every rule in style.css that sets a custom property has to
/// carry zero specificity, i.e. be wrapped in `:where()`.
#[test]
fn theme_tokens_have_zero_specificity() {
    let css =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/style.css")).unwrap();
    // Drop comments.
    let mut text = String::new();
    let mut rest = css.as_str();
    while let Some(start) = rest.find("/*") {
        text.push_str(&rest[..start]);
        let end = rest[start..].find("*/").expect("closed comment") + start + 2;
        rest = &rest[end..];
    }
    text.push_str(rest);

    let mut token_rules = 0;
    for chunk in text.split('}') {
        let Some(open) = chunk.rfind('{') else {
            continue;
        };
        let selector = chunk[..open].rsplit('{').next().unwrap().trim();
        let body = &chunk[open + 1..];
        let defines_token = body
            .split(';')
            .any(|decl| decl.trim_start().starts_with("--"));
        if defines_token {
            token_rules += 1;
            assert!(
                selector.starts_with(":where(") && selector.ends_with(')'),
                "`{selector}` sets custom properties but is not wrapped in :where()"
            );
        }
    }
    assert!(token_rules >= 11, "found only {token_rules} token rules");
}

#[test]
fn a_missing_css_file_is_an_error() {
    let out = out_dir("missing_css");
    let output = run(&fixture("sample"), &out, &["--css", "no/such/theme.css"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no/such/theme.css"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.join("index.html").exists());
}

#[test]
fn css_files_work_together_with_custom_templates() {
    let (first, _) = write_extra_css("extra_css_templates_src");
    let templates = fixture("templates").join("blocks");
    let out = generate_sample(
        "extra_css_templates",
        &[
            "--templates",
            templates.to_str().unwrap(),
            "--css",
            first.to_str().unwrap(),
        ],
    );
    let page = read(&out, "utils/helpers.html");
    assert!(page.contains("custom-header"));
    assert!(page.contains("href=\"../custom/0-theme.css\""));
    assert!(read(&out, "index.html").contains("href=\"./custom/0-theme.css\""));
}

#[test]
fn fixture_runs_ignore_the_repository_pyproject() {
    // The repository root has a pyproject.toml for the PyPI package. These
    // tests pass an empty --config, so its [project] table must not reach the
    // fixture sites: no pyproject hint, and the default docs version.
    let out = out_dir("ignores_repo_pyproject");
    let output = run(&fixture("sample"), &out, &[]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("pyproject"), "unexpected: {}", stderr);
    assert!(read(&out, "index.html").contains("1.0.0"));
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
