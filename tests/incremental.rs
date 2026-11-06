//! Incremental builds: after every kind of edit, a build that reuses the
//! cache must produce exactly what a `--no-cache` build produces.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

const CACHE: &str = ".py-doc-cache";

struct Project {
    root: PathBuf,
    src: PathBuf,
    /// The incremental output folder, reused across builds.
    out: PathBuf,
    cold_builds: usize,
}

impl Project {
    fn new(test: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("incremental")
            .join(test);
        let _ = fs::remove_dir_all(&root);
        let src = root.join("src");
        fs::create_dir_all(src.join("pkg")).unwrap();
        let project = Project {
            out: root.join("site"),
            root,
            src,
            cold_builds: 0,
        };
        project.write(
            "pkg/__init__.py",
            "\"\"\"The demo package.\n\nMore about it.\n\"\"\"\n",
        );
        project.write(
            "pkg/engine.py",
            r#""""Engine module.

Runs things.
"""


class Engine:
    """An engine.

    It has a long description.
    """

    def start(self, speed: int = 1) -> None:
        """Starts the engine."""


def build():
    """Builds an engine."""
"#,
        );
        project.write(
            "pkg/tools.py",
            r#""""Tools module.

#pd-doc-link: pkg.engine.Engine
#pd-image: logo.png
"""


def helper(x):
    """Helps."""
"#,
        );
        project.write(
            "guide.py",
            "\"\"\"#pd-write\n# Guide\n\nRead this first.\n\"\"\"\n",
        );
        project.write("logo.png", "first image");
        project
    }

    fn write(&self, path: &str, body: &str) {
        let path = self.src.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn edit(&self, path: &str, from: &str, to: &str) {
        let full = self.src.join(path);
        let body = fs::read_to_string(&full).unwrap();
        assert!(body.contains(from), "{} does not contain {:?}", path, from);
        fs::write(full, body.replace(from, to)).unwrap();
    }

    fn run(&self, out: &Path, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_py-doc"))
            .arg("--src")
            .arg(&self.src)
            .arg("--out")
            .arg(out)
            .args(["--name", "demo", "--clean"])
            .args(extra)
            .output()
            .expect("failed to run py-doc")
    }

    /// An incremental build into `self.out`.
    fn build(&self, extra: &[&str]) -> Output {
        let output = self.run(&self.out, extra);
        assert!(
            output.status.success(),
            "py-doc failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// Builds incrementally, builds from scratch into a new folder, and
    /// checks that both folders hold the same files. Returns the
    /// incremental build's output.
    fn build_and_compare(&mut self, extra: &[&str]) -> Output {
        let incremental = self.build(extra);

        self.cold_builds += 1;
        let cold_out = self.root.join(format!("cold-{}", self.cold_builds));
        let mut cold_args = vec!["--no-cache"];
        cold_args.extend_from_slice(extra);
        let cold = self.run(&cold_out, &cold_args);
        assert!(cold.status.success());

        assert_same_site(&self.out, &cold_out);
        assert_eq!(
            warning_lines(&incremental),
            warning_lines(&cold),
            "incremental and cold builds warn differently"
        );
        incremental
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Warnings, sorted: modules render in parallel, so their order varies.
fn warning_lines(output: &Output) -> Vec<String> {
    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.contains('⚠'))
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

/// The number in "(N unchanged)" of the "Generated" line, 0 if absent.
fn unchanged_pages(output: &Output) -> usize {
    let text = stdout(output);
    let line = text
        .lines()
        .find(|line| line.contains("Generated"))
        .expect("no Generated line");
    line.split('(')
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|n| n.parse().unwrap())
        .unwrap_or(0)
}

fn total_pages(output: &Output) -> usize {
    let text = stdout(output);
    let line = text
        .lines()
        .find(|line| line.contains("Generated"))
        .unwrap();
    line.split("Generated ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap()
        .parse()
        .unwrap()
}

/// Every file below `dir` (except the cache) with its bytes.
fn site_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, files);
            } else {
                let relative = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != CACHE {
                    files.insert(relative, fs::read(&path).unwrap());
                }
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(dir, dir, &mut files);
    files
}

fn assert_same_site(incremental: &Path, cold: &Path) {
    let inc = site_files(incremental);
    let cold = site_files(cold);
    assert_eq!(
        inc.keys().collect::<Vec<_>>(),
        cold.keys().collect::<Vec<_>>(),
        "the two builds wrote different files"
    );
    for (path, bytes) in &inc {
        assert!(
            bytes == &cold[path],
            "{} differs between the incremental and the cold build",
            path
        );
    }
}

fn modified_times(dir: &Path) -> BTreeMap<String, SystemTime> {
    site_files(dir)
        .keys()
        .map(|path| {
            let time = fs::metadata(dir.join(path)).unwrap().modified().unwrap();
            (path.clone(), time)
        })
        .collect()
}

#[test]
fn incremental_builds_match_cold_builds_across_edits() {
    let mut project = Project::new("edits");
    let first = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&first), 0);
    assert!(project.out.join(CACHE).is_file());
    let pages = total_pages(&first);

    // No change: nothing is written.
    let before = modified_times(&project.out);
    let again = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&again), pages);
    assert_eq!(
        before,
        modified_times(&project.out),
        "a no-change run wrote files"
    );

    // A docstring body: only that module's page and the index.
    project.edit(
        "pkg/engine.py",
        "It has a long description.",
        "It has a longer description.",
    );
    let out = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&out), pages - 2);

    // A summary line shows in search and nav data: every page.
    project.edit("pkg/engine.py", "\"\"\"An engine.", "\"\"\"A motor.");
    project.build_and_compare(&[]);

    // A new module changes the nav: every page.
    project.write(
        "pkg/extra.py",
        "\"\"\"Extra module.\"\"\"\n\ndef more():\n    pass\n",
    );
    let out = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&out), 0);
    assert!(project.out.join("pkg/extra.html").is_file());

    // Deleting it again; --clean removes its page.
    fs::remove_file(project.src.join("pkg/extra.py")).unwrap();
    project.build_and_compare(&[]);
    assert!(!project.out.join("pkg/extra.html").exists());

    // Renaming a class breaks the doc-link in another module.
    project.edit("pkg/engine.py", "class Engine:", "class Motor:");
    project.build_and_compare(&[]);
    let tools = fs::read_to_string(project.out.join("pkg/tools.html")).unwrap();
    assert!(tools.contains("(not found)"));

    // ...and pointing the link at the new name fixes it.
    project.edit("pkg/tools.py", "pkg.engine.Engine", "pkg.engine.Motor");
    project.build_and_compare(&[]);

    // A changed image is copied again.
    project.write("logo.png", "second image");
    project.build_and_compare(&[]);
    assert_eq!(
        fs::read_to_string(project.out.join("assets/logo.png")).unwrap(),
        "second image"
    );

    // A missing image becomes a warning, and comes back.
    fs::remove_file(project.src.join("logo.png")).unwrap();
    project.build_and_compare(&[]);
    project.write("logo.png", "third image");
    project.build_and_compare(&[]);

    // Changing an option rebuilds every page.
    let out = project.build_and_compare(&["--layout", "modern"]);
    assert_eq!(unchanged_pages(&out), 0);
    let out = project.build_and_compare(&["--layout", "modern", "--doc-version", "9.0"]);
    assert_eq!(unchanged_pages(&out), 0);

    // Strict and clean are not part of the key.
    let out = project.build(&[
        "--layout",
        "modern",
        "--doc-version",
        "9.0",
        "--strict=false",
    ]);
    assert_eq!(unchanged_pages(&out), pages);
}

#[test]
fn a_page_changed_on_disk_is_rewritten() {
    let mut project = Project::new("tampered");
    project.build_and_compare(&[]);
    fs::write(project.out.join("pkg/engine.html"), "edited by hand").unwrap();
    fs::remove_file(project.out.join("pkg/tools.html")).unwrap();
    let out = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&out), total_pages(&out) - 2);
}

#[test]
fn a_broken_cache_means_a_full_build() {
    let mut project = Project::new("broken_cache");
    project.build_and_compare(&[]);
    fs::write(project.out.join(CACHE), "{ not json").unwrap();
    let out = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&out), 0);
}

#[test]
fn no_cache_rebuilds_everything_and_refreshes_the_cache() {
    let mut project = Project::new("no_cache");
    project.build_and_compare(&[]);
    let out = project.build(&["--no-cache"]);
    assert_eq!(unchanged_pages(&out), 0);
    let out = project.build(&[]);
    assert_eq!(unchanged_pages(&out), total_pages(&out));
}

#[test]
fn reused_modules_still_fail_strict() {
    let project = Project::new("strict");
    project.edit("pkg/tools.py", "pkg.engine.Engine", "pkg.engine.Missing");
    let first = project.run(&project.out, &["--strict"]);
    assert!(!first.status.success());
    let second = project.run(&project.out, &["--strict"]);
    assert!(!second.status.success(), "a cached warning was lost");
    assert_eq!(warning_lines(&first), warning_lines(&second));
}

#[test]
fn template_changes_rebuild_every_page() {
    let mut project = Project::new("templates");
    let templates = project.root.join("templates");
    fs::create_dir_all(&templates).unwrap();
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let module_html = fs::read_to_string(bundled.join("module.html")).unwrap();
    fs::write(templates.join("module.html"), &module_html).unwrap();
    let templates_arg = templates.to_string_lossy().into_owned();

    project.build_and_compare(&["--templates", &templates_arg]);
    let changed = module_html.replace(
        "{% block title %}Module ",
        "{% block title %}<!-- changed -->Module ",
    );
    assert_ne!(changed, module_html, "the template edit did not apply");
    fs::write(templates.join("module.html"), changed).unwrap();
    let out = project.build_and_compare(&["--templates", &templates_arg]);
    assert_eq!(unchanged_pages(&out), 0);
    let engine = fs::read_to_string(project.out.join("pkg/engine.html")).unwrap();
    assert!(engine.contains("<!-- changed -->"));

    // A static file is part of the key too.
    fs::write(templates.join("style.css"), "body { color: red; }").unwrap();
    let out = project.build_and_compare(&["--templates", &templates_arg]);
    assert_eq!(unchanged_pages(&out), 0);
}

#[test]
fn custom_templates_may_show_other_modules_docstrings() {
    let mut project = Project::new("custom_sidebar");
    let templates = project.root.join("templates");
    fs::create_dir_all(&templates).unwrap();
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let sidebar = fs::read_to_string(bundled.join("sidebar.html")).unwrap();
    // Every page shows every module's full docstring.
    fs::write(
        templates.join("sidebar.html"),
        format!(
            "{}{{% for m in package.modules %}}{{{{ m.docstring | safe }}}}{{% endfor %}}",
            sidebar
        ),
    )
    .unwrap();
    let templates_arg = templates.to_string_lossy().into_owned();

    project.build_and_compare(&["--templates", &templates_arg]);
    project.edit(
        "pkg/engine.py",
        "Runs things.",
        "Runs things, now differently.",
    );
    project.build_and_compare(&["--templates", &templates_arg]);
    let tools = fs::read_to_string(project.out.join("pkg/tools.html")).unwrap();
    assert!(tools.contains("now differently"));
}

/// The "(N unchanged)" count on the "Parsed" line, 0 if absent.
fn unchanged_files(output: &Output) -> usize {
    let text = stdout(output);
    let line = text.lines().find(|line| line.contains("Parsed")).unwrap();
    line.split('(')
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|n| n.parse().unwrap())
        .unwrap_or(0)
}

#[test]
fn private_names_stay_hidden_across_incremental_builds() {
    let mut project = Project::new("private");
    project.write(
        "pkg/_internal.py",
        "\"\"\"Internal.\"\"\"\n\ndef secret():\n    \"\"\"Secret.\"\"\"\n",
    );
    project.edit(
        "pkg/engine.py",
        "\n\nclass Engine:",
        "\n\n__all__ = [\"Engine\", \"build\"]\n\n\nclass Engine:",
    );
    project.edit(
        "pkg/tools.py",
        "#pd-image: logo.png",
        "#pd-image: logo.png\n#pd-doc-link: pkg._internal.secret",
    );
    project.edit(
        "pkg/tools.py",
        "def helper(x):",
        "def _private_helper():\n    pass\n\n\ndef helper(x):",
    );

    let first = project.build_and_compare(&[]);
    assert!(
        String::from_utf8_lossy(&first.stderr).contains("is private and hidden"),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(!project.out.join("pkg/_internal.html").exists());

    // Nothing changed: no file is parsed (the private one included), no
    // page is written, and the same warning is reported.
    let again = project.build_and_compare(&[]);
    assert_eq!(unchanged_files(&again), 5);
    assert_eq!(unchanged_pages(&again), total_pages(&again));

    // Renaming the private function turns "hidden" into "not found".
    project.edit("pkg/_internal.py", "def secret", "def hush");
    let out = project.build_and_compare(&[]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("was not found"));

    // Dropping a name from __all__ hides it.
    project.edit("pkg/engine.py", "[\"Engine\", \"build\"]", "[\"Engine\"]");
    project.build_and_compare(&[]);
    let engine = fs::read_to_string(project.out.join("pkg/engine.html")).unwrap();
    assert!(!engine.contains("fn.build"));

    // Showing private names is an option change: everything is rebuilt.
    let out = project.build_and_compare(&["--include-private"]);
    assert_eq!(unchanged_pages(&out), 0);
    assert!(project.out.join("pkg/_internal.html").is_file());
    let out = project.build_and_compare(&[]);
    assert_eq!(unchanged_pages(&out), 0);
    assert!(!project.out.join("pkg/_internal.html").exists());
}
