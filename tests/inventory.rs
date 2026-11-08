//! `objects.inv`: a Sphinx inventory (version 2) for intersphinx.

use flate2::read::ZlibDecoder;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

#[test]
fn html_builds_write_a_sphinx_inventory_of_every_symbol() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inventory");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("empty.toml"), "").unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let output = Command::new(env!("CARGO_BIN_EXE_swiftdoc"))
        .current_dir(&dir)
        .args(["--config", "empty.toml", "--out", "site", "--name", "demo"])
        .args(["--doc-version", "2.1.0", "--src"])
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bytes = fs::read(dir.join("site/objects.inv")).unwrap();
    let header = "# Sphinx inventory version 2\n\
                  # Project: demo\n\
                  # Version: 2.1.0\n\
                  # The remainder of this file is compressed using zlib.\n";
    assert!(bytes.starts_with(header.as_bytes()));
    let mut body = String::new();
    ZlibDecoder::new(&bytes[header.len()..])
        .read_to_string(&mut body)
        .unwrap();

    // Names are import paths (the source root is the `sample` package),
    // ordering prefixes stripped; URIs are the anchors on our pages.
    for line in [
        "sample py:module 1 __init__.html -",
        "sample.core.engine py:module 1 01_core/engine.html -",
        "sample.core.engine.Engine py:class 1 01_core/engine.html#class.Engine -",
        "sample.core.engine.Engine.arun py:method 1 01_core/engine.html#method.Engine.arun -",
        "sample.core.engine.Engine.speed py:attribute 1 01_core/engine.html#attr.Engine.speed -",
        "sample.utils.helpers.slugify py:function 1 utils/helpers.html#fn.slugify -",
    ] {
        assert!(
            body.lines().any(|l| l == line),
            "missing `{line}` in:\n{body}"
        );
    }
    // Doc-only pages are not API symbols.
    assert!(!body.contains("guide"), "{body}");
    let manifest = fs::read_to_string(dir.join("site/.swiftdoc-manifest")).unwrap();
    assert!(manifest.contains("objects.inv"));
}
