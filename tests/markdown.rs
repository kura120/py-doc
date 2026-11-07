//! `--format markdown`: links and anchors, macro conversion, snapshots,
//! determinism and what is written.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn out_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("markdown")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Runs swiftdoc with an empty --config, so the repository's own
/// pyproject.toml is never read.
fn run(src: &Path, out: &Path, extra: &[&str]) -> Output {
    let config = Path::new(env!("CARGO_TARGET_TMPDIR")).join("markdown-empty.toml");
    if !config.exists() {
        fs::write(&config, "").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .arg("--config")
        .arg(&config)
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

fn markdown(fixture_name: &str, test: &str, extra: &[&str]) -> PathBuf {
    let out = out_dir(test);
    let mut args = vec!["--format", "markdown"];
    args.extend_from_slice(extra);
    run(&fixture(fixture_name), &out, &args);
    out
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {file}: {e}"))
}

/// Every file under `dir`, as forward-slash paths relative to it.
fn files(dir: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                let parts: Vec<_> = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.insert(parts.join("/"));
            }
        }
    }
    let mut found = BTreeSet::new();
    walk(dir, dir, &mut found);
    found
}

/// `a/b/../c` -> `a/c`, without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                assert!(
                    out.pop(),
                    "link climbs out of the output folder: {}",
                    path.display()
                );
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The `id`s of the `<a id="..."></a>` anchors in a page.
fn anchors(text: &str) -> Vec<String> {
    text.match_indices("<a id=\"")
        .map(|(at, prefix)| {
            let rest = &text[at + prefix.len()..];
            rest[..rest.find('"').unwrap()].to_string()
        })
        .collect()
}

/// Every `](target)` link and image target in a page, outside fenced code.
fn links(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            let after = &rest[at + 2..];
            let end = after.find(')').expect("unclosed link");
            found.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    found
}

/// Checks every relative link in every `.md` file: the target file exists
/// and, for `#anchor` links, the target page has that anchor. Anchors are
/// unique within each page. Returns (links checked, anchor links checked).
fn assert_links_resolve(out: &Path) -> (usize, usize) {
    let pages: BTreeMap<String, String> = files(out)
        .into_iter()
        .filter(|f| f.ends_with(".md"))
        .map(|f| {
            let text = read(out, &f);
            (f, text)
        })
        .collect();
    assert!(!pages.is_empty(), "no Markdown was written");

    for (page, text) in &pages {
        let ids = anchors(text);
        let unique: BTreeSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate anchors in {page}");
    }

    let (mut checked, mut with_anchor) = (0, 0);
    for (page, text) in &pages {
        let dir = Path::new(page).parent().unwrap_or(Path::new(""));
        for link in links(text) {
            if ["http://", "https://", "data:", "mailto:"]
                .iter()
                .any(|scheme| link.starts_with(scheme))
            {
                continue;
            }
            let (path, anchor) = match link.split_once('#') {
                Some((path, anchor)) => (path, Some(anchor)),
                None => (link.as_str(), None),
            };
            let target = normalize(&dir.join(path));
            let target_name = target.to_string_lossy().replace('\\', "/");
            assert!(
                out.join(&target).is_file(),
                "{page}: link `{link}` points at a missing file `{target_name}`"
            );
            checked += 1;
            if let Some(anchor) = anchor {
                let target_text = pages.get(&target_name).unwrap_or_else(|| {
                    panic!("{page}: anchor link `{link}` to a non-Markdown file")
                });
                assert!(
                    anchors(target_text).iter().any(|id| id == anchor),
                    "{page}: `{link}`: `{target_name}` has no anchor `{anchor}`"
                );
                with_anchor += 1;
            }
        }
    }
    (checked, with_anchor)
}

#[test]
fn every_link_and_anchor_resolves() {
    let cases: [(&str, &[&str]); 9] = [
        ("annotations", &[]),
        ("docstrings", &[]),
        ("nested", &[]),
        ("nested", &["--include-private"]),
        ("sample", &[]),
        ("macros", &[]),
        ("variables", &[]),
        ("visibility", &[]),
        ("visibility", &["--include-private"]),
    ];
    let (mut total, mut anchored) = (0, 0);
    for (i, (name, extra)) in cases.iter().enumerate() {
        let out = markdown(name, &format!("links_{i}_{name}"), extra);
        let (checked, with_anchor) = assert_links_resolve(&out);
        total += checked;
        anchored += with_anchor;
    }
    // The fixtures link to modules, classes, methods, functions, variables
    // and attributes, so a broken scheme cannot pass by checking nothing.
    assert!(
        total > 40 && anchored > 10,
        "only {total} links ({anchored} with anchors)"
    );
}

#[test]
fn docstring_sections_become_lists() {
    let out = markdown("docstrings", "sections", &[]);
    let google = read(&out, "google.md");
    for expected in [
        "**Parameters**\n\n- `job` (`str`): The job to run.\n\n  See reference: [`google.Engine`](./google.md#class.Engine)\n",
        "- `retries` (`int`, default `1`): How often to retry.\n\n  > **Note:** Retries back off exponentially.\n",
        "**Returns**\n\n- `bool`: True if the job **succeeded**.\n",
        "**Raises**\n\n- `ValueError`: If the job is empty.\n- `RuntimeError`: If the engine is stopped.\n",
        "**Examples**\n\n```pycon\n>>> Engine().run(\"build\")\nTrue\n```\n",
        "> **Note:** Jobs run one at a time.\n",
        "> **Warning:** Never run two engines on one database.\n",
    ] {
        assert!(
            google.contains(expected),
            "missing {expected:?} in\n{google}"
        );
    }
    assert!(!google.contains("Args:"));
    // The rest of the docstring stays above the sections.
    assert!(google.contains("Runs one job.\n\nSome more text about running.\n\n**Parameters**"));

    let numpy = read(&out, "numpy_style.md");
    assert!(
        numpy.contains("- `axis` (`int`, default `0`): Axis along which the means are computed.")
    );
    assert!(!numpy.contains("----------"));

    // Sections off: the docstrings are written as before.
    let out = markdown(
        "docstrings",
        "sections_off",
        &["--docstring-style", "markdown"],
    );
    let google = read(&out, "google.md");
    assert!(google.contains("Args:"));
    assert!(!google.contains("**Parameters**"));
}

#[test]
fn doc_links_use_the_anchor_scheme() {
    let out = markdown("variables", "anchor_scheme", &[]);
    let links = read(&out, "links.md");
    assert!(
        links.contains("[`config.TIMEOUT`](./config.md#var.TIMEOUT)"),
        "{links}"
    );
    assert!(
        links.contains("[`config.Point.x`](./config.md#attr.Point.x)"),
        "{links}"
    );
    assert!(
        links.contains("[`Mode.FAST`](./config.md#attr.Mode.FAST)"),
        "{links}"
    );

    let out = markdown("macros", "anchor_scheme_macros", &[]);
    let deep = read(&out, "pkg/deep.md");
    assert!(
        deep.contains("[`alpha.target`](../alpha.md#fn.target)"),
        "{deep}"
    );
    assert!(
        deep.contains("[`zeta.Widget`](../zeta.md#class.Widget)"),
        "{deep}"
    );
    assert!(
        deep.contains("[`zeta.Widget.spin`](../zeta.md#method.Widget.spin)"),
        "{deep}"
    );
    assert!(deep.contains("[`handbook`](../handbook.md)"), "{deep}");
    let zeta = read(&out, "zeta.md");
    assert!(
        zeta.contains("<a id=\"class.Widget\"></a>\n\n### class `Widget`"),
        "{zeta}"
    );
    assert!(
        zeta.contains("<a id=\"method.Widget.spin\"></a>\n\n##### `spin()`"),
        "{zeta}"
    );
}

#[test]
fn macros_become_plain_markdown() {
    let out = markdown("macros", "macros", &[]);
    let zeta = read(&out, "zeta.md");
    assert!(
        zeta.contains("> **Note:** Use **care** with `Widget`."),
        "{zeta}"
    );
    assert!(zeta.contains("> **Warning:** Not thread safe."), "{zeta}");

    let handbook = read(&out, "handbook.md");
    assert!(
        handbook.starts_with("# Handbook\n"),
        "doc-only page keeps its own heading"
    );
    assert!(
        !handbook.contains("#pd-write") && !handbook.contains("#pd-code"),
        "{handbook}"
    );
    assert!(
        handbook.contains("```python\ndef hello():\n    return \"hi\"\n```"),
        "{handbook}"
    );
    assert!(
        handbook.contains("```text\nplain <output>\n```"),
        "{handbook}"
    );
    assert!(
        handbook.contains("```text\n#pd-note: this stays as text\n```"),
        "fenced macro-looking lines stay literal:\n{handbook}"
    );
    assert!(handbook.contains("![](./assets/img/pic.png)"), "{handbook}");
    assert!(
        handbook.contains("![inline picture](./assets/img/pic.png)"),
        "{handbook}"
    );
    assert!(out.join("assets/img/pic.png").is_file());

    // Broken macros keep their text and never become a broken link.
    let problems = read(&out, "pkg/problems.md");
    assert!(
        problems.contains("See reference: `nowhere.at_all` (not found)"),
        "{problems}"
    );
    assert!(
        problems.contains("*Image not found: `../outside.png`*"),
        "{problems}"
    );
}

#[test]
fn doctests_are_fenced_as_pycon() {
    let out = markdown("sample", "pycon", &[]);
    let engine = read(&out, "01_core/engine.md");
    assert!(
        engine.contains("```pycon\n>>> Engine().speed\n"),
        "{engine}"
    );
}

#[test]
fn pages_mirror_the_html_paths() {
    let html = out_dir("mirror_html");
    run(&fixture("sample"), &html, &[]);
    let md = markdown("sample", "mirror_md", &[]);
    let html_pages: BTreeSet<String> = files(&html)
        .into_iter()
        .filter_map(|f| f.strip_suffix(".html").map(str::to_string))
        .collect();
    let md_pages: BTreeSet<String> = files(&md)
        .into_iter()
        .filter_map(|f| f.strip_suffix(".md").map(str::to_string))
        .collect();
    assert_eq!(html_pages, md_pages);
}

#[test]
fn markdown_alone_writes_no_html() {
    let out = markdown("sample", "md_only", &[]);
    let written = files(&out);
    assert!(
        !written
            .iter()
            .any(|f| f.ends_with(".html") || f.ends_with(".js")),
        "{written:?}"
    );
    for file in [
        "index.md",
        "llms.txt",
        "llms-full.txt",
        ".swiftdoc-manifest",
    ] {
        assert!(written.contains(file), "{file} missing: {written:?}");
    }
    let manifest = read(&out, ".swiftdoc-manifest");
    let listed: BTreeSet<String> = manifest.lines().map(str::to_string).collect();
    let mut expected = written.clone();
    expected.remove(".swiftdoc-manifest");
    assert_eq!(listed, expected, "the manifest lists every file written");
}

#[test]
fn all_three_formats_together() {
    let out = out_dir("all_formats");
    run(
        &fixture("sample"),
        &out,
        &[
            "--format", "markdown", "--format", "html", "--format", "json",
        ],
    );
    for file in ["index.html", "index.md", "api.json", "llms.txt"] {
        assert!(out.join(file).is_file(), "{file}");
    }
}

#[test]
fn llms_txt_lists_every_page() {
    let out = markdown("sample", "llms", &[]);
    let llms = read(&out, "llms.txt");
    assert!(
        llms.starts_with("# demo\n\n> Demo package root docstring.\n"),
        "{llms}"
    );
    for link in links(&llms) {
        assert!(out.join(&link).is_file(), "llms.txt: missing {link}");
    }
    let full = read(&out, "llms-full.txt");
    for page in files(&out).iter().filter(|f| f.ends_with(".md")) {
        assert!(
            full.contains(&format!("<!-- {page} -->")),
            "llms-full.txt lacks {page}"
        );
    }
}

#[test]
fn crlf_sources_give_lf_markdown() {
    // Written at test time, so it is CRLF on every OS, as a Windows
    // checkout of the fixtures would be.
    let src = out_dir("crlf_src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("win.py"),
        "\"\"\"Windows module.\r\n\r\nSecond paragraph.\r\n\"\"\"\r\n\r\n\
         def f():\r\n    \"\"\"Does f.\r\n\r\n    #pd-note: careful\r\n    >>> f()\r\n    \"\"\"\r\n",
    )
    .unwrap();
    let out = out_dir("crlf_out");
    run(&src, &out, &["--format", "markdown"]);
    let page = read(&out, "win.md");
    assert!(!page.contains('\r'), "{page:?}");
    assert!(
        page.contains("Windows module.\n\nSecond paragraph."),
        "{page}"
    );
    assert!(page.contains("> **Note:** careful\n"), "{page}");
    assert!(page.contains("```pycon\n>>> f()\n```"), "{page}");
}

#[test]
fn two_runs_are_byte_identical() {
    for fixture_name in ["sample", "nested"] {
        let first = markdown(fixture_name, &format!("same_{fixture_name}_1"), &[]);
        let second = markdown(fixture_name, &format!("same_{fixture_name}_2"), &[]);
        let names = files(&first);
        assert_eq!(names, files(&second));
        for name in &names {
            assert!(
                fs::read(first.join(name)).unwrap() == fs::read(second.join(name)).unwrap(),
                "{fixture_name}: {name} differs between two runs"
            );
        }
    }
}

#[test]
fn nested_classes_render_recursively_with_qualified_anchors() {
    let out = markdown("nested", "nested", &[]);
    let page = read(&out, "compat.md");

    // Doc-links into nested classes and their members.
    for link in [
        "[`compat.Outer.Inner`](./compat.md#class.Outer.Inner)",
        "[`compat.Outer.Inner.Deep.ping`](./compat.md#method.Outer.Inner.Deep.ping)",
        "[`Outer.Inner.LIMIT`](./compat.md#attr.Outer.Inner.LIMIT)",
        "[`compat.Outer.Meta`](./compat.md#class.Outer.Meta)",
    ] {
        assert!(page.contains(link), "missing {link}:\n{page}");
    }
    // A private nested class is hidden, so its link stays text.
    assert!(
        page.contains("`compat.Outer._Private` (not found)"),
        "{page}"
    );

    // Each nested class sits inside its parent's section, one level deeper,
    // under its qualified name.
    let at = |needle: &str| {
        page.find(needle)
            .unwrap_or_else(|| panic!("no {needle}:\n{page}"))
    };
    let outer = at("<a id=\"class.Outer\"></a>\n\n### class `Outer`");
    let inner = at("<a id=\"class.Outer.Inner\"></a>\n\n##### class `Outer.Inner`");
    let deep = at("<a id=\"class.Outer.Inner.Deep\"></a>\n\n###### class `Outer.Inner.Deep`");
    let ping = at("<a id=\"method.Outer.Inner.Deep.ping\"></a>");
    let functions = at("## Functions");
    assert!(outer < inner && inner < deep && deep < ping && ping < functions);
    assert!(
        page.contains("<a id=\"attr.Outer.Inner.LIMIT\"></a>"),
        "{page}"
    );

    // Badges from conditional and TYPE_CHECKING definitions are shown.
    assert!(page.contains("*type-checking*"), "{page}");
    assert!(page.contains("*conditional*"), "{page}");
}

#[test]
fn overloads_share_one_anchor_on_the_first_definition() {
    let out = markdown("nested", "overloads", &[]);
    let page = read(&out, "compat.md");
    assert_eq!(page.matches("<a id=\"fn.scale\"></a>").count(), 1, "{page}");
    let headings = page.matches("### `scale()`").count();
    assert!(headings >= 2, "every overload is still shown:\n{page}");
    let anchor = page.find("<a id=\"fn.scale\"></a>").unwrap();
    let first = page.find("### `scale()`").unwrap();
    assert!(anchor < first, "the anchor is on the first definition");
    assert!(
        !page[anchor + 1..first].contains("### "),
        "nothing between the anchor and the first overload"
    );
}

/// Compares the sample's Markdown with `tests/snapshots/markdown-sample`.
/// Set `SWIFTDOC_UPDATE_SNAPSHOTS=1` to rewrite the snapshots.
#[test]
fn sample_matches_the_snapshots() {
    let out = markdown("sample", "snapshots", &[]);
    let snapshots = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join("markdown-sample");
    let pages: BTreeSet<String> = files(&out)
        .into_iter()
        .filter(|f| f.ends_with(".md") || f.ends_with(".txt"))
        .collect();
    if std::env::var_os("SWIFTDOC_UPDATE_SNAPSHOTS").is_some() {
        let _ = fs::remove_dir_all(&snapshots);
        for page in &pages {
            let target = snapshots.join(page);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(out.join(page), target).unwrap();
        }
    }
    assert_eq!(pages, files(&snapshots), "the set of pages changed");
    // A Windows checkout may turn the snapshot files' LF into CRLF.
    let lf = |text: String| text.replace("\r\n", "\n");
    for page in &pages {
        assert!(
            lf(read(&out, page)) == lf(read(&snapshots, page)),
            "{page} differs from tests/snapshots/markdown-sample/{page}; \
             rerun with SWIFTDOC_UPDATE_SNAPSHOTS=1 if the change is intended"
        );
    }
}
