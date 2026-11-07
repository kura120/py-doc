//! Links in type annotations, base classes and docstring types.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("annotations")
}

/// Generates the fixture with an empty `--config` and `--strict` (so any
/// warning fails); returns the output dir.
fn generate(test: &str, extra: &[&str]) -> PathBuf {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("annotations");
    let out = base.join(test);
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&base).unwrap();
    let config = base.join("empty.toml");
    fs::write(&config, "").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--config")
        .arg(&config)
        .arg("--src")
        .arg(fixture())
        .arg("--out")
        .arg(&out)
        .args(["--name", "annotations", "--strict"])
        .args(extra)
        .output()
        .expect("failed to run py-doc");
    let log = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "py-doc failed:\n{log}");
    assert!(!log.contains('⚠'), "unexpected warning:\n{log}");
    out
}

fn read(out: &Path, file: &str) -> String {
    fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("cannot read {file}: {e}"))
}

/// The signature (`<span class="signature-code">...`) of the card with `id`.
fn signature<'a>(page: &'a str, id: &str) -> &'a str {
    let start = page
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("no {id}"));
    let rest = &page[start..];
    let code = rest.find("<span class=\"signature-code\">").unwrap();
    let end = code
        + rest[code..]
            .find("<span class=\"signature-meta\">")
            .unwrap();
    &rest[code..end]
}

fn link(href: &str, text: &str) -> String {
    format!("<a class=\"type-link\" href=\"{href}\">{text}</a>")
}

#[test]
fn return_types_parameters_and_variables_link() {
    let out = generate("signatures", &[]);
    let console = read(&out, "console.html");

    // `-> ConsoleOptions`, defined in the same module.
    assert!(
        signature(&console, "method.Console.copy").contains(&format!(
            "(self) -> {}:",
            link("./console.html#class.ConsoleOptions", "ConsoleOptions")
        ))
    );

    // Nested generics, a string forward reference, `|` unions and None: each
    // name links on its own and the rest stays escaped text.
    let render = signature(&console, "method.Console.render");
    let style = link("./style.html#class.Style", "Style");
    let button = link("./widgets/button.html#class.Button", "Button");
    for expected in [
        format!("style: Optional[{style}] = None"),
        format!("renderables: dict[str, list[&quot;{button}&quot;]] = {{}}"),
        format!("on_done: Callable[[int], Optional[{style}]] | None = None"),
        // Literal values are not names.
        "mode: Literal[&quot;Style&quot;, &quot;plain&quot;] = &quot;plain&quot;".to_string(),
        format!(") -> {style} | None:"),
    ] {
        assert!(render.contains(&expected), "{expected}\nnot in\n{render}");
    }

    let variables = [
        (
            "var.DEFAULT_STYLE",
            format!("DEFAULT_STYLE</span>: {style} = Style()"),
        ),
        (
            "var.COLORS",
            format!(
                "COLORS</span>: dict[str, {}] = {{}}",
                link("./style.html#class.Color", "Color")
            ),
        ),
    ];
    for (id, expected) in variables {
        assert!(signature(&console, id).contains(&expected), "{id}");
    }

    // `import annotations.widgets.button`: the package's own absolute path.
    assert!(signature(&console, "method.Console.find").contains(&link(
        "./widgets/button.html#class.Button",
        "annotations.widgets.button.Button"
    )));
}

#[test]
fn base_classes_link_through_aliased_and_relative_imports() {
    let out = generate("bases", &[]);
    let console = read(&out, "console.html");
    // `from . import style as style_module`
    assert!(signature(&console, "class.Themed").contains(&format!(
        "({}):",
        link("./style.html#class.Style", "style_module.Style")
    )));
    // A page one folder down: links go up to the root first.
    let button = read(&out, "widgets/button.html");
    // `from ..style import Style as BaseStyle`
    assert!(signature(&button, "class.Button").contains(&format!(
        "({}):",
        link("../style.html#class.Style", "BaseStyle")
    )));
    let press = signature(&button, "fn.press");
    assert!(press.contains(&format!(
        "console: {}, times: int = 1",
        link("../console.html#class.Console", "Console")
    )));
    assert!(press.contains(&link("../widgets/button.html#class.Button", "Button")));
}

#[test]
fn builtins_and_other_packages_stay_plain_without_warnings() {
    // `generate` runs with --strict and fails on any warning.
    let out = generate("plain", &[]);
    let console = read(&out, "console.html");
    let dump = signature(&console, "method.Console.dump");
    assert!(!dump.contains("type-link"), "{dump}");
    // `import json` is the standard library, not the package's json.py.
    assert!(dump.contains("decoder: json.JSONDecoder, data: collections.OrderedDict"));
    assert!(!signature(&console, "class.Registry").contains("type-link"));
    // A string return annotation links like any other; an external base does not.
    let style = read(&out, "style.html");
    assert!(signature(&style, "fn.parse_style").contains(&format!(
        "(text: str) -> {}:",
        link("./style.html#class.Style", "Style")
    )));
    assert!(signature(&style, "class.Color").contains("(Enum):"));
}

#[test]
fn docstring_section_types_link_including_types_from_the_signature() {
    let out = generate("sections", &[]);
    let console = read(&out, "console.html");
    let start = console.find("id=\"method.Console.render\"").unwrap();
    let card = &console[start..];
    let card = &card[..card.find("id=\"method.Console.find\"").unwrap()];
    assert!(card.contains(&format!(
        "<code class=\"doc-item-type\">Optional[{}]</code>",
        link("./style.html#class.Style", "Style")
    )));
    assert!(card.contains(&format!(
        "<code class=\"doc-item-type\">dict[str, list[&quot;{}&quot;]]</code>",
        link("./widgets/button.html#class.Button", "Button")
    )));
}

#[test]
fn api_json_keeps_annotations_as_written_and_lists_targets() {
    let out = generate("json", &["--format", "json"]);
    let api: serde_json::Value = serde_json::from_str(&read(&out, "api.json")).unwrap();
    let console = api["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["qualname"] == "console")
        .unwrap();
    let class = console["classes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Console")
        .unwrap();
    let render = class["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "render")
        .unwrap();
    assert_eq!(render["return_type"], "Style | None");
    assert_eq!(
        render["args"][2],
        "renderables: dict[str, list[\"Button\"]] = {}"
    );
    assert_eq!(
        render["type_links"],
        serde_json::json!([
            { "name": "Style", "target": "style.html#class.Style" },
            { "name": "Button", "target": "widgets/button.html#class.Button" },
        ])
    );
    assert_eq!(
        render["sections"]["params"][0]["type_links"],
        serde_json::json!([{ "name": "Style", "target": "style.html#class.Style" }])
    );
    let dump = class["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "dump")
        .unwrap();
    assert_eq!(dump["type_links"], serde_json::json!([]));
}

#[test]
fn markdown_lists_linked_types_under_signatures() {
    let out = generate("markdown", &["--format", "markdown"]);
    let console = read(&out, "console.md");
    assert!(console.contains(
        "Types: [`Style`](./style.md#class.Style), [`Button`](./widgets/button.md#class.Button)"
    ));
    let button = read(&out, "widgets/button.md");
    assert!(button.contains("Types: [`BaseStyle`](../style.md#class.Style)"));
    // A docstring type that is one documented name links too.
    assert!(!console.contains("Types: []"));
}
