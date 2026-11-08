//! Standalone Markdown pages (`--pages <DIR>`): guides and tutorials shown
//! next to the API reference.
//!
//! Each `.md` file becomes a doc-only module, so it goes through the same
//! rendering as a `#sd-write` module: macros, doc-links, search, the
//! manifest and `--clean` all work unchanged. A top-level `index.md` is not
//! a page: it replaces the landing page's introduction.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::cache;
use crate::generator::ModuleSource;
use crate::models::PythonModule;
use crate::parser::split_order_prefix;

/// Grouping key of the pages' nav group. Starts with a character no
/// directory name can, so it never merges with a source folder.
pub const PAGES_DIR: &str = "\u{1}pages";

/// A top-level `index.md`: (pages folder name, Markdown).
pub type Intro = (String, String);

/// The nav group's title.
pub const GROUP_NAME: &str = "Guides";

/// The pages under `dir`, plus the top-level `index.md` as (folder name,
/// Markdown), if there is one.
///
/// Each page's source path is `<dir name>/<path in dir>`, so its page is
/// written to `<dir name>/<path>.html` and relative links between pages
/// keep working once `.md` becomes `.html`. Paths are placed under `root`
/// (the `--src` root) because that is where source paths are taken from.
pub fn collect(dir: &Path, root: &Path) -> Result<(Vec<ModuleSource>, Option<Intro>)> {
    let mut files = Vec::new();
    find_markdown(dir, &mut files)
        .with_context(|| format!("Failed to read --pages folder {}", dir.display()))?;
    files.sort();

    let base = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pages".to_string());

    let mut sources = Vec::new();
    let mut intro = None;
    for path in files {
        let bytes =
            std::fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let rel = cache::source_path_for(&path, dir);
        let (front, body) = split_front_matter(&text);
        if rel == "index.md" {
            intro = Some((base.clone(), body.to_string()));
            continue;
        }

        let source_path = format!("{base}/{rel}");
        let stem = rel
            .rsplit('/')
            .next()
            .unwrap_or(&rel)
            .trim_end_matches(".md");
        let (prefix_order, plain_stem) = split_order_prefix(stem);
        let title = front
            .title
            .or_else(|| first_heading(body))
            .unwrap_or_else(|| plain_stem.to_string());
        let mut docstring = String::from("#sd-write\n");
        if let Some(order) = front.order.or(prefix_order) {
            docstring.push_str(&format!("#sd-z-index: {order}\n"));
        }
        docstring.push_str(body);

        let module = PythonModule {
            name: title.clone(),
            qualname: title,
            source_path: source_path.clone(),
            docstring: Some(docstring),
            z_index: i32::MAX,
            folder: Some(GROUP_NAME.to_string()),
            dir: PAGES_DIR.to_string(),
            ..PythonModule::default()
        };
        sources.push(ModuleSource {
            path: root.join(&source_path),
            source_hash: cache::hash_source(&source_path, &bytes),
            parsed: Some(module),
        });
    }
    Ok((sources, intro))
}

fn find_markdown(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if entry.file_type()?.is_dir() {
            find_markdown(&path, found)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            found.push(path);
        }
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq)]
struct FrontMatter {
    title: Option<String>,
    order: Option<i32>,
}

/// Splits an optional `---` front matter block (`title:` and `order:`
/// lines; other keys are ignored) from the Markdown after it.
fn split_front_matter(text: &str) -> (FrontMatter, &str) {
    let mut front = FrontMatter::default();
    let Some(rest) = text.strip_prefix("---\n") else {
        return (front, text);
    };
    let Some(end) = rest.find("\n---\n").map(|i| (i, i + 5)).or_else(|| {
        rest.strip_suffix("\n---")
            .map(|block| (block.len(), rest.len()))
    }) else {
        return (front, text);
    };
    for line in rest[..end.0].lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        match key.trim() {
            "title" if !value.is_empty() => front.title = Some(value.to_string()),
            "order" => front.order = value.parse().ok(),
            _ => {}
        }
    }
    (front, &rest[end.1..])
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
}

/// For links written in a page: `guide.md` or `../setup.md#install`
/// becomes the `.html` page it is published as. External links, absolute
/// paths and other files are left alone.
pub fn rewrite_link(dest: &str) -> Option<String> {
    if dest.contains("://") || dest.starts_with(['/', '#']) || dest.starts_with("mailto:") {
        return None;
    }
    let (path, anchor) = match dest.find('#') {
        Some(i) => dest.split_at(i),
        None => (dest, ""),
    };
    let stem = path.strip_suffix(".md")?;
    Some(format!("{stem}.html{anchor}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_is_read_and_removed() {
        let (front, body) =
            split_front_matter("---\ntitle: \"Getting started\"\norder: 2\nauthor: x\n---\n# Hi\n");
        assert_eq!(
            front,
            FrontMatter {
                title: Some("Getting started".into()),
                order: Some(2)
            }
        );
        assert_eq!(body, "# Hi\n");
        let (front, body) = split_front_matter("# No front matter\n---\n");
        assert_eq!(front, FrontMatter::default());
        assert_eq!(body, "# No front matter\n---\n");
    }

    #[test]
    fn only_relative_markdown_links_are_rewritten() {
        assert_eq!(rewrite_link("guide.md").as_deref(), Some("guide.html"));
        assert_eq!(
            rewrite_link("../setup.md#install").as_deref(),
            Some("../setup.html#install")
        );
        assert_eq!(rewrite_link("https://x.org/a.md"), None);
        assert_eq!(rewrite_link("/abs.md"), None);
        assert_eq!(rewrite_link("#section"), None);
        assert_eq!(rewrite_link("image.png"), None);
    }
}
