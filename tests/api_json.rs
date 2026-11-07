//! `--format json`: what is written, determinism, and validation of real
//! output against `docs/api.schema.json`.
//!
//! The schema is checked with the small validator below instead of a crate:
//! the schema uses a handful of keywords, a full JSON Schema implementation
//! would add dozens of dependencies for one test, and this validator is
//! stricter where it matters for drift. It rejects keywords it does not
//! implement, requires every object property to be listed as required (so a
//! field removed from the output fails), relies on `additionalProperties:
//! false` (so a field added to the output fails), and checks that every
//! definition is exercised by the fixtures.

use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SUPPORTED: &[&str] = &[
    "$schema",
    "$id",
    "$defs",
    "$ref",
    "title",
    "description",
    "type",
    "const",
    "minimum",
    "maxItems",
    "items",
    "properties",
    "required",
    "additionalProperties",
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn out_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("api_json")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Runs py-doc with an empty --config, so the repository's own
/// pyproject.toml is never read.
fn run(src: &Path, out: &Path, extra: &[&str]) -> Output {
    let config = Path::new(env!("CARGO_TARGET_TMPDIR")).join("api-json-empty.toml");
    if !config.exists() {
        fs::write(&config, "").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_py-doc"))
        .arg("--config")
        .arg(&config)
        .arg("--src")
        .arg(src)
        .arg("--out")
        .arg(out)
        .args(["--name", "demo"])
        .args(extra)
        .output()
        .expect("failed to run py-doc");
    assert!(
        output.status.success(),
        "py-doc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn api(out: &Path) -> Value {
    let text = fs::read_to_string(out.join("api.json")).expect("api.json was not written");
    serde_json::from_str(&text).expect("api.json is not JSON")
}

fn schema() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/api.schema.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).expect("schema is not JSON")
}

fn type_matches(name: &str, value: &Value) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_u64() || value.is_i64(),
        "null" => value.is_null(),
        other => panic!("unsupported type `{other}` in the schema"),
    }
}

struct Validator<'a> {
    root: &'a Value,
    used_defs: BTreeSet<String>,
    errors: Vec<String>,
}

impl Validator<'_> {
    fn validate(&mut self, value: &Value, schema: &Value, path: &str) {
        let schema = schema.as_object().expect("a schema must be an object");
        for key in schema.keys() {
            assert!(
                SUPPORTED.contains(&key.as_str()),
                "unsupported schema keyword `{key}`"
            );
        }

        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let name = reference
                .strip_prefix("#/$defs/")
                .unwrap_or_else(|| panic!("unsupported $ref `{reference}`"));
            self.used_defs.insert(name.to_string());
            let target = &self.root["$defs"][name];
            assert!(!target.is_null(), "$ref to a missing definition `{name}`");
            self.validate(value, target, path);
        }

        if let Some(types) = schema.get("type") {
            let ok = match types {
                Value::String(name) => type_matches(name, value),
                Value::Array(names) => names
                    .iter()
                    .any(|name| type_matches(name.as_str().unwrap(), value)),
                _ => panic!("bad `type` at {path}"),
            };
            if !ok {
                self.errors
                    .push(format!("{path}: expected {types}, got {value}"));
                return;
            }
        }
        if let Some(expected) = schema.get("const")
            && value != expected
        {
            self.errors
                .push(format!("{path}: expected {expected}, got {value}"));
        }
        if let (Some(min), Some(n)) = (schema.get("minimum"), value.as_f64())
            && n < min.as_f64().unwrap()
        {
            self.errors.push(format!("{path}: {n} is below {min}"));
        }

        if let Some(items) = value.as_array() {
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
                && items.len() as u64 > max
            {
                self.errors.push(format!(
                    "{path}: {} items, at most {max} allowed",
                    items.len()
                ));
            }
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    self.validate(item, item_schema, &format!("{path}[{i}]"));
                }
            }
        }

        if let Some(object) = value.as_object() {
            self.validate_object(object, schema, path);
        }
    }

    fn validate_object(
        &mut self,
        object: &Map<String, Value>,
        schema: &Map<String, Value>,
        path: &str,
    ) {
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return;
        };
        let required: BTreeSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect())
            .unwrap_or_default();
        let listed: BTreeSet<&str> = properties.keys().map(String::as_str).collect();
        assert_eq!(
            required, listed,
            "at {path}: every property must be required, so a removed field fails"
        );
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "at {path}: objects must set additionalProperties: false"
        );

        for name in &required {
            if !object.contains_key(*name) {
                self.errors.push(format!("{path}: missing `{name}`"));
            }
        }
        for (name, value) in object {
            match properties.get(name) {
                Some(property) => self.validate(value, property, &format!("{path}.{name}")),
                None => self
                    .errors
                    .push(format!("{path}: unexpected field `{name}`")),
            }
        }
    }
}

/// Validates every document against the schema and checks that together
/// they use every definition, so no part of the schema goes untested.
fn assert_valid(documents: &[&Value]) {
    let schema = schema();
    let mut validator = Validator {
        root: &schema,
        used_defs: BTreeSet::new(),
        errors: Vec::new(),
    };
    for document in documents {
        validator.validate(document, &schema, "$");
    }
    assert!(
        validator.errors.is_empty(),
        "api.json does not match docs/api.schema.json:\n{}",
        validator.errors.join("\n")
    );
    let defs: BTreeSet<String> = schema["$defs"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        validator.used_defs, defs,
        "some definitions are never exercised"
    );
}

fn module<'a>(api: &'a Value, qualname: &str) -> &'a Value {
    api["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["qualname"] == qualname)
        .unwrap_or_else(|| panic!("no module `{qualname}`"))
}

fn names(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect()
}

#[test]
fn output_matches_the_schema() {
    let sample = out_dir("schema_sample");
    run(&fixture("sample"), &sample, &["--format", "json"]);
    let visibility = out_dir("schema_visibility");
    run(
        &fixture("visibility"),
        &visibility,
        &["--format", "json", "--include-private"],
    );
    let macros = out_dir("schema_macros");
    run(&fixture("macros"), &macros, &["--format", "json"]);
    let variables = out_dir("schema_variables");
    run(
        &fixture("variables"),
        &variables,
        &["--format", "json", "--include-private"],
    );
    let nested = out_dir("schema_nested");
    run(
        &fixture("nested"),
        &nested,
        &["--format", "json", "--include-private"],
    );
    assert_valid(&[
        &api(&sample),
        &api(&visibility),
        &api(&macros),
        &api(&variables),
        &api(&nested),
    ]);
}

#[test]
fn nested_classes_and_branch_badges_are_included() {
    let out = out_dir("nested");
    run(&fixture("nested"), &out, &["--format", "json"]);
    let api = api(&out);
    let compat = module(&api, "compat");
    let classes = compat["classes"].as_array().unwrap();
    assert_eq!(names(&compat["classes"]), ["Outer"]);
    let outer = &classes[0];
    assert_eq!(outer["qualname"], "compat.Outer");
    let nested = outer["classes"].as_array().unwrap();
    assert_eq!(
        names(&outer["classes"]),
        ["Inner", "Meta"],
        "private nested classes are filtered"
    );
    let inner = &nested[0];
    assert_eq!(inner["qualname"], "compat.Outer.Inner");
    assert_eq!(
        inner["variables"][0]["qualname"],
        "compat.Outer.Inner.LIMIT"
    );
    let deep = &inner["classes"][0];
    assert_eq!(deep["qualname"], "compat.Outer.Inner.Deep");
    assert_eq!(
        deep["methods"][0]["qualname"],
        "compat.Outer.Inner.Deep.ping"
    );
    assert_eq!(outer["methods"][0]["badges"][0], "conditional");

    let functions = compat["functions"].as_array().unwrap();
    let read_config = functions
        .iter()
        .find(|f| f["name"] == "read_config")
        .unwrap();
    assert_eq!(read_config["badges"][0], "conditional");
    let handler = compat["variables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "Handler")
        .unwrap();
    assert_eq!(handler["badges"][0], "type-checking");
}

#[test]
fn variables_and_attributes_are_included() {
    let out = out_dir("variables");
    run(&fixture("variables"), &out, &["--format", "json"]);
    let api = api(&out);
    let config = module(&api, "config");
    let variables = config["variables"].as_array().unwrap();
    let find = |list: &[Value], name: &str| -> Value {
        list.iter()
            .find(|v| v["name"] == name)
            .unwrap_or_else(|| panic!("no variable `{name}`"))
            .clone()
    };

    let retries = find(variables, "MAX_RETRIES");
    assert_eq!(retries["qualname"], "config.MAX_RETRIES");
    assert_eq!(retries["value"], "3");
    assert_eq!(retries["docstring"], "How often a request is retried.");
    assert_eq!(
        retries["docstring_html"],
        "<p>How often a request is retried.</p>\n"
    );

    let timeout = find(variables, "TIMEOUT");
    assert_eq!(timeout["annotation"], "float");
    assert!(
        timeout["docstring"]
            .as_str()
            .unwrap()
            .contains("Seconds to wait")
    );

    assert_eq!(find(variables, "JsonValue")["is_type_alias"], true);
    assert_eq!(find(variables, "PALETTE")["value_truncated"], true);
    assert!(
        !variables.iter().any(|v| v["name"] == "NOT_EXPORTED"),
        "__all__ filters variables too"
    );

    let mode = config["classes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Mode")
        .unwrap();
    let fast = find(mode["variables"].as_array().unwrap(), "FAST");
    assert_eq!(fast["qualname"], "config.Mode.FAST");
    assert_eq!(fast["docstring"], "Go fast.");
    assert!(
        fast["badges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b == "enum")
    );
}

#[test]
fn validator_catches_drift() {
    let out = out_dir("drift");
    run(&fixture("sample"), &out, &["--format", "json"]);
    let good = api(&out);
    let schema = schema();
    let check = |document: &Value| {
        let mut validator = Validator {
            root: &schema,
            used_defs: BTreeSet::new(),
            errors: Vec::new(),
        };
        validator.validate(document, &schema, "$");
        validator.errors
    };
    assert!(check(&good).is_empty());

    let mut added = good.clone();
    added["modules"][0]["surprise"] = Value::Bool(true);
    assert!(
        check(&added)
            .iter()
            .any(|e| e.contains("unexpected field `surprise`"))
    );

    let mut removed = good.clone();
    removed["modules"][0]
        .as_object_mut()
        .unwrap()
        .remove("page");
    assert!(check(&removed).iter().any(|e| e.contains("missing `page`")));

    let mut retyped = good.clone();
    retyped["include_private"] = Value::from("no");
    assert!(!check(&retyped).is_empty());

    let mut variables = good;
    variables["modules"][0]["variables"] = serde_json::json!([{ "name": "X" }]);
    assert!(
        check(&variables)
            .iter()
            .any(|e| e.contains("variables[0]: missing `qualname`")),
        "a malformed variable must fail"
    );
}

#[test]
fn json_alone_writes_api_json_and_the_manifest_only() {
    let out = out_dir("json_only");
    run(&fixture("visibility"), &out, &["--format", "json"]);
    let mut files: Vec<String> = fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, [".py-doc-manifest", "api.json"]);
    let manifest = fs::read_to_string(out.join(".py-doc-manifest")).unwrap();
    assert_eq!(manifest, "api.json\n");
}

#[test]
fn html_and_json_together_write_both() {
    let out = out_dir("both");
    run(
        &fixture("sample"),
        &out,
        &["--format", "html", "--format", "json"],
    );
    assert!(out.join("index.html").is_file());
    assert!(out.join("search-index.js").is_file());
    assert!(out.join("api.json").is_file());
    let manifest = fs::read_to_string(out.join(".py-doc-manifest")).unwrap();
    assert!(manifest.lines().any(|l| l == "api.json"));
    assert!(manifest.lines().any(|l| l == "index.html"));
}

#[test]
fn default_format_is_html_only() {
    let out = out_dir("default");
    run(&fixture("sample"), &out, &[]);
    assert!(out.join("index.html").is_file());
    assert!(!out.join("api.json").exists());
}

#[test]
fn clean_removes_the_site_when_switching_to_json() {
    let out = out_dir("switch");
    run(&fixture("sample"), &out, &[]);
    assert!(out.join("index.html").is_file());
    run(&fixture("sample"), &out, &["--format", "json", "--clean"]);
    assert!(!out.join("index.html").exists());
    assert!(out.join("api.json").is_file());
}

#[test]
fn two_runs_are_byte_identical() {
    let first = out_dir("same_1");
    let second = out_dir("same_2");
    run(&fixture("sample"), &first, &["--format", "json"]);
    run(&fixture("sample"), &second, &["--format", "json"]);
    let a = fs::read(first.join("api.json")).unwrap();
    let b = fs::read(second.join("api.json")).unwrap();
    assert!(a == b, "api.json differs between two runs");
}

#[test]
fn modules_are_sorted_and_members_keep_source_order() {
    let out = out_dir("order");
    run(&fixture("sample"), &out, &["--format", "json"]);
    let api = api(&out);
    let keys: Vec<(String, String)> = api["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["qualname"].as_str().unwrap().to_string(),
                m["path"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    // No ordering prefix, no build-machine path.
    assert!(
        keys.iter()
            .all(|(q, p)| !q.contains("01_") && !p.starts_with('/'))
    );
}

#[test]
fn raw_and_rendered_docstrings_are_both_included() {
    let out = out_dir("docstrings");
    run(&fixture("sample"), &out, &["--format", "json"]);
    let api = api(&out);
    let core = module(&api, "core");
    assert_eq!(core["docstring"], "Core package.\n\nHolds the engine.\n");
    assert_eq!(
        core["docstring_html"],
        "<p>Core package.</p>\n<p>Holds the engine.</p>\n"
    );
    assert_eq!(core["summary"], "Core package.");
    assert_eq!(core["page"], "01_core/index.html");
    let boot = &core["functions"][0];
    assert_eq!(boot["qualname"], "core.boot");
    assert_eq!(boot["docstring"], "Boot it.");

    assert_eq!(api["schema_version"], 1);
    assert_eq!(api["py_doc_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(api["package"]["name"], "demo");
    assert_eq!(api["package"]["version"], "1.0.0");
}

#[test]
fn public_filtering_applies_and_include_private_keeps_everything() {
    let public = out_dir("public");
    run(&fixture("visibility"), &public, &["--format", "json"]);
    let private = out_dir("private");
    run(
        &fixture("visibility"),
        &private,
        &["--format", "json", "--include-private"],
    );
    let (public, private) = (api(&public), api(&private));
    assert_eq!(public["include_private"], false);
    assert_eq!(private["include_private"], true);

    let count = |api: &Value| api["modules"].as_array().unwrap().len();
    assert!(count(&private) > count(&public), "private modules are kept");
    let has_private = |api: &Value| {
        api["modules"].as_array().unwrap().iter().any(|m| {
            m["is_private"] == true
                || m["functions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f["is_private"] == true)
                || m["classes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["is_private"] == true)
        })
    };
    assert!(!has_private(&public));
    assert!(has_private(&private));

    let api_module = module(&public, "api");
    // __all__ decides: a listed `_name` is public, an unlisted name is not.
    assert!(api_module["all"].is_array(), "the fixture defines __all__");
    assert_eq!(
        names(&api_module["functions"]),
        ["connect", "_exported_private"]
    );
}
