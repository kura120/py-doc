//! End-to-end tests for reading settings from pyproject.toml.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway project: `<test>/repo` holding a `.git` marker, a
/// pyproject.toml with `pyproject` as its text, and `src/pkg/core.py`.
fn project(test: &str, pyproject: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("config")
        .join(test);
    let _ = fs::remove_dir_all(&root);
    let repo = root.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("src/pkg")).unwrap();
    fs::create_dir_all(repo.join("sub")).unwrap();
    fs::write(
        repo.join("src/pkg/core.py"),
        "\"\"\"Core module.\"\"\"\n\ndef run():\n    \"\"\"Runs.\"\"\"\n",
    )
    .unwrap();
    if !pyproject.is_empty() {
        fs::write(repo.join("pyproject.toml"), pyproject).unwrap();
    }
    repo
}

fn swiftdoc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("failed to run swiftdoc")
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "swiftdoc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const CONFIGURED: &str = r#"
[project]
name = "from-project"
version = "2.3.4"

[tool.swiftdoc]
src = "src/pkg"
out = "site"
layout = "modern"
"#;

#[test]
fn no_args_in_a_configured_repo_builds_the_site() {
    let repo = project("no_args", CONFIGURED);
    let output = swiftdoc(&repo, &[]);
    assert_ok(&output);

    let index = fs::read_to_string(repo.join("site/index.html")).unwrap();
    assert!(index.contains("<title>from-project - Documentation Index</title>"));
    assert!(index.contains("v2.3.4"));
    assert!(index.contains("data-layout=\"modern\""));
    assert!(repo.join("site/core.html").is_file());
}

#[test]
fn config_paths_are_relative_to_the_pyproject() {
    let repo = project("relative_config", CONFIGURED);
    // Run from a subdirectory: `src` and `out` still resolve against the repo root.
    let output = swiftdoc(&repo.join("sub"), &["--config", "../pyproject.toml"]);
    assert_ok(&output);
    assert!(repo.join("site/index.html").is_file());
    assert!(!repo.join("sub/site").exists());
}

#[test]
fn css_in_the_pyproject_is_relative_to_it() {
    let pyproject = format!("{CONFIGURED}css = [\"docs/theme.css\"]\n");
    let repo = project("css_config", &pyproject);
    fs::create_dir_all(repo.join("docs")).unwrap();
    fs::write(
        repo.join("docs/theme.css"),
        ":root { --accent-blue: #2563eb; }\n",
    )
    .unwrap();
    let output = swiftdoc(&repo.join("sub"), &["--config", "../pyproject.toml"]);
    assert_ok(&output);
    let copied = fs::read_to_string(repo.join("site/custom/0-theme.css")).unwrap();
    assert!(copied.contains("#2563eb"));
    let index = fs::read_to_string(repo.join("site/index.html")).unwrap();
    assert!(index.contains("href=\"./custom/0-theme.css\""));
}

#[test]
fn cli_paths_are_relative_to_the_working_directory() {
    let repo = project("relative_cli", CONFIGURED);
    let output = swiftdoc(
        &repo.join("sub"),
        &["--src", "../src/pkg", "--out", "cli-site"],
    );
    assert_ok(&output);
    // The pyproject.toml is found from --src upwards; --out beats its `out`.
    let index = fs::read_to_string(repo.join("sub/cli-site/index.html")).unwrap();
    assert!(index.contains("from-project"));
    assert!(!repo.join("site").exists());
}

#[test]
fn cli_beats_tool_beats_project() {
    let repo = project(
        "precedence",
        r#"
[project]
name = "from-project"
version = "2.3.4"

[tool.swiftdoc]
src = "src/pkg"
out = "site"
name = "from-tool"
"#,
    );
    let output = swiftdoc(&repo, &["--doc-version", "9.9.9"]);
    assert_ok(&output);
    let index = fs::read_to_string(repo.join("site/index.html")).unwrap();
    assert!(index.contains("from-tool - Documentation Index"));
    assert!(index.contains("v9.9.9"));
}

#[test]
fn unknown_keys_warn_and_fail_under_strict() {
    let repo = project(
        "unknown",
        "[tool.swiftdoc]\nsrc = \"src/pkg\"\nout = \"site\"\nname = \"x\"\nlayuot = \"modern\"\n",
    );
    let lenient = swiftdoc(&repo, &[]);
    assert_ok(&lenient);
    assert!(
        stderr(&lenient).contains("`layuot`"),
        "{}",
        stderr(&lenient)
    );

    let strict = swiftdoc(&repo, &["--strict"]);
    assert!(!strict.status.success());
    assert!(stderr(&strict).contains("`layuot`"), "{}", stderr(&strict));
}

#[test]
fn dynamic_version_falls_back_with_a_hint() {
    let repo = project(
        "dynamic",
        "[project]\nname = \"dyn\"\ndynamic = [\"version\"]\n",
    );
    let output = swiftdoc(&repo, &["--src", "src/pkg", "--out", "site"]);
    assert_ok(&output);
    assert!(stderr(&output).contains("dynamic"), "{}", stderr(&output));
    let index = fs::read_to_string(repo.join("site/index.html")).unwrap();
    assert!(index.contains("dyn - Documentation Index"));
    assert!(index.contains("v1.0.0"));
}

#[test]
fn flags_alone_work_without_a_pyproject() {
    let repo = project("flags_only", "");
    let output = swiftdoc(
        &repo,
        &["--src", "src/pkg", "--out", "site", "--name", "plain"],
    );
    assert_ok(&output);
    let index = fs::read_to_string(repo.join("site/index.html")).unwrap();
    assert!(index.contains("plain - Documentation Index"));
    assert!(index.contains("v1.0.0"));
}

#[test]
fn missing_name_is_reported() {
    let repo = project("missing_name", "");
    let output = swiftdoc(&repo, &["--src", "src/pkg", "--out", "site"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--name"), "{}", stderr(&output));
}

#[test]
fn no_args_without_config_or_terminal_exits() {
    // Only [project]: not configured, so swiftdoc wants the wizard, and there
    // is no terminal in tests.
    let repo = project("no_tty", "[project]\nname = \"p\"\n");
    let output = swiftdoc(&repo, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("no interactive terminal"));
    assert!(!repo.join("site").exists());
}
