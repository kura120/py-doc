//! A small build-time syntax highlighter for Python code blocks.
//!
//! It is a tokenizer, not a parser: enough to colour keywords, strings,
//! comments, numbers, decorators and definition names without pulling a
//! full highlighting library into the binary.

const KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "case", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "match", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield",
];

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
}

fn push_token(out: &mut String, class: &str, text: &str) {
    out.push_str("<span class=\"tok-");
    out.push_str(class);
    out.push_str("\">");
    push_escaped(out, text);
    out.push_str("</span>");
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// If a string literal starts at `i` (optionally with an r/b/u/f prefix),
/// returns the index one past its end.
fn string_end(chars: &[char], i: usize) -> Option<usize> {
    let mut quote_at = i;
    while quote_at < chars.len() && quote_at < i + 2 && "rRbBuUfF".contains(chars[quote_at]) {
        quote_at += 1;
    }
    let quote = *chars.get(quote_at)?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let triple = chars.get(quote_at + 1) == Some(&quote) && chars.get(quote_at + 2) == Some(&quote);
    let mut j = quote_at + if triple { 3 } else { 1 };
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            c if c == quote => {
                if !triple {
                    return Some(j + 1);
                }
                if chars.get(j + 1) == Some(&quote) && chars.get(j + 2) == Some(&quote) {
                    return Some(j + 3);
                }
                j += 1;
            }
            // An unterminated single-quoted string stops at the end of its line.
            '\n' if !triple => return Some(j),
            _ => j += 1,
        }
    }
    Some(chars.len())
}

/// Returns HTML for a Python snippet, with tokens wrapped in `tok-*` spans.
pub fn highlight_python(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    let mut out = String::with_capacity(code.len() * 2);
    let mut i = 0;
    // Set after `def` / `class`, so the next identifier is styled as a definition.
    let mut expect_name = false;
    let mut at_line_start = true;

    while i < chars.len() {
        let c = chars[i];
        let text =
            |from: usize, to: usize| chars[from..to.min(chars.len())].iter().collect::<String>();

        if c == '#' {
            let end = chars[i..]
                .iter()
                .position(|&ch| ch == '\n')
                .map_or(chars.len(), |p| i + p);
            push_token(&mut out, "com", &text(i, end));
            i = end;
        } else if let Some(end) = string_end(&chars, i) {
            push_token(&mut out, "str", &text(i, end));
            i = end.min(chars.len());
            expect_name = false;
        } else if c.is_ascii_digit() {
            let mut end = i;
            while end < chars.len() && (is_ident_char(chars[end]) || chars[end] == '.') {
                end += 1;
            }
            push_token(&mut out, "num", &text(i, end));
            i = end;
        } else if is_ident_start(c) {
            let mut end = i;
            while end < chars.len() && is_ident_char(chars[end]) {
                end += 1;
            }
            let word = text(i, end);
            if KEYWORDS.contains(&word.as_str()) {
                push_token(&mut out, "kw", &word);
                expect_name = word == "def" || word == "class";
            } else if expect_name {
                push_token(&mut out, "fn", &word);
                expect_name = false;
            } else {
                push_escaped(&mut out, &word);
            }
            i = end;
        } else if c == '@' && at_line_start {
            let mut end = i + 1;
            while end < chars.len() && (is_ident_char(chars[end]) || chars[end] == '.') {
                end += 1;
            }
            push_token(&mut out, "dec", &text(i, end));
            i = end;
        } else {
            push_escaped(&mut out, &c.to_string());
            i += 1;
        }

        if c == '\n' {
            at_line_start = true;
        } else if !c.is_whitespace() {
            at_line_start = false;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_names_strings_and_comments_are_tagged() {
        let html = highlight_python("def run(x):  # go\n    return f\"<{x}>\"\n");
        assert!(html.contains("<span class=\"tok-kw\">def</span>"));
        assert!(html.contains("<span class=\"tok-fn\">run</span>"));
        assert!(html.contains("<span class=\"tok-com\"># go</span>"));
        assert!(html.contains("<span class=\"tok-str\">f&quot;&lt;{x}&gt;&quot;</span>"));
    }

    #[test]
    fn identifiers_starting_with_a_prefix_letter_are_not_strings() {
        let html = highlight_python("result = bar\n");
        assert!(!html.contains("tok-str"));
    }

    #[test]
    fn decorators_and_triple_quotes() {
        let html = highlight_python("@app.route\nx = \"\"\"a\nb\"\"\"\n");
        assert!(html.contains("<span class=\"tok-dec\">@app.route</span>"));
        assert!(
            html.contains(
                "<span class=\"tok-str\">&quot;&quot;&quot;a\nb&quot;&quot;&quot;</span>"
            )
        );
    }

    #[test]
    fn unterminated_input_does_not_panic() {
        highlight_python("x = 'oops");
        highlight_python("x = 'oops\\");
        highlight_python("\"\"\"never closed");
    }
}
