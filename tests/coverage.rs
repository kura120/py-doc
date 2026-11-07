//! End-to-end tests for `--coverage`, `--fail-under` and
//! `--coverage-format`, against `tests/fixtures/coverage`.
//!
//! Expected counts for the fixture, public API only (3/7). Dunder methods are not counted:
//!   __init__.py  module documented                          1/1
//!   api          module, Public documented; Public.run and
//!                listed not (`Public.__init__` is a dunder;
//!                `unlisted` and `Public._hidden` not public) 2/4
//!   plain        no module docstring; `blank` is whitespace 0/2
//!   guide        `#pd-write` page, not counted
//! With --include-private (5/12): api adds `_hidden` and `unlisted` (2/6),
//! plain adds the documented `_private` (1/3), and `_internal` counts
//! (module undocumented, `helper` documented: 1/2).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("coverage")
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("coverage")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs swiftdoc on the fixture with an empty --config, so the repository's
/// own pyproject.toml is never read.
fn run(test: &str, args: &[&str]) -> Output {
    let dir = scratch(test);
    let config = dir.join("empty.toml");
    fs::write(&config, "").unwrap();
    Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .current_dir(&dir)
        .arg("--config")
        .arg(&config)
        .arg("--src")
        .arg(fixture())
        .args(args)
        .output()
        .expect("failed to run swiftdoc")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "swiftdoc failed:\n{}",
        stderr(output)
    );
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", stdout(output)))
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn text_report_lists_the_worst_module_first_then_the_total() {
    let output = run("text", &["--coverage"]);
    assert_ok(&output);
    let text = stdout(&output);
    let plain = text.find("plain").expect("plain row");
    let api = text.find("api").expect("api row");
    let init = text.find("__init__.py").expect("root row");
    assert!(plain < api && api < init, "{text}");
    assert!(text.contains("0/2") && text.contains("2/4"), "{text}");
    let total = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap();
    assert!(
        total.starts_with("Total") && total.contains("3/7") && total.ends_with("42.9%"),
        "{total}"
    );
    assert!(
        !text.contains("guide"),
        "doc-only pages are not counted:\n{text}"
    );
}

#[test]
fn json_report_follows_the_schema_and_owns_stdout() {
    let output = run("json", &["--coverage", "--coverage-format", "json"]);
    assert_ok(&output);
    // Progress goes to stderr, so all of stdout parses.
    let report = json(&output);
    assert!(stderr(&output).contains("Scanning"));

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["include_private"], false);
    assert_eq!(report["fail_under"], Value::Null);
    assert_eq!(report["passed"], true);
    assert_eq!(report["totals"]["documented"], 3);
    assert_eq!(report["totals"]["total"], 7);
    assert_eq!(report["totals"]["percent"], 42.86);
    assert_eq!(report["kinds"]["module"]["total"], 3);
    assert_eq!(report["kinds"]["class"]["documented"], 1);
    assert_eq!(report["kinds"]["method"]["total"], 1);
    assert_eq!(report["kinds"]["function"]["total"], 2);

    let modules = report["modules"].as_array().unwrap();
    let order: Vec<&str> = modules
        .iter()
        .map(|m| m["module"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["", "api", "plain"], "sorted by module name");
    assert_eq!(modules[0]["path"], "__init__.py");
    assert_eq!(modules[1]["percent"], 50.0);
    assert_eq!(
        names(&modules[1]["undocumented"]),
        ["api.Public.run", "api.listed"]
    );
    assert_eq!(
        names(&report["undocumented"]),
        ["api.Public.run", "api.listed", "plain", "plain.blank"]
    );
}

#[test]
fn include_private_counts_everything() {
    let output = run(
        "private",
        &[
            "--coverage",
            "--coverage-format",
            "json",
            "--include-private",
        ],
    );
    assert_ok(&output);
    let report = json(&output);
    assert_eq!(report["include_private"], true);
    assert_eq!(report["totals"]["documented"], 5);
    assert_eq!(report["totals"]["total"], 12);
    assert_eq!(report["totals"]["percent"], 41.67);
    let missing = names(&report["undocumented"]);
    for name in ["_internal", "api.Public._hidden", "api.unlisted"] {
        assert!(missing.iter().any(|n| n == name), "{name} in {missing:?}");
    }
    assert!(
        !missing.iter().any(|n| n == "plain._private"),
        "it is documented"
    );
}

#[test]
fn fail_under_sets_the_exit_code_and_names_both_numbers() {
    let output = run("fail", &["--fail-under", "50"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("documentation coverage is 42.85%, below --fail-under 50%"),
        "{err}"
    );
    // The report is still printed, so the log shows what is missing.
    assert!(stdout(&output).contains("Total"));

    // The threshold is inclusive.
    assert_ok(&run("pass", &["--fail-under", "42.85"]));
}

#[test]
fn json_records_a_failed_threshold() {
    let output = run(
        "json_fail",
        &["--fail-under=90", "--coverage-format", "json"],
    );
    assert!(!output.status.success());
    let report = json(&output);
    assert_eq!(report["fail_under"], 90.0);
    assert_eq!(report["passed"], false);
}

#[test]
fn coverage_without_out_writes_no_site_even_if_the_config_sets_out() {
    // A project whose pyproject.toml sets `out`: a command-line coverage
    // check ignores it; --out on the command line builds the site too.
    let repo = scratch("config_out");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("pkg")).unwrap();
    fs::write(
        repo.join("pkg/core.py"),
        "\"\"\"Core.\"\"\"\n\ndef run():\n    pass\n",
    )
    .unwrap();
    fs::write(
        repo.join("pyproject.toml"),
        "[tool.swiftdoc]\nsrc = \"pkg\"\nout = \"site\"\nname = \"demo\"\n",
    )
    .unwrap();
    let swiftdoc = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
            .current_dir(&repo)
            .args(args)
            .output()
            .expect("failed to run swiftdoc")
    };

    let output = swiftdoc(&["--coverage"]);
    assert_ok(&output);
    assert!(stdout(&output).contains("1/2"), "{}", stdout(&output));
    assert!(!repo.join("site").exists(), "check-only run wrote a site");

    let output = swiftdoc(&["--coverage", "--out", "cli-site"]);
    assert_ok(&output);
    assert!(repo.join("cli-site/index.html").is_file());
    assert!(!repo.join("site").exists());
    assert!(stdout(&output).contains("Total"));

    // `coverage = true` in the file adds the report to a normal build.
    fs::write(
        repo.join("pyproject.toml"),
        "[tool.swiftdoc]\nsrc = \"pkg\"\nout = \"site\"\nname = \"demo\"\ncoverage = true\n",
    )
    .unwrap();
    let output = swiftdoc(&[]);
    assert_ok(&output);
    assert!(repo.join("site/index.html").is_file());
    assert!(stdout(&output).contains("Total"));
}

#[test]
fn json_stdout_stays_clean_while_building_the_site() {
    let output = run(
        "json_site",
        &[
            "--coverage-format",
            "json",
            "--coverage",
            "--out",
            "site",
            "--name",
            "demo",
        ],
    );
    assert_ok(&output);
    let report = json(&output);
    assert_eq!(report["totals"]["total"], 7);
    assert!(stderr(&output).contains("Generated"));
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("coverage/json_site/site");
    assert!(dir.join("index.html").is_file());
}

#[test]
fn without_coverage_nothing_changes() {
    let output = run("off", &["--out", "site", "--name", "demo"]);
    assert_ok(&output);
    let out = stdout(&output);
    assert!(out.contains("[1/3]") && !out.contains("Total"), "{out}");
}
