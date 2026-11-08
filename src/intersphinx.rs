//! `--intersphinx NAME=URL`: links to other projects' documentation
//! through their Sphinx `objects.inv` inventories.
//!
//! Inventories are downloaded once and cached in the user's cache folder,
//! so later builds work offline. A cached copy is refreshed after a week;
//! when that fails the old copy is used.

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::ZlibDecoder;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Python names from every loaded inventory -> absolute URL.
#[derive(Debug, Default)]
pub struct Inventories {
    urls: HashMap<String, String>,
}

impl Inventories {
    /// Loads every `NAME=URL` spec. Returns the inventories that could be
    /// loaded and a warning for each one that could not.
    pub fn load(specs: &[String]) -> (Self, Vec<String>) {
        let mut inventories = Self::default();
        let mut warnings = Vec::new();
        for spec in specs {
            if let Err(e) = inventories.load_one(spec) {
                warnings.push(format!("--intersphinx {spec}: {e:#}"));
            }
        }
        (inventories, warnings)
    }

    fn load_one(&mut self, spec: &str) -> Result<()> {
        let (name, url) = spec
            .split_once('=')
            .filter(|(name, url)| !name.trim().is_empty() && !url.trim().is_empty())
            .ok_or_else(|| anyhow!("expected NAME=URL"))?;
        let base = url.trim().trim_end_matches('/');
        let bytes = fetch_cached(name.trim(), base)?;
        let before = self.urls.len();
        parse(&bytes, base, &mut self.urls)?;
        if self.urls.len() == before {
            bail!("the inventory lists no Python objects");
        }
        Ok(())
    }

    /// The documentation URL for a dotted Python name, if any inventory
    /// lists it.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.urls.get(name).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.urls.is_empty()
    }
}

/// Where inventories are cached: `SWIFTDOC_CACHE_DIR`, else the platform's
/// user cache folder.
fn cache_dir() -> Option<PathBuf> {
    let env = |key: &str| {
        std::env::var_os(key)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = env("SWIFTDOC_CACHE_DIR") {
        return Some(dir.join("intersphinx"));
    }
    let base = if cfg!(windows) {
        env("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|home| home.join("Library/Caches"))
    } else {
        env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|home| home.join(".cache")))
    };
    base.map(|dir| dir.join("swiftdoc").join("intersphinx"))
}

/// The cache file name for an inventory URL.
pub fn cache_file_name(base: &str) -> String {
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}.inv")
}

fn fetch_cached(name: &str, base: &str) -> Result<Vec<u8>> {
    let cached = cache_dir().map(|dir| dir.join(cache_file_name(base)));
    let fresh = cached.as_ref().is_some_and(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < MAX_AGE)
    });
    if fresh && let Some(bytes) = cached.as_ref().and_then(|path| std::fs::read(path).ok()) {
        return Ok(bytes);
    }
    match download(&format!("{base}/objects.inv")) {
        Ok(bytes) => {
            if let Some(path) = &cached
                && let Some(parent) = path.parent()
            {
                // A cache that cannot be written only costs a download next time.
                let _ = std::fs::create_dir_all(parent).and_then(|()| std::fs::write(path, &bytes));
            }
            Ok(bytes)
        }
        Err(e) => match cached.as_ref().and_then(|path| std::fs::read(path).ok()) {
            Some(bytes) => {
                eprintln!(
                    "  \x1b[33m⚠\x1b[0m Could not refresh the {name} inventory ({e:#}); using the cached copy."
                );
                Ok(bytes)
            }
            None => Err(e),
        },
    }
}

fn download(url: &str) -> Result<Vec<u8>> {
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(true)
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .get(url)
        .call()
        .with_context(|| format!("could not download {url}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .with_context(|| format!("could not download {url}"))?;
    Ok(bytes)
}

/// Adds the Python objects of a version 2 inventory to `urls`. Names
/// already present keep their first URL.
fn parse(bytes: &[u8], base: &str, urls: &mut HashMap<String, String>) -> Result<()> {
    let mut rest = bytes;
    for expected in [
        "# Sphinx inventory version 2",
        "# Project:",
        "# Version:",
        "#",
    ] {
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| anyhow!("not a Sphinx inventory"))?;
        let line = String::from_utf8_lossy(&rest[..end]);
        if !line.starts_with(expected) {
            bail!("not a Sphinx inventory (version 2)");
        }
        rest = &rest[end + 1..];
    }
    let mut text = String::new();
    ZlibDecoder::new(rest)
        .read_to_string(&mut text)
        .context("the inventory is corrupt")?;

    for line in text.lines() {
        // name domain:role priority uri dispname; only the name can
        // contain spaces, and Python names never do.
        let mut fields = line.split_whitespace();
        let (Some(name), Some(role), Some(_priority), Some(uri)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if !role.starts_with("py:") {
            continue;
        }
        let uri = match uri.strip_suffix('$') {
            Some(start) => format!("{start}{name}"),
            None => uri.to_string(),
        };
        urls.entry(name.to_string())
            .or_insert_with(|| format!("{base}/{uri}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    pub(crate) fn inventory(lines: &str) -> Vec<u8> {
        let mut out = b"# Sphinx inventory version 2\n# Project: Python\n# Version: 3.13\n# The remainder of this file is compressed using zlib.\n".to_vec();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(lines.as_bytes()).unwrap();
        out.extend(encoder.finish().unwrap());
        out
    }

    #[test]
    fn python_objects_are_read_with_dollar_uris_expanded() {
        let bytes = inventory(
            "pathlib.Path py:class 1 library/pathlib.html#$ -\n\
             os.getcwd py:function 1 library/os.html#os.getcwd -\n\
             a label std:label -1 glossary.html#term-a A label\n",
        );
        let mut urls = HashMap::new();
        parse(&bytes, "https://docs.python.org/3", &mut urls).unwrap();
        assert_eq!(
            urls.get("pathlib.Path").map(String::as_str),
            Some("https://docs.python.org/3/library/pathlib.html#pathlib.Path")
        );
        assert!(urls.contains_key("os.getcwd"));
        assert_eq!(urls.len(), 2, "only the py domain is kept");
        assert!(parse(b"not an inventory\n", "x", &mut urls).is_err());
    }
}
