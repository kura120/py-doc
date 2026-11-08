//! `--version-dir ROOT`: docs for several releases side by side. Each
//! build goes to `ROOT/<version>/`; `ROOT/versions.json` lists them for
//! the sidebar's version switcher, and `ROOT/index.html` redirects to the
//! newest. All links are relative, so the folder works on any static host.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::path::Path;

pub const FILE_NAME: &str = "versions.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub version: String,
    /// Folder of this version's site, relative to the root.
    pub path: String,
    pub latest: bool,
}

/// The folder name for a version: the version itself, which must be a
/// plain name.
pub fn folder_for(version: &str) -> Result<&str> {
    let version = version.trim();
    if version.is_empty()
        || version == "."
        || version == ".."
        || version.contains(['/', '\\', ':'])
        || version.chars().any(char::is_control)
    {
        bail!("--version-dir needs a --doc-version usable as a folder name, not {version:?}");
    }
    Ok(version)
}

/// Adds `version` to `root/versions.json` (or updates it) and points
/// `root/index.html` at the newest version.
pub fn record(root: &Path, version: &str) -> Result<Vec<Entry>> {
    let path = root.join(FILE_NAME);
    let mut entries: Vec<Entry> = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("{} is not a versions list", path.display()))?,
        Err(_) => Vec::new(),
    };
    let folder = folder_for(version)?;
    entries.retain(|e| e.version != version);
    entries.push(Entry {
        version: version.to_string(),
        path: folder.to_string(),
        latest: false,
    });
    // Versions that are no longer on disk are dropped.
    entries.retain(|e| root.join(&e.path).join("index.html").is_file());
    entries.sort_by(|a, b| compare_versions(&b.version, &a.version));
    let newest = entries
        .iter()
        .position(|e| !is_prerelease(&e.version))
        .unwrap_or(0);
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.latest = index == newest;
    }

    let json = serde_json::to_string_pretty(&entries)?;
    fs::write(&path, json + "\n").with_context(|| format!("Failed to write {}", path.display()))?;
    if let Some(latest) = entries.get(newest) {
        let target = format!("{}/index.html", latest.path);
        let target = crate::generator::escape_html(&target);
        fs::write(
            root.join("index.html"),
            format!(
                "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
                 <title>Redirecting</title>\n<meta http-equiv=\"refresh\" content=\"0; url={target}\">\n\
                 <link rel=\"canonical\" href=\"{target}\">\n</head>\n\
                 <body><p><a href=\"{target}\">Go to the latest documentation</a>.</p></body>\n</html>\n"
            ),
        )
        .context("Failed to write the version root's index.html")?;
    }
    Ok(entries)
}

/// "1.10.0" after "1.9.2": numeric parts compare as numbers.
fn compare_versions(a: &str, b: &str) -> Ordering {
    let key = |v: &str| -> Vec<(u64, String)> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                (
                    digits.parse().unwrap_or(0),
                    part[digits.len()..].to_string(),
                )
            })
            .collect()
    };
    let (ka, kb) = (key(a), key(b));
    for (pa, pb) in ka.iter().zip(&kb) {
        let ord =
            pa.0.cmp(&pb.0)
                .then_with(|| match (pa.1.is_empty(), pb.1.is_empty()) {
                    // "1.0" is newer than "1.0rc1".
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    _ => pa.1.cmp(&pb.1),
                });
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ka.len().cmp(&kb.len()).then_with(|| a.cmp(b))
}

fn is_prerelease(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    ["a", "b", "rc", "dev", "alpha", "beta", "pre"]
        .iter()
        .any(|tag| {
            lower
                .split(|c: char| !c.is_ascii_alphabetic())
                .any(|word| word == *tag)
        })
        || lower
            .chars()
            .zip(lower.chars().skip(1))
            .any(|(a, b)| a.is_ascii_digit() && b.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_sort_newest_first_and_skip_prereleases_for_latest() {
        let mut v = vec!["1.9.2", "1.10.0", "2.0.0rc1", "1.0", "0.9"];
        v.sort_by(|a, b| compare_versions(b, a));
        assert_eq!(v, ["2.0.0rc1", "1.10.0", "1.9.2", "1.0", "0.9"]);
        assert!(is_prerelease("2.0.0rc1") && is_prerelease("3.0-beta"));
        assert!(!is_prerelease("1.10.0") && !is_prerelease("v2"));
    }

    #[test]
    fn version_folders_are_plain_names() {
        assert_eq!(folder_for("2.1.0").unwrap(), "2.1.0");
        for bad in ["", "..", "a/b", "c:\\x"] {
            assert!(folder_for(bad).is_err(), "{bad}");
        }
    }
}
