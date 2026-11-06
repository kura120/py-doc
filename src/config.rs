//! Settings from the command line, `[tool.py-doc]` and `[project]` in
//! pyproject.toml, merged into one [`Config`].
//!
//! Precedence, highest first: CLI flag > `[tool.py-doc]` > `[project]` >
//! built-in default.
//!
//! Adding an option:
//! 1. add an `Option<T>` field to `Args` (in main.rs). That gives it both
//!    its `--flag` and its `[tool.py-doc]` key, with the same name;
//! 2. add the resolved field to [`Config`] and one line to
//!    [`Config::resolve`], which is where its default lives;
//! 3. if it is a path, add it to [`rebase_paths`] so a config-file value is
//!    read relative to the pyproject.toml.

use anyhow::{Context, Result, bail};
use clap::CommandFactory;
use std::path::{Path, PathBuf};

use crate::{Args, CoverageFormat, LayoutKind, ThemeKind};

pub const DEFAULT_VERSION: &str = "1.0.0";

/// How to report documentation coverage.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageSettings {
    pub format: CoverageFormat,
    /// Fail when the total is below this percentage.
    pub fail_under: Option<f64>,
}

/// The fully resolved settings the rest of the program runs on.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub src: String,
    /// Where to write the site. None in check-only coverage runs: `--coverage`
    /// or `--fail-under` on the command line without `--out` there (an `out`
    /// in `[tool.py-doc]` does not count), so CI can check coverage without
    /// building the site.
    pub out: Option<String>,
    pub name: String,
    pub doc_version: String,
    pub templates: Option<String>,
    pub layout: LayoutKind,
    pub theme: ThemeKind,
    pub exclude: Vec<String>,
    pub source_url: Option<String>,
    pub strict: bool,
    pub clean: bool,
    pub include_private: bool,
    /// None when no coverage report was asked for.
    pub coverage: Option<CoverageSettings>,
}

/// Resolves the coverage options of two layers (CLI over `[tool.py-doc]`).
/// `fail-under` turns the report on within its own layer, so `--fail-under`
/// works alone, while an explicit `--coverage=false` still turns it off.
pub fn resolve_coverage(cli: &Args, tool: &Args) -> Result<Option<CoverageSettings>> {
    let wants = |layer: &Args| layer.coverage.or(layer.fail_under.map(|_| true));
    if !wants(cli).or(wants(tool)).unwrap_or(false) {
        return Ok(None);
    }
    let fail_under = cli.fail_under.or(tool.fail_under);
    if let Some(value) = fail_under
        && !(0.0..=100.0).contains(&value)
    {
        bail!("fail-under must be a percentage from 0 to 100, got {value}");
    }
    Ok(Some(CoverageSettings {
        format: cli
            .coverage_format
            .or(tool.coverage_format)
            .unwrap_or(CoverageFormat::Text),
        fail_under,
    }))
}

/// What was read from one pyproject.toml.
#[derive(Debug, Default)]
pub struct PyProject {
    pub path: PathBuf,
    /// `[tool.py-doc]`, with paths already made relative to the file's directory.
    pub tool: Option<Args>,
    /// Keys in `[tool.py-doc]` that are not options.
    pub unknown_keys: Vec<String>,
    pub project_name: Option<String>,
    pub project_version: Option<String>,
    /// `version` is listed in `[project] dynamic`.
    pub version_dynamic: bool,
}

impl PyProject {
    /// Reads `--config` if given, otherwise looks for pyproject.toml in
    /// `--src` (or the current directory) and its parents, stopping at the
    /// repository root. `Ok(None)` when there is none.
    pub fn load(cli: &Args) -> Result<Option<PyProject>> {
        if let Some(path) = &cli.config {
            return Self::read(Path::new(path)).map(Some);
        }
        let start = match &cli.src {
            Some(src) => PathBuf::from(src),
            None => std::env::current_dir()?,
        };
        match find_pyproject(&start) {
            Some(path) => Self::read(&path).map(Some),
            None => Ok(None),
        }
    }

    pub fn read(path: &Path) -> Result<PyProject> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let base = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Self::parse(&text, path, base)
    }

    /// Parses pyproject.toml text. `base` is the directory config paths are
    /// relative to.
    pub fn parse(text: &str, path: &Path, base: &Path) -> Result<PyProject> {
        let doc: toml::Table =
            toml::from_str(text).with_context(|| format!("cannot parse {}", path.display()))?;

        let mut result = PyProject {
            path: path.to_path_buf(),
            ..Default::default()
        };

        if let Some(project) = doc.get("project").and_then(|p| p.as_table()) {
            result.project_name = project
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            result.project_version = project
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            result.version_dynamic = project
                .get("dynamic")
                .and_then(|v| v.as_array())
                .is_some_and(|items| items.iter().any(|i| i.as_str() == Some("version")));
        }

        let tool = doc
            .get("tool")
            .and_then(|t| t.as_table())
            .and_then(|t| t.get("py-doc"));
        if let Some(tool) = tool {
            let table = tool
                .as_table()
                .with_context(|| format!("[tool.py-doc] in {} is not a table", path.display()))?;
            let known = known_keys();
            result.unknown_keys = table
                .keys()
                .filter(|k| !known.contains(k))
                .cloned()
                .collect();
            let mut args: Args = table
                .clone()
                .try_into()
                .with_context(|| format!("invalid [tool.py-doc] in {}", path.display()))?;
            rebase_paths(&mut args, base);
            result.tool = Some(args);
        }

        Ok(result)
    }

    /// Whether `[tool.py-doc]` is present, i.e. the project opted in to
    /// configuration and `py-doc` alone should build it.
    pub fn is_configured(&self) -> bool {
        self.tool.is_some()
    }
}

/// The `[tool.py-doc]` keys: every long flag except `--config`.
fn known_keys() -> Vec<String> {
    Args::command()
        .get_arguments()
        .filter_map(|arg| arg.get_long())
        .filter(|long| !matches!(*long, "config" | "help" | "version"))
        .map(str::to_string)
        .collect()
}

/// Makes the path options of a config-file layer relative to `base`.
fn rebase_paths(args: &mut Args, base: &Path) {
    for path in [&mut args.src, &mut args.out, &mut args.templates]
        .into_iter()
        .flatten()
    {
        *path = base.join(&*path).to_string_lossy().into_owned();
    }
}

/// Looks for pyproject.toml in `start` (or its directory, for a file) and
/// its parents. Stops after the first directory holding `.git`.
fn find_pyproject(start: &Path) -> Option<PathBuf> {
    // dunce avoids Windows verbatim paths (`\\?\C:\...`), which would
    // otherwise flow into --src and --out and show up in messages.
    let start = dunce::canonicalize(start).ok()?;
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    loop {
        let candidate = dir.join("pyproject.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir.join(".git").exists() {
            return None;
        }
        dir = dir.parent()?.to_path_buf();
    }
}

/// Settings from the config file alone (no CLI), used to prefill the wizard.
pub fn file_layer(file: Option<&PyProject>) -> Args {
    let mut layer = file.and_then(|f| f.tool.clone()).unwrap_or_default();
    if let Some(file) = file {
        layer.name = layer.name.or_else(|| file.project_name.clone());
        layer.doc_version = layer.doc_version.or_else(|| file.project_version.clone());
    }
    layer
}

fn missing(key: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "missing --{key}: pass it on the command line or set `{key}` in [tool.py-doc] of pyproject.toml"
    )
}

impl Config {
    /// Merges the CLI over the config file and fills in defaults.
    /// Prints a warning for unknown `[tool.py-doc]` keys (an error when
    /// strict) and a hint when the version falls back because it is dynamic.
    pub fn resolve(cli: Args, file: Option<&PyProject>) -> Result<Config> {
        let tool = file.and_then(|f| f.tool.clone()).unwrap_or_default();

        let strict = cli.strict.or(tool.strict).unwrap_or(false);

        let coverage = resolve_coverage(&cli, &tool)?;
        // Check-only: coverage asked for on the command line, with no --out
        // there. A config-file `out` is for normal builds, so it is ignored.
        let cli_wants_coverage = cli.coverage.or(cli.fail_under.map(|_| true)) == Some(true);
        let check_only = cli_wants_coverage && cli.out.is_none();

        if let Some(file) = file
            && !file.unknown_keys.is_empty()
        {
            let keys = file
                .unknown_keys
                .iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!(
                "unknown key(s) {keys} in [tool.py-doc] of {}",
                file.path.display()
            );
            if strict {
                bail!("{message}");
            }
            eprintln!("  \x1b[33m⚠\x1b[0m warning: {message}");
        }

        let project_name = file.and_then(|f| f.project_name.clone());
        let project_version = file.and_then(|f| f.project_version.clone());
        let version_dynamic = file.is_some_and(|f| f.version_dynamic);

        let doc_version = match cli.doc_version.or(tool.doc_version).or(project_version) {
            Some(version) => version,
            None => {
                if version_dynamic {
                    eprintln!(
                        "  hint: the version in pyproject.toml is dynamic, so the docs show {DEFAULT_VERSION}. \
                         Pass --doc-version or set `doc-version` in [tool.py-doc]."
                    );
                }
                DEFAULT_VERSION.to_string()
            }
        };

        let src = cli.src.or(tool.src).ok_or_else(|| missing("src"))?;
        let out = if check_only {
            None
        } else {
            Some(cli.out.or(tool.out).ok_or_else(|| missing("out"))?)
        };
        let name = match cli.name.or(tool.name).or(project_name) {
            Some(name) => name,
            // No site, so the name is never shown.
            None if check_only => String::new(),
            None => return Err(missing("name")),
        };

        Ok(Config {
            src,
            out,
            name,
            doc_version,
            templates: cli.templates.or(tool.templates),
            layout: cli.layout.or(tool.layout).unwrap_or(LayoutKind::Classic),
            theme: cli.theme.or(tool.theme).unwrap_or(ThemeKind::Auto),
            exclude: if cli.exclude.is_empty() {
                tool.exclude
            } else {
                cli.exclude
            },
            source_url: cli.source_url.or(tool.source_url),
            strict,
            clean: cli.clean.or(tool.clean).unwrap_or(false),
            include_private: cli
                .include_private
                .or(tool.include_private)
                .unwrap_or(false),
            coverage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(args: &[&str]) -> Args {
        Args::try_parse_from(std::iter::once("py-doc").chain(args.iter().copied())).unwrap()
    }

    fn in_proj(path: &str) -> String {
        Path::new("/proj").join(path).to_string_lossy().into_owned()
    }

    fn pyproject(text: &str) -> PyProject {
        PyProject::parse(text, Path::new("/proj/pyproject.toml"), Path::new("/proj")).unwrap()
    }

    const FULL_TOOL: &str = r#"
[project]
name = "proj-name"
version = "2.0.0"

[tool.py-doc]
src = "tool-src"
out = "tool-out"
name = "tool-name"
doc-version = "3.0.0"
templates = "tool-templates"
layout = "modern"
theme = "slate"
exclude = ["tool-ex"]
source-url = "https://tool"
strict = true
clean = true
include-private = true
coverage = true
fail-under = 50
coverage-format = "json"
"#;

    const ALL_CLI: &[&str] = &[
        "--src",
        "cli-src",
        "--out",
        "cli-out",
        "--name",
        "cli-name",
        "--doc-version",
        "4.0.0",
        "--templates",
        "cli-templates",
        "--layout",
        "minimal",
        "--theme",
        "dark",
        "--exclude",
        "cli-ex",
        "--source-url",
        "https://cli",
        "--strict=false",
        "--clean=false",
        "--include-private=false",
        "--fail-under",
        "75.5",
        "--coverage-format",
        "text",
    ];

    #[test]
    fn cli_beats_tool_for_every_key() {
        let file = pyproject(FULL_TOOL);
        let config = Config::resolve(cli(ALL_CLI), Some(&file)).unwrap();
        assert_eq!(
            config,
            Config {
                src: "cli-src".into(),
                out: Some("cli-out".into()),
                name: "cli-name".into(),
                doc_version: "4.0.0".into(),
                templates: Some("cli-templates".into()),
                layout: LayoutKind::Minimal,
                theme: ThemeKind::Dark,
                exclude: vec!["cli-ex".into()],
                source_url: Some("https://cli".into()),
                strict: false,
                clean: false,
                include_private: false,
                coverage: Some(CoverageSettings {
                    format: CoverageFormat::Text,
                    fail_under: Some(75.5),
                }),
            }
        );
    }

    #[test]
    fn tool_beats_project_and_defaults_for_every_key() {
        let file = pyproject(FULL_TOOL);
        let config = Config::resolve(cli(&[]), Some(&file)).unwrap();
        assert_eq!(
            config,
            Config {
                src: in_proj("tool-src"),
                out: Some(in_proj("tool-out")),
                name: "tool-name".into(),
                doc_version: "3.0.0".into(),
                templates: Some(in_proj("tool-templates")),
                layout: LayoutKind::Modern,
                theme: ThemeKind::Slate,
                exclude: vec!["tool-ex".into()],
                source_url: Some("https://tool".into()),
                strict: true,
                clean: true,
                include_private: true,
                coverage: Some(CoverageSettings {
                    format: CoverageFormat::Json,
                    fail_under: Some(50.0),
                }),
            }
        );
    }

    #[test]
    fn project_fills_name_and_version() {
        let file = pyproject("[project]\nname = \"proj-name\"\nversion = \"2.0.0\"\n");
        let config = Config::resolve(cli(&["-s", "a", "-o", "b"]), Some(&file)).unwrap();
        assert_eq!(config.name, "proj-name");
        assert_eq!(config.doc_version, "2.0.0");
    }

    #[test]
    fn defaults_apply_without_config() {
        let config = Config::resolve(cli(&["-s", "a", "-o", "b", "-n", "c"]), None).unwrap();
        assert_eq!(
            config,
            Config {
                src: "a".into(),
                out: Some("b".into()),
                name: "c".into(),
                doc_version: "1.0.0".into(),
                templates: None,
                layout: LayoutKind::Classic,
                theme: ThemeKind::Auto,
                exclude: vec![],
                source_url: None,
                strict: false,
                clean: false,
                include_private: false,
                coverage: None,
            }
        );
    }

    #[test]
    fn bare_bool_flags_mean_true() {
        let config = Config::resolve(
            cli(&["-s", "a", "-o", "b", "-n", "c", "--strict", "--clean"]),
            None,
        )
        .unwrap();
        assert!(config.strict && config.clean);
    }

    #[test]
    fn dynamic_version_falls_back() {
        let file = pyproject("[project]\nname = \"p\"\ndynamic = [\"version\"]\n");
        assert!(file.version_dynamic);
        let config = Config::resolve(cli(&["-s", "a", "-o", "b"]), Some(&file)).unwrap();
        assert_eq!(config.doc_version, DEFAULT_VERSION);
    }

    #[test]
    fn missing_required_keys_are_named() {
        let err = Config::resolve(cli(&["-o", "b", "-n", "c"]), None).unwrap_err();
        assert!(err.to_string().contains("--src"), "{err}");
        let err = Config::resolve(cli(&["-s", "a", "-n", "c"]), None).unwrap_err();
        assert!(err.to_string().contains("--out"), "{err}");
        let err = Config::resolve(cli(&["-s", "a", "-o", "b"]), None).unwrap_err();
        assert!(err.to_string().contains("--name"), "{err}");
    }

    #[test]
    fn unknown_keys_warn_or_fail_under_strict() {
        let file = pyproject("[tool.py-doc]\nsrc = \"a\"\nout = \"b\"\nname = \"c\"\nbogus = 1\n");
        assert_eq!(file.unknown_keys, ["bogus"]);
        assert!(Config::resolve(cli(&[]), Some(&file)).is_ok());
        let err = Config::resolve(cli(&["--strict"]), Some(&file)).unwrap_err();
        assert!(err.to_string().contains("`bogus`"), "{err}");

        let file = pyproject(
            "[tool.py-doc]\nsrc = \"a\"\nout = \"b\"\nname = \"c\"\nstrict = true\nbogus = 1\n",
        );
        assert!(Config::resolve(cli(&[]), Some(&file)).is_err());
    }

    #[test]
    fn every_flag_is_a_known_key() {
        let keys = known_keys();
        for key in [
            "src",
            "out",
            "name",
            "doc-version",
            "templates",
            "layout",
            "theme",
            "exclude",
            "source-url",
            "strict",
            "clean",
            "include-private",
            "coverage",
            "fail-under",
            "coverage-format",
        ] {
            assert!(keys.iter().any(|k| k == key), "{key}");
        }
        assert!(!keys.iter().any(|k| k == "config"));
    }

    #[test]
    fn bad_values_are_errors() {
        let text = "[tool.py-doc]\nlayout = \"sideways\"\n";
        assert!(PyProject::parse(text, Path::new("p"), Path::new(".")).is_err());
        let text = "[tool.py-doc]\nexclude = \"not-a-list\"\n";
        assert!(PyProject::parse(text, Path::new("p"), Path::new(".")).is_err());
        let text = "[tool.py-doc]\ncoverage-format = \"xml\"\n";
        assert!(PyProject::parse(text, Path::new("p"), Path::new(".")).is_err());
    }

    #[test]
    fn coverage_on_the_command_line_without_out_skips_the_site() {
        // A config-file `out` is for normal builds: a command-line coverage
        // check without --out writes no site, and needs no name.
        let file = pyproject("[tool.py-doc]\nsrc = \"a\"\nout = \"site\"\n");
        let config = Config::resolve(cli(&["--coverage"]), Some(&file)).unwrap();
        assert_eq!(config.out, None);
        assert_eq!(
            config.coverage,
            Some(CoverageSettings {
                format: CoverageFormat::Text,
                fail_under: None,
            })
        );

        let config = Config::resolve(cli(&["--fail-under", "80"]), Some(&file)).unwrap();
        assert_eq!(config.out, None);
        assert_eq!(config.coverage.unwrap().fail_under, Some(80.0));

        // --out on the command line builds the site as well.
        let config = Config::resolve(
            cli(&["--coverage", "-o", "cli-out", "-n", "x"]),
            Some(&file),
        )
        .unwrap();
        assert_eq!(config.out.as_deref(), Some("cli-out"));
        assert!(config.coverage.is_some());
    }

    #[test]
    fn coverage_in_the_config_file_adds_to_a_normal_build() {
        let file = pyproject(
            "[tool.py-doc]\nsrc = \"a\"\nout = \"site\"\nname = \"n\"\nfail-under = 80\n",
        );
        let config = Config::resolve(cli(&[]), Some(&file)).unwrap();
        assert_eq!(config.out, Some(in_proj("site")));
        assert_eq!(config.coverage.unwrap().fail_under, Some(80.0));

        // An explicit --coverage=false beats the file's fail-under.
        let config = Config::resolve(cli(&["--coverage=false"]), Some(&file)).unwrap();
        assert_eq!(config.coverage, None);
        assert_eq!(config.out, Some(in_proj("site")));
    }

    #[test]
    fn fail_under_must_be_a_percentage() {
        for bad in ["-1", "100.5", "NaN"] {
            let flag = format!("--fail-under={bad}");
            let err = Config::resolve(cli(&["-s", "a", &flag]), None).unwrap_err();
            assert!(err.to_string().contains("0 to 100"), "{bad}: {err}");
        }
        for ok in ["0", "100", "87.5"] {
            assert!(Config::resolve(cli(&["-s", "a", "--fail-under", ok]), None).is_ok());
        }
    }
}
