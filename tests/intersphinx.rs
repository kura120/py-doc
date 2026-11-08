//! `--intersphinx`: links to other projects' docs through their Sphinx
//! inventories. Runs offline: the inventory is placed in the cache first,
//! as an earlier build would have left it.

use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("intersphinx")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("empty.toml"), "").unwrap();
    dir
}

fn swiftdoc(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .current_dir(dir)
        .env("SWIFTDOC_CACHE_DIR", dir.join("cache"))
        .args(["--config", "empty.toml", "--out", "site", "--name", "demo"])
        .arg("--src")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/intersphinx/pkg"))
        .args(args)
        .output()
        .expect("failed to run swiftdoc")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn annotations_and_doc_links_use_cached_inventories() {
    let dir = scratch("cached");
    let mut inventory = b"# Sphinx inventory version 2\n# Project: Python\n# Version: 3.13\n\
                          # The remainder of this file is compressed using zlib.\n"
        .to_vec();
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(
            b"pathlib.Path py:class 1 library/pathlib.html#$ -\n\
              os.getcwd py:function 1 library/os.html#os.getcwd -\n",
        )
        .unwrap();
    inventory.extend(encoder.finish().unwrap());
    let cache = dir.join("cache/intersphinx");
    fs::create_dir_all(&cache).unwrap();
    // The cache file is named after the URL; see intersphinx::cache_file_name.
    fs::write(cache.join("https___docs.python.org_3.inv"), inventory).unwrap();

    let output = swiftdoc(
        &dir,
        &["--intersphinx", "python=https://docs.python.org/3/"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let page = fs::read_to_string(dir.join("site/paths.html")).unwrap();
    let path = r#"href="https://docs.python.org/3/library/pathlib.html#pathlib.Path""#;
    // `-> Path` (from pathlib import Path) and `pathlib.Path` both link.
    assert_eq!(page.matches(path).count(), 2, "{page}");
    assert!(
        page.contains(r#"href="https://docs.python.org/3/library/os.html#os.getcwd""#),
        "#pd-doc-link falls back to the inventory:\n{page}"
    );
    assert!(
        !stderr(&output).contains("not found"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_unreachable_inventory_warns_and_fails_strict_builds() {
    let dir = scratch("offline");
    let spec = "nowhere=http://127.0.0.1:9/docs";
    let output = swiftdoc(&dir, &["--intersphinx", spec]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--intersphinx nowhere="),
        "{}",
        stderr(&output)
    );
    assert!(dir.join("site/paths.html").is_file());

    let output = swiftdoc(&dir, &["--intersphinx", spec, "--strict"]);
    assert!(!output.status.success(), "--strict should fail");
}
