//! Structured docstring sections: Google (`Args:`), NumPy
//! (`Parameters` + `----------`) and reST (`:param x:`) styles.
//!
//! [`split`] takes a dedented docstring and returns the text that is not in
//! a recognized section (the body, still rendered as Markdown) plus the
//! parsed sections. It never drops text: a section it cannot parse is left
//! in the body as written, and every word of the input ends up either in the
//! body or in a section field. Descriptions stay raw Markdown here; the
//! generator renders them through the same Markdown and `#sd-` macro
//! pipeline as the rest of the docstring.

use serde::{Deserialize, Serialize};

/// `--docstring-style`.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocstringStyle {
    /// Detect the style of each docstring
    #[default]
    Auto,
    /// `Args:` / `Returns:` sections
    Google,
    /// `Parameters` headers underlined with dashes
    Numpy,
    /// `:param x:` fields
    Rest,
    /// No sections: every docstring is plain Markdown
    Markdown,
}

impl DocstringStyle {
    fn name(self) -> &'static str {
        match self {
            DocstringStyle::Auto => "auto",
            DocstringStyle::Google => "google",
            DocstringStyle::Numpy => "numpy",
            DocstringStyle::Rest => "rest",
            DocstringStyle::Markdown => "markdown",
        }
    }
}

/// One documented parameter, attribute, return value or exception.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocItem {
    /// The parameter or attribute name; None for return values and exceptions.
    pub name: Option<String>,
    /// The type as written in the docstring, else from the signature. For an
    /// exception, the exception class.
    #[serde(rename = "type")]
    pub ty: Option<String>,
    /// The default value, from the signature.
    pub default: Option<String>,
    /// Raw Markdown.
    pub description: String,
    /// Rendered HTML, filled in by the generator.
    pub description_html: String,
    /// `ty` as HTML with links to documented types; None when nothing links.
    pub type_html: Option<String>,
    /// The names in `ty` that link, filled in with `type_html`. Not in the
    /// template context or the cache (see `LinkedTypes::links`).
    #[serde(skip)]
    pub type_links: Vec<crate::annotations::TypeLink>,
}

/// A free-text section: an example, a note, a warning or "see also".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocBlock {
    /// Raw Markdown.
    pub text: String,
    /// Rendered HTML, filled in by the generator. Notes and warnings are
    /// rendered as the same boxes as `#sd-note` and `#sd-warning`.
    pub html: String,
}

/// The sections of one docstring, in a fixed order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocSections {
    /// "google", "numpy" or "rest".
    pub style: String,
    pub params: Vec<DocItem>,
    pub keyword_params: Vec<DocItem>,
    pub other_params: Vec<DocItem>,
    pub attributes: Vec<DocItem>,
    pub returns: Vec<DocItem>,
    pub yields: Vec<DocItem>,
    pub raises: Vec<DocItem>,
    pub examples: Vec<DocBlock>,
    pub notes: Vec<DocBlock>,
    pub warnings: Vec<DocBlock>,
    pub see_also: Vec<DocBlock>,
}

impl DocSections {
    pub fn is_empty(&self) -> bool {
        self.items().all(|(_, items)| items.is_empty())
            && self.blocks().all(|(_, blocks)| blocks.is_empty())
    }

    /// Every item list with its kind.
    pub fn items(&self) -> impl Iterator<Item = (Kind, &Vec<DocItem>)> {
        [
            (Kind::Params, &self.params),
            (Kind::KeywordParams, &self.keyword_params),
            (Kind::OtherParams, &self.other_params),
            (Kind::Attributes, &self.attributes),
            (Kind::Returns, &self.returns),
            (Kind::Yields, &self.yields),
            (Kind::Raises, &self.raises),
        ]
        .into_iter()
    }

    pub fn items_mut(&mut self) -> impl Iterator<Item = &mut DocItem> {
        self.params
            .iter_mut()
            .chain(self.keyword_params.iter_mut())
            .chain(self.other_params.iter_mut())
            .chain(self.attributes.iter_mut())
            .chain(self.returns.iter_mut())
            .chain(self.yields.iter_mut())
            .chain(self.raises.iter_mut())
    }

    /// Every block list with its kind.
    pub fn blocks(&self) -> impl Iterator<Item = (Kind, &Vec<DocBlock>)> {
        [
            (Kind::Examples, &self.examples),
            (Kind::Notes, &self.notes),
            (Kind::Warnings, &self.warnings),
            (Kind::SeeAlso, &self.see_also),
        ]
        .into_iter()
    }

    pub fn blocks_mut(&mut self) -> impl Iterator<Item = (Kind, &mut DocBlock)> {
        let examples = self.examples.iter_mut().map(|b| (Kind::Examples, b));
        let notes = self.notes.iter_mut().map(|b| (Kind::Notes, b));
        let warnings = self.warnings.iter_mut().map(|b| (Kind::Warnings, b));
        let see_also = self.see_also.iter_mut().map(|b| (Kind::SeeAlso, b));
        examples.chain(notes).chain(warnings).chain(see_also)
    }

    fn item_list(&mut self, kind: Kind) -> Option<&mut Vec<DocItem>> {
        match kind {
            Kind::Params => Some(&mut self.params),
            Kind::KeywordParams => Some(&mut self.keyword_params),
            Kind::OtherParams => Some(&mut self.other_params),
            Kind::Attributes => Some(&mut self.attributes),
            Kind::Returns => Some(&mut self.returns),
            Kind::Yields => Some(&mut self.yields),
            Kind::Raises => Some(&mut self.raises),
            _ => None,
        }
    }

    fn block_list(&mut self, kind: Kind) -> Option<&mut Vec<DocBlock>> {
        match kind {
            Kind::Examples => Some(&mut self.examples),
            Kind::Notes => Some(&mut self.notes),
            Kind::Warnings => Some(&mut self.warnings),
            Kind::SeeAlso => Some(&mut self.see_also),
            _ => None,
        }
    }
}

/// The kind of a section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Params,
    KeywordParams,
    OtherParams,
    Attributes,
    Returns,
    Yields,
    Raises,
    Examples,
    Notes,
    Warnings,
    SeeAlso,
}

impl Kind {
    /// Sections whose entries are named (`x (int): ...`).
    fn is_named(self) -> bool {
        matches!(
            self,
            Kind::Params | Kind::KeywordParams | Kind::OtherParams | Kind::Attributes
        )
    }

    fn is_block(self) -> bool {
        matches!(
            self,
            Kind::Examples | Kind::Notes | Kind::Warnings | Kind::SeeAlso
        )
    }

    /// From a section title, case-insensitively ("Args", "Keyword Arguments", ...).
    fn from_title(title: &str) -> Option<Kind> {
        let title = title.trim();
        // Called on every candidate line: the longest title is 18 bytes, so
        // longer text is rejected before lowercasing it.
        if title.len() > 18 || !title.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return None;
        }
        let title = title.to_ascii_lowercase();
        Some(match title.as_str() {
            "args" | "arguments" | "parameters" | "params" => Kind::Params,
            "keyword args" | "keyword arguments" | "keyword parameters" | "kwargs" => {
                Kind::KeywordParams
            }
            "other parameters" | "other args" | "other arguments" => Kind::OtherParams,
            "attributes" => Kind::Attributes,
            "returns" | "return" => Kind::Returns,
            "yields" | "yield" => Kind::Yields,
            "raises" | "raise" | "exceptions" | "except" => Kind::Raises,
            "examples" | "example" => Kind::Examples,
            "notes" | "note" => Kind::Notes,
            "warnings" | "warning" => Kind::Warnings,
            "see also" => Kind::SeeAlso,
            _ => return None,
        })
    }
}

/// The body that is left once the sections are taken out, and the sections.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub body: String,
    pub sections: DocSections,
}

/// A quick check on the raw text, before any dedenting: false when no line
/// could start a section, so most plain docstrings skip [`split`]. Every
/// header has a colon (`Args:`, `:param`, `.. note::`) or a dashed underline.
pub fn may_have_sections(doc: &str, style: DocstringStyle) -> bool {
    style != DocstringStyle::Markdown && (doc.contains(':') || doc.contains("---"))
}

/// Splits a dedented docstring into its body and sections. None when the
/// style is `markdown`, or no section was found: the docstring is then
/// rendered exactly as before.
pub fn split(doc: &str, style: DocstringStyle) -> Option<Split> {
    let order: Vec<DocstringStyle> = match style {
        DocstringStyle::Markdown => return None,
        DocstringStyle::Auto => detect(doc),
        other => vec![other],
    };
    let mut sections = DocSections {
        style: order.first()?.name().to_string(),
        ..DocSections::default()
    };
    // Each style takes its sections out of what the previous one left, so
    // a docstring that mixes styles loses none of them.
    let mut body = doc.to_string();
    for style in order {
        let lines: Vec<&str> = body.lines().collect();
        let code = code_mask(&lines);
        body = match style {
            DocstringStyle::Google => google(&lines, &code, &mut sections),
            DocstringStyle::Numpy => numpy(&lines, &code, &mut sections),
            DocstringStyle::Rest => rest(&lines, &code, &mut sections),
            DocstringStyle::Auto | DocstringStyle::Markdown => unreachable!(),
        };
    }
    if sections.is_empty() {
        return None;
    }
    Some(Split {
        body: trim_blank_lines(&body),
        sections,
    })
}

/// The styles a docstring uses, most used first: the number of NumPy
/// headers, reST fields and Google headers it has. On a tie NumPy's
/// underlined headers win, being the most specific, then reST fields.
fn detect(doc: &str) -> Vec<DocstringStyle> {
    let lines: Vec<&str> = doc.lines().collect();
    let code = code_mask(&lines);
    let prose = || (0..lines.len()).filter(|&i| !code[i]);
    let mut counts = [
        (
            DocstringStyle::Numpy,
            prose()
                .filter(|&i| numpy_header(&lines, i).is_some())
                .count(),
        ),
        (
            DocstringStyle::Rest,
            prose()
                .filter(|&i| indent_of(lines[i]) == 0 && rest_field(lines[i]).is_some())
                .count(),
        ),
        (
            DocstringStyle::Google,
            prose()
                .filter(|&i| google_header(&lines, i).is_some())
                .count(),
        ),
    ];
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(style, _)| style)
        .collect()
}

/// Marks the lines inside ``` fences and `#sd-code` blocks (and their
/// delimiters), where nothing is a section header.
fn code_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = Vec::with_capacity(lines.len());
    let mut in_fence = false;
    let mut in_sd_code = false;
    for line in lines {
        let trimmed = line.trim();
        if in_sd_code {
            mask.push(true);
            in_sd_code = trimmed != "```";
        } else if trimmed.starts_with("#sd-code") {
            mask.push(true);
            in_sd_code = true;
        } else if trimmed.starts_with("```") {
            mask.push(true);
            in_fence = !in_fence;
        } else {
            mask.push(in_fence);
        }
    }
    mask
}

// ---------------------------------------------------------------------------
// Line helpers

/// Indentation width, a tab counting as 4.
fn indent_of(line: &str) -> usize {
    let mut width = 0;
    for c in line.chars() {
        match c {
            ' ' => width += 1,
            '\t' => width += 4,
            _ => break,
        }
    }
    width
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Removes up to `width` columns of leading whitespace.
fn strip_indent(line: &str, width: usize) -> &str {
    let mut removed = 0;
    for (at, c) in line.char_indices() {
        if removed >= width {
            return &line[at..];
        }
        match c {
            ' ' => removed += 1,
            '\t' => removed += 4,
            _ => return &line[at..],
        }
    }
    ""
}

/// The lines with their common indentation removed; blank lines become "".
fn dedent<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let min = lines
        .iter()
        .filter(|l| !is_blank(l))
        .map(|l| indent_of(l))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if is_blank(l) {
                ""
            } else {
                strip_indent(l, min)
            }
        })
        .collect()
}

fn trim_blank_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| !is_blank(l))
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !is_blank(l))
        .map_or(start, |i| i + 1);
    lines[start..end].join("\n")
}

/// A description from its first line (possibly empty) and the indented
/// lines after it.
fn description(first: &str, rest: &[&str]) -> String {
    let mut text = first.trim().to_string();
    let rest = dedent(rest);
    for line in rest {
        if !text.is_empty() || !line.is_empty() {
            text.push('\n');
        }
        text.push_str(line);
    }
    trim_blank_lines(&text)
}

/// Splits `lines` (already dedented to the section's own indentation) into
/// entries: each starts at a line with no indentation and takes the
/// indented or blank lines after it. None if the content starts indented.
fn entries<'a>(lines: &[&'a str]) -> Option<Vec<(&'a str, Vec<&'a str>)>> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for &line in lines {
        if is_blank(line) {
            if let Some((_, rest)) = out.last_mut() {
                rest.push(line);
            }
        } else if indent_of(line) == 0 {
            out.push((line, Vec::new()));
        } else {
            out.last_mut()?.1.push(line);
        }
    }
    Some(out)
}

/// `x`, `*args`, `**kwargs` or `x, y`.
fn is_name_list(text: &str) -> bool {
    !text.is_empty()
        && text.split(',').all(|name| {
            let name = name.trim().trim_start_matches('*');
            // NumPy's `a1, a2, ... : array_like`
            if name == "..." {
                return true;
            }
            let mut chars = name.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        })
}

/// Whether text reads like a type rather than a sentence: `int`,
/// `list[str]`, `dict[str, int] | None`, `int or None`, `Optional[int]`.
fn is_type_like(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.len() > 80 || text.ends_with('.') {
        return false;
    }
    // Drop everything inside brackets, then allow only `|`, `,` and `or`
    // between single words.
    let mut depth = 0i32;
    let mut outer = String::new();
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            _ if depth == 0 => outer.push(c),
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    let words: Vec<&str> = outer
        .split(|c: char| c.is_whitespace() || c == '|' || c == ',')
        .filter(|w| !w.is_empty() && *w != "or" && *w != "optional")
        .collect();
    !words.is_empty()
        && words.iter().all(|w| {
            w.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '*' | '\'' | '"' | '-'))
        })
}

/// Drops a trailing ", optional" (NumPy and Google mark optional
/// parameters that way; the default comes from the signature).
fn strip_optional(ty: &str) -> String {
    let ty = ty.trim();
    for suffix in [", optional", ",optional", " optional"] {
        if let Some(stripped) = ty.strip_suffix(suffix) {
            return stripped.trim().to_string();
        }
    }
    if ty == "optional" {
        return String::new();
    }
    ty.to_string()
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

// ---------------------------------------------------------------------------
// Google

/// A Google section header at line `i`: `Args:` alone on its line, followed
/// by indented content, or `Returns: text` on one line (for the sections
/// whose content is free text). Returns the kind and any inline text.
fn google_header<'a>(lines: &[&'a str], i: usize) -> Option<(Kind, &'a str)> {
    let line = lines[i];
    if indent_of(line) != 0 {
        return None;
    }
    let (title, inline) = line.trim().split_once(':')?;
    let kind = Kind::from_title(title)?;
    let inline = inline.trim();
    if inline.is_empty() {
        let next = lines[i + 1..].iter().find(|l| !is_blank(l))?;
        (indent_of(next) > 0).then_some((kind, inline))
    } else {
        (!kind.is_named()).then_some((kind, inline))
    }
}

fn google(lines: &[&str], code: &[bool], sections: &mut DocSections) -> String {
    let mut body: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let header = (!code[i]).then(|| google_header(lines, i)).flatten();
        let Some((kind, inline)) = header else {
            body.push(lines[i]);
            i += 1;
            continue;
        };
        let mut end = i + 1;
        while end < lines.len() && (is_blank(lines[end]) || indent_of(lines[end]) > 0) {
            end += 1;
        }
        // Trailing blank lines belong to the body, as spacing.
        let mut content_end = end;
        while content_end > i + 1 && is_blank(lines[content_end - 1]) {
            content_end -= 1;
        }
        let content = &lines[i + 1..content_end];
        if parse_google_section(kind, inline, content, sections) {
            body.extend(&lines[content_end..end]);
        } else {
            body.extend(&lines[i..end]);
        }
        i = end;
    }
    body.join("\n")
}

/// Parses one Google section into `sections`; false (and nothing added)
/// when its content does not fit the section's shape.
fn parse_google_section(
    kind: Kind,
    inline: &str,
    content: &[&str],
    sections: &mut DocSections,
) -> bool {
    if kind.is_block() {
        let text = description(inline, content);
        if text.is_empty() {
            return false;
        }
        sections.block_list(kind).unwrap().push(DocBlock {
            text,
            html: String::new(),
        });
        return true;
    }
    let content = dedent(content);
    let parsed = match kind {
        Kind::Returns | Kind::Yields => google_return(inline, &content).map(|item| vec![item]),
        // "Raises: ValueError: when ..." on the header line.
        Kind::Raises if !inline.is_empty() => {
            let mut lines = vec![inline];
            lines.extend(content.iter().copied());
            google_items(kind, &lines)
        }
        _ => google_items(kind, &content),
    };
    match parsed {
        Some(items) if !items.is_empty() => {
            sections.item_list(kind).unwrap().extend(items);
            true
        }
        _ => false,
    }
}

/// `x (int): desc`, `x: desc`, `*args: desc`; for raises `ValueError: desc`.
fn google_items(kind: Kind, content: &[&str]) -> Option<Vec<DocItem>> {
    let mut items = Vec::new();
    for (first, rest) in entries(content)? {
        let item = if kind == Kind::Raises {
            let (ty, text) = first.split_once(':')?;
            if !is_type_like(ty) || ty.contains(' ') && !ty.contains(',') {
                return None;
            }
            DocItem {
                ty: Some(ty.trim().to_string()),
                description: description(text, &rest),
                ..DocItem::default()
            }
        } else {
            google_named(first, &rest)?
        };
        items.push(item);
    }
    Some(items)
}

fn google_named(first: &str, rest: &[&str]) -> Option<DocItem> {
    let first = first.trim();
    // The name runs up to "(" or ":".
    let name_end = first.find(['(', ':'])?;
    let name = first[..name_end].trim();
    if !is_name_list(name) {
        return None;
    }
    let mut after = &first[name_end..];
    let mut ty = None;
    if after.starts_with('(') {
        let close = matching_paren(after)?;
        ty = non_empty(strip_optional(&after[1..close]));
        after = after[close + 1..].trim_start();
    }
    let text = after.strip_prefix(':')?;
    Some(DocItem {
        name: Some(name.to_string()),
        ty,
        description: description(text, rest),
        ..DocItem::default()
    })
}

/// Index of the `)` matching the `(` at the start of `text`.
fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 0;
    for (at, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return (c == ')').then_some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// A Google `Returns:` / `Yields:` section: one entry, `Type: desc` when
/// the part before the first colon reads like a type, else just text.
fn google_return(inline: &str, content: &[&str]) -> Option<DocItem> {
    let (first, rest): (&str, &[&str]) = if inline.is_empty() {
        let at = content.iter().position(|l| !is_blank(l))?;
        (content[at], &content[at + 1..])
    } else {
        (inline, content)
    };
    let (ty, text) = match first.split_once(':') {
        Some((ty, text)) if is_type_like(ty) && !text.starts_with("//") => {
            (non_empty(ty.trim().to_string()), text)
        }
        _ => (None, first),
    };
    // Continuation lines of a free-text return are often not indented.
    let mut item = DocItem {
        ty,
        ..DocItem::default()
    };
    let mut text_lines = vec![text.trim()];
    text_lines.extend(rest.iter().copied());
    item.description = trim_blank_lines(&text_lines.join("\n"));
    Some(item)
}

// ---------------------------------------------------------------------------
// NumPy

/// A NumPy header at line `i`: a known title, then a line of dashes.
fn numpy_header(lines: &[&str], i: usize) -> Option<Kind> {
    if !numpy_any_header(lines, i) {
        return None;
    }
    Kind::from_title(lines[i])
}

/// Any underlined title, known or not (`Methods`, `References`, ...): where
/// the section before it ends.
fn numpy_any_header(lines: &[&str], i: usize) -> bool {
    let title = lines[i];
    let Some(underline) = lines.get(i + 1).map(|l| l.trim()) else {
        return false;
    };
    indent_of(title) == 0
        && !is_blank(title)
        && underline.len() >= 3
        && underline.chars().all(|c| c == '-')
}

fn numpy(lines: &[&str], code: &[bool], sections: &mut DocSections) -> String {
    let header = |i: usize| (!code[i]).then(|| numpy_header(lines, i)).flatten();
    let mut body: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(kind) = header(i) else {
            body.push(lines[i]);
            i += 1;
            continue;
        };
        let start = i + 2;
        let mut end = start;
        while end < lines.len() && !(!code[end] && numpy_any_header(lines, end)) {
            end += 1;
        }
        let mut content_end = end;
        while content_end > start && is_blank(lines[content_end - 1]) {
            content_end -= 1;
        }
        if parse_numpy_section(kind, &lines[start..content_end], sections) {
            body.extend(&lines[content_end..end]);
        } else {
            body.extend(&lines[i..end]);
        }
        i = end;
    }
    body.join("\n")
}

fn parse_numpy_section(kind: Kind, content: &[&str], sections: &mut DocSections) -> bool {
    if kind.is_block() {
        let text = description("", content);
        if text.is_empty() {
            return false;
        }
        sections.block_list(kind).unwrap().push(DocBlock {
            text,
            html: String::new(),
        });
        return true;
    }
    let Some(entries) = entries(&dedent(content)) else {
        return false;
    };
    let mut items = Vec::new();
    for (first, rest) in entries {
        let first = first.trim();
        let item = match kind {
            Kind::Raises => match first.split_once(':') {
                _ if is_type_like(first) => DocItem {
                    ty: Some(first.to_string()),
                    description: description("", &rest),
                    ..DocItem::default()
                },
                // `ValueError: shape mismatch`
                Some((ty, text)) if is_type_like(ty) => DocItem {
                    ty: Some(ty.trim().to_string()),
                    description: description(text, &rest),
                    ..DocItem::default()
                },
                _ => return false,
            },
            Kind::Returns | Kind::Yields => match first.split_once(':') {
                // NumPy's spaced ` : ` always separates a name from its
                // type, even an unusual name (`retval, [weights] : float`).
                Some((name, ty)) if is_name_list(name.trim()) || first.contains(" : ") => DocItem {
                    name: Some(name.trim().to_string()),
                    ty: non_empty(ty.trim().to_string()),
                    description: description("", &rest),
                    ..DocItem::default()
                },
                _ if is_type_like(first) => DocItem {
                    ty: Some(first.to_string()),
                    description: description("", &rest),
                    ..DocItem::default()
                },
                // A sentence instead of a type: the whole entry is the description.
                _ => DocItem {
                    description: description(first, &rest),
                    ..DocItem::default()
                },
            },
            _ => {
                let (name, ty) = match first.split_once(':') {
                    Some((name, ty)) => (name.trim(), non_empty(strip_optional(ty))),
                    None => (first, None),
                };
                if !is_name_list(name) {
                    return false;
                }
                DocItem {
                    name: Some(name.to_string()),
                    ty,
                    description: description("", &rest),
                    ..DocItem::default()
                }
            }
        };
        items.push(item);
    }
    if items.is_empty() {
        return false;
    }
    sections.item_list(kind).unwrap().extend(items);
    true
}

// ---------------------------------------------------------------------------
// reST

/// A reST field this module understands: (field name, argument, text).
/// `:param int x: text` gives ("param", "int x", "text").
fn rest_field(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.trim_start().strip_prefix(':')?;
    let close = rest.find(':')?;
    let (head, text) = (&rest[..close], &rest[close + 1..]);
    let (field, argument) = match head.split_once(char::is_whitespace) {
        Some((field, argument)) => (field, argument.trim()),
        None => (head, ""),
    };
    let known = matches!(
        field,
        "param"
            | "parameter"
            | "arg"
            | "argument"
            | "key"
            | "keyword"
            | "kwarg"
            | "kwparam"
            | "type"
            | "kwtype"
            | "raises"
            | "raise"
            | "except"
            | "exception"
            | "returns"
            | "return"
            | "rtype"
            | "yields"
            | "yield"
            | "ytype"
            | "ivar"
            | "cvar"
            | "var"
            | "vartype"
    );
    known.then_some((field, argument, text))
}

/// A reST directive this module maps onto a block: `.. note::`.
fn rest_directive(line: &str) -> Option<(Kind, &str)> {
    let rest = line.trim().strip_prefix("..")?.trim_start();
    let (name, text) = rest.split_once("::")?;
    let kind = match name.trim() {
        "note" => Kind::Notes,
        "warning" | "caution" | "attention" => Kind::Warnings,
        "seealso" => Kind::SeeAlso,
        _ => return None,
    };
    Some((kind, text))
}

fn rest(lines: &[&str], code: &[bool], sections: &mut DocSections) -> String {
    let mut body: Vec<&str> = Vec::new();
    // (target list, name) -> type, from `:type x:` / `:vartype x:` fields
    // seen before or after their `:param x:`.
    // Each with the lines it came from, put back in the body if it cannot
    // be applied (a second `:rtype:`, or a parameter that already has a type).
    let mut types: Vec<(Kind, String, String, std::ops::Range<usize>)> = Vec::new();
    let mut return_type: Option<(String, std::ops::Range<usize>)> = None;
    let mut yield_type: Option<(String, std::ops::Range<usize>)> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let at_margin = indent_of(line) == 0 && !code[i];
        let field = at_margin.then(|| rest_field(line)).flatten();
        let directive = at_margin.then(|| rest_directive(line)).flatten();
        if field.is_none() && directive.is_none() {
            body.push(line);
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < lines.len()
            && (indent_of(lines[end]) > 0
                || (is_blank(lines[end])
                    && lines[end + 1..]
                        .iter()
                        .find(|l| !is_blank(l))
                        .is_some_and(|l| indent_of(l) > 0)))
        {
            end += 1;
        }
        let rest_lines = &lines[i + 1..end];
        if let Some((kind, text)) = directive {
            let text = description(text, rest_lines);
            if text.is_empty() {
                body.extend(&lines[i..end]);
            } else {
                sections.block_list(kind).unwrap().push(DocBlock {
                    text,
                    html: String::new(),
                });
            }
            i = end;
            continue;
        }
        let (name, argument, text) = field.unwrap();
        let text = description(text, rest_lines);
        let ok = match name {
            "param" | "parameter" | "arg" | "argument" | "key" | "keyword" | "kwarg"
            | "kwparam" | "ivar" | "cvar" | "var" => {
                let kind = match name {
                    "key" | "keyword" | "kwarg" | "kwparam" => Kind::KeywordParams,
                    "ivar" | "cvar" | "var" => Kind::Attributes,
                    _ => Kind::Params,
                };
                // `:param x:` or `:param int x:` (the type comes first).
                match argument.rsplit_once(char::is_whitespace) {
                    Some((ty, param)) if is_name_list(param) => {
                        sections.item_list(kind).unwrap().push(DocItem {
                            name: Some(param.to_string()),
                            ty: non_empty(ty.trim().to_string()),
                            description: text,
                            ..DocItem::default()
                        });
                        true
                    }
                    None if is_name_list(argument) => {
                        sections.item_list(kind).unwrap().push(DocItem {
                            name: Some(argument.to_string()),
                            description: text,
                            ..DocItem::default()
                        });
                        true
                    }
                    _ => false,
                }
            }
            "type" | "kwtype" | "vartype" if is_name_list(argument) && !text.is_empty() => {
                let kind = match name {
                    "kwtype" => Kind::KeywordParams,
                    "vartype" => Kind::Attributes,
                    _ => Kind::Params,
                };
                if types.iter().any(|(k, n, _, _)| *k == kind && n == argument) {
                    false
                } else {
                    types.push((kind, argument.to_string(), text, i..end));
                    true
                }
            }
            "raises" | "raise" | "except" | "exception" => {
                let ty = non_empty(argument.to_string());
                if ty.as_deref().is_none_or(is_type_like) {
                    sections.raises.push(DocItem {
                        ty,
                        description: text,
                        ..DocItem::default()
                    });
                    true
                } else {
                    false
                }
            }
            "returns" | "return" if argument.is_empty() => {
                sections.returns.push(DocItem {
                    description: text,
                    ..DocItem::default()
                });
                true
            }
            "yields" | "yield" if argument.is_empty() => {
                sections.yields.push(DocItem {
                    description: text,
                    ..DocItem::default()
                });
                true
            }
            "rtype" if argument.is_empty() && !text.is_empty() && return_type.is_none() => {
                return_type = Some((text, i..end));
                true
            }
            "ytype" if argument.is_empty() && !text.is_empty() && yield_type.is_none() => {
                yield_type = Some((text, i..end));
                true
            }
            _ => false,
        };
        if !ok {
            body.extend(&lines[i..end]);
        }
        i = end;
    }

    let mut leftover = Vec::new();
    for (kind, name, ty, range) in types {
        let list = sections.item_list(kind).unwrap();
        match list.iter_mut().find(|p| p.name.as_deref() == Some(&name)) {
            Some(item) if item.ty.is_none() => item.ty = Some(ty),
            Some(_) => leftover.push(range),
            // A type for a parameter that has no `:param:` still documents it.
            None => list.push(DocItem {
                name: Some(name),
                ty: Some(ty),
                ..DocItem::default()
            }),
        }
    }
    for (list, ty) in [
        (&mut sections.returns, return_type),
        (&mut sections.yields, yield_type),
    ] {
        if let Some((ty, _)) = ty {
            match list.first_mut() {
                Some(item) => item.ty = Some(ty),
                None => list.push(DocItem {
                    ty: Some(ty),
                    ..DocItem::default()
                }),
            }
        }
    }
    for range in leftover {
        body.extend(&lines[range]);
    }
    body.join("\n")
}

// ---------------------------------------------------------------------------
// Signature merge

/// One parameter of a signature as swiftdoc prints it: `name`, `name=1`,
/// `name: int`, `name: int = 1`, `*args`, `**kwargs: Any`.
pub(crate) fn parse_parameter(param: &str) -> Option<(&str, Option<&str>, Option<&str>)> {
    let stars = param.len() - param.trim_start_matches('*').len();
    let rest = &param[stars..];
    let name_len = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if name_len == 0 {
        return None; // "/" or "*"
    }
    let name = &rest[..name_len];
    let after = &rest[name_len..];
    if let Some(default) = after.strip_prefix('=') {
        return Some((name, None, Some(default)));
    }
    let Some(annotated) = after.strip_prefix(": ") else {
        return Some((name, None, None));
    };
    // The default follows the first " = " outside brackets.
    let mut depth = 0i32;
    let bytes = annotated.as_bytes();
    for (at, &b) in bytes.iter().enumerate() {
        match b {
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => depth -= 1,
            b' ' if depth == 0 && annotated[at..].starts_with(" = ") => {
                return Some((name, Some(&annotated[..at]), Some(&annotated[at + 3..])));
            }
            _ => {}
        }
    }
    Some((name, Some(annotated), None))
}

/// Fills in types the docstring leaves out, and defaults, from the
/// signature (`args` as in `PythonFunction::args`). Parameters the
/// docstring does not mention are not added.
pub fn merge_signature(sections: &mut DocSections, args: &[String]) {
    let signature: Vec<_> = args.iter().filter_map(|a| parse_parameter(a)).collect();
    for item in sections
        .params
        .iter_mut()
        .chain(sections.keyword_params.iter_mut())
        .chain(sections.other_params.iter_mut())
    {
        let Some(name) = item.name.as_deref() else {
            continue;
        };
        let name = name.trim_start_matches('*');
        if let Some((_, annotation, default)) = signature.iter().find(|(n, _, _)| *n == name) {
            if item.ty.is_none() {
                item.ty = annotation.map(str::to_string);
            }
            if item.default.is_none() {
                item.default = default.map(str::to_string);
            }
        }
    }
}

/// Fills in the type of a return value the docstring leaves untyped.
pub fn merge_return_type(sections: &mut DocSections, return_type: Option<&str>) {
    if let (Some(item), Some(ty)) = (sections.returns.first_mut(), return_type)
        && item.ty.is_none()
    {
        item.ty = Some(ty.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(doc: &str) -> Split {
        split(doc, DocstringStyle::Auto).unwrap_or_else(|| panic!("no sections in {doc:?}"))
    }

    fn item(name: Option<&str>, ty: Option<&str>, description: &str) -> DocItem {
        DocItem {
            name: name.map(str::to_string),
            ty: ty.map(str::to_string),
            description: description.to_string(),
            ..DocItem::default()
        }
    }

    #[test]
    fn google_sections() {
        let doc = "Summary line.\n\nMore text.\n\nArgs:\n    x (int): The x.\n        Continued.\n    y: The y.\n    *args: Extra.\n    **kwargs (Any, optional): Options.\n\nReturns:\n    ConsoleOptions: a copy of self.\n\nRaises:\n    ValueError: If bad.\n    KeyError: If missing.\n\nNote:\n    Be careful.\n\nExample:\n    >>> f(1)\n    2\n";
        let s = auto(doc);
        assert_eq!(s.body, "Summary line.\n\nMore text.");
        assert_eq!(s.sections.style, "google");
        assert_eq!(
            s.sections.params,
            [
                item(Some("x"), Some("int"), "The x.\nContinued."),
                item(Some("y"), None, "The y."),
                item(Some("*args"), None, "Extra."),
                item(Some("**kwargs"), Some("Any"), "Options."),
            ]
        );
        assert_eq!(
            s.sections.returns,
            [item(None, Some("ConsoleOptions"), "a copy of self.")]
        );
        assert_eq!(
            s.sections.raises,
            [
                item(None, Some("ValueError"), "If bad."),
                item(None, Some("KeyError"), "If missing.")
            ]
        );
        assert_eq!(s.sections.notes[0].text, "Be careful.");
        assert_eq!(s.sections.examples[0].text, ">>> f(1)\n2");
    }

    #[test]
    fn google_free_text_returns() {
        let s = auto(
            "Do it.\n\nReturns:\n    The number of things, or None\n    when there are none.\n",
        );
        assert_eq!(
            s.sections.returns,
            [item(
                None,
                None,
                "The number of things, or None\nwhen there are none."
            )]
        );
        let s = auto("Do it.\n\nReturns: True if it worked.\n");
        assert_eq!(s.sections.returns, [item(None, None, "True if it worked.")]);
        let s = auto("Do it.\n\nReturns:\n    dict[str, int] | None: The map.\n");
        assert_eq!(
            s.sections.returns,
            [item(None, Some("dict[str, int] | None"), "The map.")]
        );
    }

    #[test]
    fn numpy_sections() {
        let doc = "Summary.\n\nParameters\n----------\nx : int\n    The x.\ny : str, optional\n    The y.\n\n    Second paragraph.\nflag\n    No type.\n\nReturns\n-------\nint\n    The result.\n\nRaises\n------\nValueError\n    When bad.\n\nSee Also\n--------\nother : Another function.\n\nNotes\n-----\nSome notes.\n";
        let s = auto(doc);
        assert_eq!(s.body, "Summary.");
        assert_eq!(s.sections.style, "numpy");
        assert_eq!(
            s.sections.params,
            [
                item(Some("x"), Some("int"), "The x."),
                item(Some("y"), Some("str"), "The y.\n\nSecond paragraph."),
                item(Some("flag"), None, "No type."),
            ]
        );
        assert_eq!(s.sections.returns, [item(None, Some("int"), "The result.")]);
        assert_eq!(
            s.sections.raises,
            [item(None, Some("ValueError"), "When bad.")]
        );
        assert_eq!(s.sections.see_also[0].text, "other : Another function.");
        assert_eq!(s.sections.notes[0].text, "Some notes.");
    }

    #[test]
    fn numpy_named_returns() {
        let s = auto(
            "S.\n\nReturns\n-------\ncount : int\n    How many.\nnames : list of str\n    Which.\n",
        );
        assert_eq!(
            s.sections.returns,
            [
                item(Some("count"), Some("int"), "How many."),
                item(Some("names"), Some("list of str"), "Which.")
            ]
        );
    }

    #[test]
    fn numpy_sections_end_at_unknown_headers() {
        let doc = "S.\n\nAttributes\n----------\nfilename : str\n    Path.\n\nMethods\n-------\nflush\n    Writes.\n\nReturns\n-------\nFunction suitable for decorating.\n\nRaises\n------\nValueError: shape mismatch\n    When shapes differ.\n";
        let s = auto(doc);
        assert_eq!(
            s.sections.attributes,
            [item(Some("filename"), Some("str"), "Path.")]
        );
        assert!(s.body.contains("Methods\n-------\nflush\n    Writes."));
        assert_eq!(
            s.sections.returns,
            [item(None, None, "Function suitable for decorating.")]
        );
        assert_eq!(
            s.sections.raises,
            [item(
                None,
                Some("ValueError"),
                "shape mismatch\nWhen shapes differ."
            )]
        );
    }

    #[test]
    fn rest_fields() {
        let doc = "Summary.\n\n:param x: The x.\n    Continued.\n:type x: int\n:param str y: The y.\n:keyword z: A keyword.\n:raises ValueError: If bad.\n:returns: The result.\n:rtype: bool\n:meta private:\n\n.. note:: Careful.\n\n.. warning::\n   Dangerous.\n";
        let s = auto(doc);
        assert_eq!(s.sections.style, "rest");
        assert_eq!(s.body, "Summary.\n\n:meta private:");
        assert_eq!(
            s.sections.params,
            [
                item(Some("x"), Some("int"), "The x.\nContinued."),
                item(Some("y"), Some("str"), "The y.")
            ]
        );
        assert_eq!(
            s.sections.keyword_params,
            [item(Some("z"), None, "A keyword.")]
        );
        assert_eq!(
            s.sections.raises,
            [item(None, Some("ValueError"), "If bad.")]
        );
        assert_eq!(
            s.sections.returns,
            [item(None, Some("bool"), "The result.")]
        );
        assert_eq!(s.sections.notes[0].text, "Careful.");
        assert_eq!(s.sections.warnings[0].text, "Dangerous.");
    }

    #[test]
    fn plain_docstrings_have_no_sections() {
        for doc in [
            "Just text.",
            "Text.\n\nArgs are explained elsewhere.",
            "Note: the line above is not a section header here\nbecause the next line is not indented? It is free text.",
            "Returns:",
            "Args:\nx: not indented, so not a section",
            "```\nArgs:\n    x: in a code block?\n```",
        ] {
            if let Some(s) = split(doc, DocstringStyle::Auto) {
                // Only free-text sections may be found in such text.
                assert!(s.sections.params.is_empty(), "{doc:?} -> {s:?}");
            }
        }
        assert_eq!(split("Args:\n    x: y", DocstringStyle::Markdown), None);
    }

    #[test]
    fn malformed_sections_stay_in_the_body() {
        // An Args entry that is not `name: text` keeps the whole section as text.
        let doc =
            "S.\n\nArgs:\n    this is not a parameter list\n    x: fine\n\nReturns:\n    int: ok\n";
        let s = auto(doc);
        assert!(
            s.body
                .contains("Args:\n    this is not a parameter list\n    x: fine")
        );
        assert_eq!(s.sections.params, []);
        assert_eq!(s.sections.returns, [item(None, Some("int"), "ok")]);

        // A NumPy section whose entries start indented.
        let doc = "S.\n\nParameters\n----------\n    x : int\n  y : int\n";
        if let Some(s) = split(doc, DocstringStyle::Auto) {
            assert!(s.body.contains("x : int"));
        }
    }

    #[test]
    fn forced_styles_only_look_for_their_own_sections() {
        let doc = "S.\n\nArgs:\n    x: The x.\n\n:param y: The y.\n";
        let google = split(doc, DocstringStyle::Google).unwrap();
        assert_eq!(google.sections.params.len(), 1);
        assert!(google.body.contains(":param y: The y."));
        let rest = split(doc, DocstringStyle::Rest).unwrap();
        assert_eq!(rest.sections.params[0].name.as_deref(), Some("y"));
        assert!(rest.body.contains("Args:\n    x: The x."));
        assert_eq!(split(doc, DocstringStyle::Numpy), None);
    }

    #[test]
    fn mixed_styles_are_all_taken_out() {
        let doc = "S.\n\nArgs:\n    a: First.\n    b: Second.\n\nReturns:\n    int: Sum.\n\n:raises ValueError: If bad.\n:meta private:\n";
        let s = auto(doc);
        // Google has more sections, so it names the style.
        assert_eq!(s.sections.style, "google");
        assert_eq!(s.sections.params.len(), 2);
        assert_eq!(
            s.sections.raises,
            [item(None, Some("ValueError"), "If bad.")]
        );
        let body: Vec<&str> = s.body.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(body, ["S.", ":meta private:"]);
    }

    #[test]
    fn signature_fills_types_and_defaults() {
        let mut s = auto("S.\n\nArgs:\n    a: A.\n    b (str): B.\n    *args: More.\n    ghost: Not in the signature.\n").sections;
        let args: Vec<String> = [
            "self",
            "a: int = 3",
            "b: Optional[str] = None",
            "c=1",
            "*args: Any",
            "/",
            "d: Callable[[int], str] = f(x = 1)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        merge_signature(&mut s, &args);
        let got: Vec<_> = s
            .params
            .iter()
            .map(|p| (p.name.clone().unwrap(), p.ty.clone(), p.default.clone()))
            .collect();
        assert_eq!(
            got,
            [
                ("a".into(), Some("int".into()), Some("3".into())),
                ("b".into(), Some("str".into()), Some("None".into())),
                ("*args".into(), Some("Any".into()), None),
                ("ghost".into(), None, None),
            ]
        );
        assert_eq!(
            parse_parameter("d: Callable[[int], str] = f(x = 1)"),
            Some(("d", Some("Callable[[int], str]"), Some("f(x = 1)")))
        );
        assert_eq!(parse_parameter("*"), None);
    }

    /// Every word of the input must come out somewhere: in the body or in a
    /// section field. Section titles and underlines are the only text that
    /// may disappear.
    fn assert_keeps_text(doc: &str, style: DocstringStyle) {
        let Some(s) = split(doc, style) else {
            return;
        };
        let mut out = s.body.clone();
        for (_, items) in s.sections.items() {
            for item in items {
                for part in [&item.name, &item.ty, &item.default].into_iter().flatten() {
                    out.push(' ');
                    out.push_str(part);
                }
                out.push(' ');
                out.push_str(&item.description);
            }
        }
        for (_, blocks) in s.sections.blocks() {
            for block in blocks {
                out.push(' ');
                out.push_str(&block.text);
            }
        }
        let words = |text: &str| -> Vec<String> {
            text.split(|c: char| c.is_whitespace() || "():,".contains(c))
                .filter(|w| !w.is_empty() && !w.chars().all(|c| c == '-'))
                .map(str::to_string)
                .collect()
        };
        let have = words(&out);
        let titles = [
            "Args",
            "Arguments",
            "Parameters",
            "Params",
            "Returns",
            "Return",
            "Yields",
            "Raises",
            "Attributes",
            "Examples",
            "Example",
            "Notes",
            "Note",
            "Warnings",
            "Warning",
            "See",
            "Also",
            "Keyword",
            "Other",
            "param",
            "type",
            "returns",
            "rtype",
            "raises",
            "keyword",
            "ivar",
            "note",
            "warning",
            "..",
            "optional",
        ];
        for word in words(doc) {
            assert!(
                titles.contains(&word.as_str()) || have.contains(&word),
                "{word:?} was dropped from {doc:?} ({style:?}):\n{s:#?}"
            );
        }
    }

    #[test]
    fn fuzz_never_drops_text() {
        let pieces = [
            "Summary of the thing.",
            "",
            "Args:",
            "    alpha (int): First value.",
            "    beta: Second\tvalue.",
            "        continued beta text.",
            "\tgamma (list[str], optional): Tabbed.",
            "  odd indentation here",
            "Returns:",
            "    int: The result.",
            "    plain returned words",
            "Raises:",
            "    ValueError: bad input.",
            "    not an exception line",
            "Parameters",
            "----------",
            "delta : float",
            "    Delta text.",
            "epsilon",
            "Returns",
            "-------",
            "zeta : int",
            ":param eta: Eta words.",
            ":type eta: int",
            ":param: broken field",
            ":rtype:",
            ":raises: anything",
            ".. note:: noted words",
            ".. warning::",
            "   warned words",
            "Note:",
            "    noteworthy words",
            "Examples:",
            "    >>> example()",
            "See Also:",
            "Args:",
            "x: unindented under a header",
            "```",
            "    code: in a fence",
        ];
        // A small deterministic generator, so failures are reproducible.
        let mut state: u64 = 0x2545F4914F6CDD1D;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..3000 {
            let count = 1 + (next() % 14) as usize;
            let doc: Vec<&str> = (0..count)
                .map(|_| pieces[(next() % pieces.len() as u64) as usize])
                .collect();
            let doc = doc.join("\n");
            for style in [
                DocstringStyle::Auto,
                DocstringStyle::Google,
                DocstringStyle::Numpy,
                DocstringStyle::Rest,
            ] {
                assert_keeps_text(&doc, style);
            }
        }
    }
}
