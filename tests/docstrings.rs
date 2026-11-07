//! Docstring sections: Google, NumPy and reST styles, mixed and malformed
//! input, macros inside descriptions and `--docstring-style`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn strip_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("docstrings")
}

/// Generates the `docstrings` fixture with an empty `--config`; returns the
/// output dir and the run's stdout + stderr.
fn generate(test: &str, extra: &[&str]) -> (PathBuf, String) {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("docstrings");
    let out = base.join(test);
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&base).unwrap();
    let config = base.join("empty.toml");
    fs::write(&config, "").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .arg("--config")
        .arg(&config)
        .arg("--src")
        .arg(fixture())
        .arg("--out")
        .arg(&out)
        .args(["--name", "demo"])
        .args(extra)
        .output()
        .expect("failed to run swiftdoc");
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "swiftdoc failed:\n{log}");
    (out, log)
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {file}: {e}"))
}

/// The HTML of the card with `id`, up to the next card.
fn card<'a>(page: &'a str, id: &str) -> &'a str {
    let start = page
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("no {id}"));
    let rest = &page[start + 1..];
    let end = rest
        .find("<div class=\"method-item\" id=")
        .into_iter()
        .chain(rest.find("<div class=\"item-card\" id="))
        .min()
        .unwrap_or(rest.len());
    &page[start..start + 1 + end]
}

/// (name, type, default) of each entry under a section title, as rendered.
fn entries(card: &str, title: &str) -> Vec<String> {
    let start = card
        .find(&format!("<p class=\"doc-section-title\">{title}</p>"))
        .unwrap_or_else(|| panic!("no {title} section in {card}"));
    let section = &card[start..];
    let section = &section[..section.find("</ul>").unwrap()];
    section
        .split("<li class=\"doc-item\">")
        .skip(1)
        .map(|li| {
            let head = li.split("</span>").next().unwrap_or("");
            let text = |class: &str| {
                // The text up to `</code>`, without tags (a type may hold links).
                head.split(&format!("class=\"{class}\">"))
                    .nth(1)
                    .map(|t| strip_tags(t.split("</code>").next().unwrap()))
            };
            let default = head
                .split("class=\"doc-item-default\">default <code>")
                .nth(1)
                .map(|t| t.split('<').next().unwrap().to_string());
            format!(
                "{}|{}|{}",
                text("doc-item-name").unwrap_or_default(),
                text("doc-item-type").unwrap_or_default(),
                default.unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn google_sections_render_with_signature_types_and_defaults() {
    let (out, log) = generate("google", &[]);
    let page = read(&out, "google.html");
    let run = card(&page, "method.Engine.run");
    assert_eq!(
        entries(run, "Parameters"),
        [
            "job|str|",
            "*args||",
            "retries|int|1",
            "**kwargs||",
            // Documented but not in the signature: kept, never invented.
            "ghost||",
        ]
    );
    assert_eq!(entries(run, "Returns"), ["|bool|"]);
    assert!(run.contains("True if the job <strong>succeeded</strong>."));
    assert_eq!(entries(run, "Raises"), ["|ValueError|", "|RuntimeError|"]);
    assert!(run.contains(
        "<p class=\"doc-section-title\">Examples</p><pre><code class=\"language-pycon\">"
    ));
    assert!(run.contains("class=\"alert alert-note\""));
    assert!(run.contains("Jobs run one at a time."));
    assert!(run.contains("class=\"alert alert-warning\""));
    // The rest of the docstring is still the docstring.
    assert!(run.contains("<p>Some more text about running.</p>"));
    assert!(!run.contains("Args:"));

    // Class docstrings take their defaults from __init__.
    let engine = card(&page, "class.Engine");
    assert_eq!(
        entries(engine, "Parameters"),
        ["name|str|&quot;main&quot;", "speed|int|3"]
    );
    assert_eq!(entries(engine, "Attributes"), ["speed|int|", "name||"]);

    assert_eq!(
        entries(card(&page, "method.Engine.stream"), "Yields"),
        ["|str|"]
    );
    // `Returns: text` on one line, typed from the signature.
    let copy = card(&page, "method.Engine.copy");
    assert_eq!(entries(copy, "Returns"), ["|Engine|"]);
    assert!(copy.contains("a copy of self."));
    assert!(!log.contains('⚠'), "{log}");
}

#[test]
fn macros_work_inside_section_descriptions() {
    let (out, _) = generate("macros", &[]);
    let page = read(&out, "google.html");
    let run = card(&page, "method.Engine.run");
    assert!(run.contains("href=\"./google.html#class.Engine\" class=\"doc-internal-link\""));
    assert!(run.contains("Retries back off exponentially."));
    assert!(!run.contains("#pd-"));
}

#[test]
fn numpy_sections() {
    let (out, _) = generate("numpy", &[]);
    let page = read(&out, "numpy_style.html");
    let mean = card(&page, "fn.mean");
    assert_eq!(
        entries(mean, "Parameters"),
        ["values|array_like|", "axis|int|0", "weights||None"]
    );
    assert_eq!(entries(mean, "Returns"), ["|float|"]);
    assert_eq!(entries(mean, "Raises"), ["|ZeroDivisionError|"]);
    assert!(mean.contains("<p class=\"doc-section-title\">See also</p>"));
    assert!(mean.contains("The mean is the sum divided by the count."));
    assert!(!mean.contains("----------"));
    assert_eq!(
        entries(card(&page, "fn.split"), "Returns"),
        ["head|str|", "tail|str|"]
    );
}

#[test]
fn rest_fields() {
    let (out, _) = generate("rest", &[]);
    let page = read(&out, "rest_style.html");
    let connect = card(&page, "fn.connect");
    assert_eq!(
        entries(connect, "Parameters"),
        ["host|str|", "port|int|5432"]
    );
    assert_eq!(entries(connect, "Keyword arguments"), ["timeout|float|1.0"]);
    assert_eq!(entries(connect, "Raises"), ["|ConnectionError|"]);
    assert_eq!(entries(connect, "Returns"), ["|Connection|"]);
    assert!(connect.contains("Connections are not thread-safe."));
    assert!(connect.contains("Close connections you open."));
    assert!(!connect.contains(":param"));
}

#[test]
fn mixed_malformed_and_lookalike_docstrings_keep_their_text() {
    let (out, _) = generate("odd", &[]);
    let page = read(&out, "odd.html");

    let mixed = card(&page, "fn.mixed");
    assert_eq!(entries(mixed, "Parameters"), ["a|int|", "b|int|"]);
    assert_eq!(entries(mixed, "Raises"), ["|ValueError|"]);
    assert!(mixed.contains(":meta private:"));

    // A half-written Args section stays text, word for word.
    let malformed = card(&page, "fn.malformed");
    assert!(!malformed.contains("doc-section"));
    for text in [
        "Args:",
        "this line is not a parameter",
        "x: Fine on its own.",
        "Returns:",
    ] {
        assert!(malformed.contains(text), "{text} dropped");
    }

    let lookalike = card(&page, "fn.lookalike");
    assert!(!lookalike.contains("doc-section"));
    assert!(lookalike.contains("x: inside a code block"));
}

#[test]
fn markdown_style_turns_sections_off() {
    let (out, _) = generate("markdown", &["--docstring-style", "markdown"]);
    for page in [
        "google.html",
        "numpy_style.html",
        "rest_style.html",
        "odd.html",
    ] {
        assert!(!read(&out, page).contains("doc-section"), "{page}");
    }
    let page = read(&out, "google.html");
    assert!(page.contains("Args:"));
}

#[test]
fn forced_style_reads_only_that_style() {
    let (out, _) = generate("forced", &["--docstring-style", "rest"]);
    assert!(read(&out, "rest_style.html").contains("doc-section"));
    let google = read(&out, "google.html");
    assert!(!google.contains("doc-section"));
    assert!(google.contains("Args:"));
}

#[test]
fn style_can_be_set_in_pyproject() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("docstrings");
    fs::create_dir_all(&base).unwrap();
    let config = base.join("markdown.toml");
    fs::write(&config, "[tool.swiftdoc]\ndocstring-style = \"markdown\"\n").unwrap();
    let out = base.join("pyproject");
    let _ = fs::remove_dir_all(&out);
    let output = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .arg("--config")
        .arg(&config)
        .arg("--src")
        .arg(fixture())
        .arg("--out")
        .arg(&out)
        .args(["--name", "demo"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!read(&out, "google.html").contains("doc-section"));
}
