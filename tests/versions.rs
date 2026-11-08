//! `--version-dir`: several releases side by side with a version switcher.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("versions")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("empty.toml"), "").unwrap();
    dir
}

fn swiftdoc(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .current_dir(dir)
        .args(["--config", "empty.toml", "--name", "demo", "--src"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample"))
        .args(args)
        .output()
        .expect("failed to run swiftdoc")
}

fn ok(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn two_versions_share_one_root_with_a_switcher() {
    let dir = scratch("two");
    ok(swiftdoc(
        &dir,
        &["--version-dir", "site", "--doc-version", "1.0.0"],
    ));
    ok(swiftdoc(
        &dir,
        &["--version-dir", "site", "--doc-version", "2.0.0"],
    ));
    let root = dir.join("site");

    let versions: Value =
        serde_json::from_str(&fs::read_to_string(root.join("versions.json")).unwrap()).unwrap();
    assert_eq!(
        versions,
        serde_json::json!([
            { "version": "2.0.0", "path": "2.0.0", "latest": true },
            { "version": "1.0.0", "path": "1.0.0", "latest": false },
        ])
    );
    let redirect = fs::read_to_string(root.join("index.html")).unwrap();
    assert!(redirect.contains(r#"url=2.0.0/index.html""#), "{redirect}");

    // Both builds have the switcher, which reads ../versions.json.
    for version in ["1.0.0", "2.0.0"] {
        let page = fs::read_to_string(root.join(version).join("01_core/engine.html")).unwrap();
        assert!(
            page.contains(r#"id="version-switcher""#)
                && page.contains(&format!(r#"data-current="{version}""#)),
            "{page}"
        );
    }

    // Rebuilding an old version keeps the list and the newest as latest.
    ok(swiftdoc(
        &dir,
        &["--version-dir", "site", "--doc-version", "1.0.0"],
    ));
    let again = fs::read_to_string(root.join("versions.json")).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&again).unwrap(), versions);
}

#[test]
fn single_version_builds_are_unchanged() {
    let dir = scratch("single");
    ok(swiftdoc(&dir, &["--out", "site"]));
    let page = fs::read_to_string(dir.join("site/index.html")).unwrap();
    assert!(
        page.contains(r#"<div class="version-badge">v1.0.0</div>"#),
        "{page}"
    );
    assert!(!page.contains("version-switcher"));
    assert!(!dir.join("site/versions.json").exists());
}

#[test]
fn a_version_must_be_a_folder_name() {
    let dir = scratch("bad");
    let output = swiftdoc(&dir, &["--version-dir", "site", "--doc-version", "1/2"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("folder name"));
}
