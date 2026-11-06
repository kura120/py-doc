//! The incremental-build cache, stored as `.py-doc-cache` in `--out`.
//!
//! It holds, per source file, the content hash and the rendered module plus
//! everything its rendering depended on (doc-link answers, image files,
//! warnings), and per page a hash of the page's inputs and of the bytes
//! written. A run reuses a module only when all of those still match, and
//! skips a page only when its inputs match and the file on disk is the one
//! this cache wrote. Anything else is rebuilt: a stale page is worse than a
//! slow build.
//!
//! The whole cache is dropped when its key changes. The key covers the
//! py-doc version, every option that can change the output and the theme
//! files, so an option or template change is a full rebuild.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::PythonModule;

pub const CACHE_NAME: &str = ".py-doc-cache";

/// Bump when the cache layout or what it records changes.
const FORMAT: u32 = 2;

/// A 128-bit content hash as hex.
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:032x}", xxhash_rust::xxh3::xxh3_128(bytes))
}

/// Hashes a sequence of parts with unambiguous boundaries.
pub fn hash_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut buffer = Vec::new();
    for part in parts {
        buffer.extend_from_slice(&(part.len() as u64).to_le_bytes());
        buffer.extend_from_slice(part.as_bytes());
    }
    hash_bytes(&buffer)
}

/// Hash of one source file: its path relative to the source root and its bytes.
pub fn hash_source(source_path: &str, contents: &[u8]) -> String {
    let mut buffer = Vec::with_capacity(source_path.len() + contents.len() + 1);
    buffer.extend_from_slice(source_path.as_bytes());
    buffer.push(0);
    buffer.extend_from_slice(contents);
    hash_bytes(&buffer)
}

/// The key for a cache built by this py-doc with these settings.
/// `settings` is every option that affects the output, serialized;
/// `theme` is the fingerprint of the template and static files.
pub fn cache_key(settings: &str, theme: &str) -> String {
    hash_parts([
        env!("CARGO_PKG_VERSION"),
        &FORMAT.to_string(),
        settings,
        theme,
    ])
}

/// `path` relative to `root` with forward slashes, the way the parser
/// spells `source_path`. A mismatch only costs a cache miss.
pub fn source_path_for(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// What rendering one module looked at, besides its own source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModuleDeps {
    /// Each `#pd-doc-link` target, what it resolved to and, when it did
    /// not resolve, whether it names a hidden private symbol.
    pub links: Vec<(String, Option<String>, bool)>,
    /// Each local image path and its status (see `SiteGenerator::image_status`).
    pub images: Vec<(String, String)>,
    /// Warnings printed while rendering, replayed when the module is reused.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleEntry {
    pub source_path: String,
    pub source_hash: String,
    /// Not serialized as part of `PythonModule`, so stored here.
    pub dir: String,
    /// The public module after rendering; None for a private module.
    pub module: Option<PythonModule>,
    /// The file's private part (see `PythonPackage::hide_private`), without
    /// docstrings: the resolver only needs its names.
    pub hidden: Option<PythonModule>,
    /// The fields below describe `module`; empty without one.
    pub deps: ModuleDeps,
    /// Hash of the rendered module.
    pub full_hash: String,
    /// Hash of the rendered module without its rendered docstrings: the part
    /// that other pages (sidebar, nav, search) can show.
    pub shared_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageEntry {
    /// Hash of everything the page was rendered from.
    pub inputs: String,
    /// Hash of the bytes written.
    pub output: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    pub key: String,
    /// By `source_path`.
    pub modules: BTreeMap<String, ModuleEntry>,
    /// By site path.
    pub pages: BTreeMap<String, PageEntry>,
}

impl Cache {
    pub fn path(out_dir: &Path) -> PathBuf {
        out_dir.join(CACHE_NAME)
    }

    /// The cache in `out_dir` if it exists, is readable and has this key.
    /// Any problem just means a full build.
    pub fn load(out_dir: &Path, key: &str) -> Option<Cache> {
        let bytes = fs::read(Self::path(out_dir)).ok()?;
        let mut cache: Cache = serde_json::from_slice(&bytes).ok()?;
        if cache.key != key {
            return None;
        }
        for entry in cache.modules.values_mut() {
            if let Some(module) = entry.module.as_mut() {
                module.dir = entry.dir.clone();
            }
        }
        Some(cache)
    }

    /// Whether `source_path` is cached with this content hash.
    pub fn has_source(&self, source_path: &str, source_hash: &str) -> bool {
        self.modules
            .get(source_path)
            .is_some_and(|entry| entry.source_hash == source_hash)
    }

    /// Writes this cache (used by tests; a run writes with [`write`]).
    #[cfg(test)]
    pub fn save(&self, out_dir: &Path) -> Result<()> {
        let entries = self
            .modules
            .values()
            .map(|entry| {
                let module_json = entry
                    .module
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;
                entry_json(&EntryParts {
                    source_path: &entry.source_path,
                    source_hash: &entry.source_hash,
                    dir: &entry.dir,
                    module_json: module_json.as_deref(),
                    hidden: entry.hidden.as_ref(),
                    deps: &entry.deps,
                    full_hash: &entry.full_hash,
                    shared_hash: &entry.shared_hash,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        write(out_dir, &self.key, &modules_json(&entries), &self.pages)
    }
}

/// The fields of one [`ModuleEntry`], with the module already serialized:
/// a run has its JSON from hashing it, so it is not serialized twice.
pub struct EntryParts<'a> {
    pub source_path: &'a str,
    pub source_hash: &'a str,
    pub dir: &'a str,
    pub module_json: Option<&'a str>,
    pub hidden: Option<&'a PythonModule>,
    pub deps: &'a ModuleDeps,
    pub full_hash: &'a str,
    pub shared_hash: &'a str,
}

/// One `"source_path": {entry}` member of the `modules` map, as JSON.
pub fn entry_json(parts: &EntryParts) -> Result<String> {
    let text = |value: &str| serde_json::to_string(value);
    Ok(format!(
        "{}:{{\"source_path\":{},\"source_hash\":{},\"dir\":{},\"module\":{},\"hidden\":{},\"deps\":{},\"full_hash\":{},\"shared_hash\":{}}}",
        text(parts.source_path)?,
        text(parts.source_path)?,
        text(parts.source_hash)?,
        text(parts.dir)?,
        parts.module_json.unwrap_or("null"),
        serde_json::to_string(&parts.hidden)?,
        serde_json::to_string(parts.deps)?,
        text(parts.full_hash)?,
        text(parts.shared_hash)?,
    ))
}

/// The `modules` map of a cache file from its members.
pub fn modules_json(entries: &[String]) -> String {
    format!("{{{}}}", entries.join(","))
}

/// Writes the cache file next to the site, replacing the old one in one step.
pub fn write(
    out_dir: &Path,
    key: &str,
    modules_json: &str,
    pages: &BTreeMap<String, PageEntry>,
) -> Result<()> {
    let body = format!(
        "{{\"key\":{},\"modules\":{},\"pages\":{}}}",
        serde_json::to_string(key)?,
        modules_json,
        serde_json::to_string(pages)?
    );
    let path = Cache::path(out_dir);
    let temporary = out_dir.join(format!("{}.tmp", CACHE_NAME));
    fs::write(&temporary, body).with_context(|| format!("Failed to write {:?}", temporary))?;
    fs::rename(&temporary, &path).with_context(|| format!("Failed to write {:?}", path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_parts_keeps_boundaries() {
        assert_ne!(hash_parts(["ab", "c"]), hash_parts(["a", "bc"]));
        assert_eq!(hash_parts(["ab", "c"]), hash_parts(["ab", "c"]));
    }

    #[test]
    fn source_hash_covers_the_path() {
        assert_ne!(hash_source("a.py", b"x"), hash_source("b.py", b"x"));
    }

    #[test]
    fn source_path_uses_forward_slashes() {
        let root = Path::new("root");
        assert_eq!(
            source_path_for(&root.join("a").join("b.py"), root),
            "a/b.py"
        );
    }

    #[test]
    fn a_cache_with_another_key_is_ignored() {
        let dir = std::env::temp_dir().join(format!("py-doc-cache-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cache = Cache {
            key: "one".into(),
            ..Default::default()
        };
        cache.save(&dir).unwrap();
        assert!(Cache::load(&dir, "one").is_some());
        assert!(Cache::load(&dir, "two").is_none());
        fs::write(Cache::path(&dir), "not json").unwrap();
        assert!(Cache::load(&dir, "one").is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
