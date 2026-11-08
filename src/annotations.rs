//! Links in type annotations, base classes and docstring types.
//!
//! An annotation is kept as written (`dict[str, list["Engine"]]`) and split
//! into dotted names and the text between them. Each name is resolved by
//! the caller (local definitions first, then the module's imports, see
//! `generator::TypeContext`); a name that resolves becomes a link, every
//! other piece is escaped text. Names that do not resolve (builtins,
//! `typing`, other packages) stay plain, without a warning.

use serde::{Deserialize, Serialize};

/// One name in an annotation that links to its documentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeLink {
    /// The name as written: `Style`, `base.Base`.
    pub name: String,
    /// The link from the site root: `style.html#class.Style`.
    pub target: String,
}

/// Rendered annotations of one function, class or variable, with links.
/// Only set when at least one name links; templates fall back to the
/// plain text otherwise, so pages without links are unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkedTypes {
    /// As HTML: a function's parameters (`PythonFunction::args` joined
    /// with ", "), a class's base classes (joined the same way), or a
    /// variable's annotation.
    pub html: String,
    /// A function's return annotation as HTML.
    pub return_type: Option<String>,
    /// Every linked name, once, in order of appearance. Only for outputs
    /// built in the same run (api.json, Markdown), so it is not part of
    /// the template context or the cache.
    #[serde(skip)]
    pub links: Vec<TypeLink>,
}

/// A piece of annotation text.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// A dotted name that may be linked: `Engine`, `base.Base`.
    Name(&'a str),
    Text(&'a str),
}

/// Names never worth resolving: keywords and the most common builtins.
const PLAIN: [&str; 17] = [
    "None",
    "True",
    "False",
    "int",
    "str",
    "float",
    "bool",
    "bytes",
    "complex",
    "list",
    "dict",
    "set",
    "frozenset",
    "tuple",
    "object",
    "type",
    "self",
];

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Splits annotation text into names and text. Strings are annotations
/// too (forward references, `list["Engine"]`), except inside `Literal[...]`,
/// where they are values.
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut text_start = 0;
    // The name before each open `[`, to know when we are inside Literal[...].
    let mut subscripts: Vec<&str> = Vec::new();
    let mut last_name: &str = "";
    let mut quote: Option<char> = None;
    let mut prev: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else if is_ident_start(c)
                && !prev.is_some_and(is_ident_char)
                && !subscripts.last().is_some_and(|s| s.ends_with("Literal"))
            {
                let end = name_end(text, at);
                push_name(&mut out, text, &mut text_start, at, end);
                last_name = &text[at..end];
                while chars.peek().is_some_and(|&(i, _)| i < end) {
                    chars.next();
                }
                prev = text[..end].chars().next_back();
                continue;
            }
            prev = Some(c);
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' => subscripts.push(last_name),
            ']' => {
                subscripts.pop();
            }
            _ if is_ident_start(c) && !prev.is_some_and(|p| is_ident_char(p) || p == '.') => {
                let end = name_end(text, at);
                push_name(&mut out, text, &mut text_start, at, end);
                last_name = &text[at..end];
                while chars.peek().is_some_and(|&(i, _)| i < end) {
                    chars.next();
                }
                prev = text[..end].chars().next_back();
                continue;
            }
            _ => {}
        }
        if !c.is_whitespace() && !is_ident_start(c) {
            last_name = "";
        }
        prev = Some(c);
    }
    if text_start < text.len() {
        out.push(Piece::Text(&text[text_start..]));
    }
    out
}

/// The end of the dotted name starting at `start`.
fn name_end(text: &str, start: usize) -> usize {
    let mut end = start;
    let mut expect_start = true;
    for (offset, c) in text[start..].char_indices() {
        if expect_start {
            if !is_ident_start(c) {
                break;
            }
            expect_start = false;
            end = start + offset + c.len_utf8();
        } else if is_ident_char(c) {
            end = start + offset + c.len_utf8();
        } else if c == '.' {
            expect_start = true;
        } else {
            break;
        }
    }
    end
}

fn push_name<'a>(
    out: &mut Vec<Piece<'a>>,
    text: &'a str,
    text_start: &mut usize,
    at: usize,
    end: usize,
) {
    if *text_start < at {
        out.push(Piece::Text(&text[*text_start..at]));
    }
    out.push(Piece::Name(&text[at..end]));
    *text_start = end;
}

/// Text escaped exactly as a Tera template escapes it.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape(text, &mut out);
    out
}

/// Escapes like Tera's `escape_html` (`& < > " '`), so linked and unlinked
/// text look the same.
fn escape(text: &str, out: &mut String) {
    let mut start = 0;
    for (at, byte) in text.bytes().enumerate() {
        let entity = match byte {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#39;",
            _ => continue,
        };
        out.push_str(&text[start..at]);
        out.push_str(entity);
        start = at + 1;
    }
    out.push_str(&text[start..]);
}

/// The annotation as HTML with every resolvable name linked, or None when
/// no name resolves (the caller then shows the text as before). `resolve`
/// returns a link from the site root; `prefix` leads from the page to the
/// root. Linked names are added to `links` once each.
pub fn link(
    text: &str,
    resolve: &mut dyn FnMut(&str) -> Option<String>,
    prefix: &str,
    links: &mut Vec<TypeLink>,
) -> Option<String> {
    // Resolve first: most annotations link nothing, and then no HTML is built.
    let pieces = pieces(text);
    let targets: Vec<Option<String>> = pieces
        .iter()
        .map(|piece| match piece {
            Piece::Name(name) if !PLAIN.contains(name) => resolve(name),
            _ => None,
        })
        .collect();
    if targets.iter().all(Option::is_none) {
        return None;
    }
    let mut html = String::with_capacity(text.len() + 64);
    for (piece, target) in pieces.into_iter().zip(targets) {
        match piece {
            Piece::Text(t) => escape(t, &mut html),
            Piece::Name(name) => match target {
                Some(target) => {
                    html.push_str("<a class=\"type-link\" href=\"");
                    // External (`--intersphinx`) links are absolute.
                    if !target.contains("://") {
                        escape(prefix, &mut html);
                    }
                    escape(&target, &mut html);
                    html.push_str("\">");
                    escape(name, &mut html);
                    html.push_str("</a>");
                    if !links.iter().any(|l| l.name == name) {
                        links.push(TypeLink {
                            name: name.to_string(),
                            target,
                        });
                    }
                }
                None => escape(name, &mut html),
            },
        }
    }
    Some(html)
}

/// A parameter as swiftdoc prints it (`name: ann = default`, `*args: T`,
/// `name=1`) as HTML with its annotation linked; None when nothing links.
pub fn link_parameter(
    param: &str,
    resolve: &mut dyn FnMut(&str) -> Option<String>,
    prefix: &str,
    links: &mut Vec<TypeLink>,
) -> Option<String> {
    let (_, annotation, _) = crate::docstring::parse_parameter(param)?;
    let annotation = annotation?;
    // `annotation` is a slice of `param`: link it, escape the rest.
    let start = annotation.as_ptr() as usize - param.as_ptr() as usize;
    let end = start + annotation.len();
    let linked = link(annotation, resolve, prefix, links)?;
    let mut html = String::new();
    escape(&param[..start], &mut html);
    html.push_str(&linked);
    escape(&param[end..], &mut html);
    Some(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<&str> {
        pieces(text)
            .into_iter()
            .filter_map(|p| match p {
                Piece::Name(n) => Some(n),
                Piece::Text(_) => None,
            })
            .collect()
    }

    #[test]
    fn splits_names_out_of_annotations() {
        assert_eq!(names("Optional[Style]"), ["Optional", "Style"]);
        assert_eq!(
            names("dict[str, list[\"Engine\"]]"),
            ["dict", "str", "list", "Engine"]
        );
        assert_eq!(
            names("Callable[[int], Optional[base.Style]]"),
            ["Callable", "int", "Optional", "base.Style"]
        );
        assert_eq!(names("Engine | None"), ["Engine", "None"]);
        assert_eq!(names("'pkg.Engine'"), ["pkg.Engine"]);
        // Literal values are not names; numbers are not either.
        assert_eq!(
            names("Literal[\"red\", 'blue'] | Engine"),
            ["Literal", "Engine"]
        );
        assert_eq!(
            names("Annotated[int, 1e5, x.y]"),
            ["Annotated", "int", "x.y"]
        );
        // Every byte is kept.
        for text in [
            "dict[str, list[\"Engine\"]]",
            "a.b . c",
            "ünï[x]",
            "x[",
            "]]",
            "'a",
        ] {
            let joined: String = pieces(text)
                .iter()
                .map(|p| match p {
                    Piece::Name(n) | Piece::Text(n) => *n,
                })
                .collect();
            assert_eq!(joined, text);
        }
    }

    #[test]
    fn links_each_name_and_escapes_the_rest() {
        let mut resolve = |name: &str| match name {
            "Engine" => Some("core/engine.html#class.Engine".to_string()),
            "base.Style" => Some("base.html#class.Style".to_string()),
            _ => None,
        };
        let mut links = Vec::new();
        let html = link(
            "dict[str, list[\"Engine\"]] | Callable[[int], Optional[base.Style]] | None",
            &mut resolve,
            "../",
            &mut links,
        )
        .unwrap();
        assert_eq!(
            html,
            "dict[str, list[&quot;<a class=\"type-link\" href=\"../core/engine.html#class.Engine\">Engine</a>&quot;]] \
             | Callable[[int], Optional[<a class=\"type-link\" href=\"../base.html#class.Style\">base.Style</a>]] | None"
        );
        assert_eq!(
            links,
            [
                TypeLink {
                    name: "Engine".into(),
                    target: "core/engine.html#class.Engine".into()
                },
                TypeLink {
                    name: "base.Style".into(),
                    target: "base.html#class.Style".into()
                }
            ]
        );
        assert_eq!(link("Optional[int]", &mut resolve, "", &mut links), None);
    }

    #[test]
    fn parameters_link_only_their_annotation() {
        let mut resolve =
            |name: &str| (name == "Engine").then(|| "e.html#class.Engine".to_string());
        let mut links = Vec::new();
        assert_eq!(
            link_parameter("engine: Engine = Engine()", &mut resolve, "./", &mut links).unwrap(),
            "engine: <a class=\"type-link\" href=\"./e.html#class.Engine\">Engine</a> = Engine()"
        );
        assert_eq!(
            link_parameter("*args: \"Engine\"", &mut resolve, "", &mut links).unwrap(),
            "*args: &quot;<a class=\"type-link\" href=\"e.html#class.Engine\">Engine</a>&quot;"
        );
        assert_eq!(
            link_parameter("x=Engine", &mut resolve, "", &mut links),
            None
        );
        assert_eq!(link_parameter("/", &mut resolve, "", &mut links), None);
    }
}
