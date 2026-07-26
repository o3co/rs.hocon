//! JSON with comments and trailing commas — the dialect VS Code and TypeScript
//! use for their config files — as HOCON config.
//!
//! Plain JSON needs no adapter: HOCON is a JSON superset, so `hocon::parse`
//! already accepts a `.json` file. This exists for the two things HOCON does
//! not accept, block comments and trailing commas.
//!
//! Comments are **token-separating** (spec F3.2): a comment is replaced by
//! whitespace, never deleted, so it can never splice its neighbors into one
//! token. `{"a": 1/*x*/2}` is a syntax error rather than the number `12`.

use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use super::{config_from_object, AdapterError};
use crate::value::{HoconValue, ScalarValue};
use crate::Config;

/// Read JSONC text.
pub fn parse(input: &str, origin: Option<&str>) -> Result<Config, AdapterError> {
    let cleaned = strip_trailing_commas(&strip_comments(super::strip_bom(input))?);
    let doc: JsonValue =
        serde_json::from_str(&cleaned).map_err(|e| AdapterError::new(format!("jsonc: {e}")))?;
    if !doc.is_object() {
        return Err(AdapterError::new(
            "jsonc: document root must be an object (spec F0.3)",
        ));
    }
    Ok(config_from_object(convert(&doc, "")?, origin))
}

/// Read a JSONC file, using its path as the origin description.
pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Config, AdapterError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::new(format!("jsonc: {}: {e}", path.display())))?;
    parse(&text, Some(&path.display().to_string()))
}

fn convert(v: &JsonValue, at: &str) -> Result<HoconValue, AdapterError> {
    match v {
        JsonValue::Object(m) => {
            let mut out: IndexMap<String, HoconValue> = IndexMap::new();
            for (k, e) in m {
                let path = if at.is_empty() {
                    k.clone()
                } else {
                    format!("{at}.{k}")
                };
                out.insert(k.clone(), convert(e, &path)?);
            }
            Ok(HoconValue::Object(out))
        }
        JsonValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, e) in items.iter().enumerate() {
                out.push(convert(e, &format!("{at}[{i}]"))?);
            }
            Ok(HoconValue::Array(out))
        }
        JsonValue::String(s) => Ok(HoconValue::Scalar(ScalarValue::string(s.clone()))),
        JsonValue::Bool(b) => Ok(HoconValue::Scalar(ScalarValue::boolean(*b))),
        JsonValue::Null => Ok(HoconValue::Scalar(ScalarValue::null())),
        // serde_json keeps the integer/float distinction, so F0.5's rule holds
        // without re-reading the source text.
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(HoconValue::Scalar(ScalarValue::number(i.to_string())))
            } else if let Some(f) = n.as_f64() {
                if f.is_nan() || f.is_infinite() {
                    return Err(AdapterError::new(format!(
                        "jsonc: at {at}: {f} is not representable in HOCON (spec F0.6)"
                    )));
                }
                Ok(HoconValue::Scalar(ScalarValue::number(n.to_string())))
            } else {
                Err(AdapterError::new(format!(
                    "jsonc: at {at}: {n} does not fit in i64 (spec F0.5)"
                )))
            }
        }
    }
}

/// Remove `//` line comments and block comments, leaving string literals
/// alone.
///
/// Two invariants, both load-bearing:
///
/// 1. **A comment becomes whitespace, never the empty string.** Erasing it
///    outright would splice its neighbours into one token (`1/*x*/2` → `12`,
///    which is valid JSON), so each block comment leaves at least one space
///    behind (spec F3.2). A `//` comment keeps its terminator, which already
///    separates tokens.
/// 2. **Line structure is preserved exactly.** Every line terminator inside a
///    removed span is re-emitted verbatim and in order, so a `\r\n` survives
///    as a pair and a lone `\r` survives as itself. The stripped text
///    therefore has the same lines as the source and a decoder's reported
///    line numbers point at the right source line.
///
/// Column offsets within a line are deliberately *not* preserved — a comment
/// body collapses to a single space — so only line numbers are meaningful,
/// which is the same trade `//` stripping has always made.
fn strip_comments(src: &str) -> Result<String, AdapterError> {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '"' => {
                let end = end_of_string(&b, i)?;
                out.extend(&b[i..end]);
                i = end;
            }
            '/' if i + 1 < b.len() && b[i + 1] == '/' => {
                // A lone CR ends the comment too: a classic-Mac or otherwise
                // CR-delimited file would otherwise have the rest of the
                // document swallowed by the first `//` (spec F3.2). The
                // terminator itself is left in place, so it still separates
                // tokens.
                while i < b.len() && b[i] != '\n' && b[i] != '\r' {
                    i += 1;
                }
            }
            '/' if i + 1 < b.len() && b[i + 1] == '*' => {
                let mut j = i + 2;
                loop {
                    if j + 1 >= b.len() {
                        return Err(AdapterError::new("jsonc: unterminated block comment"));
                    }
                    if b[j] == '*' && b[j + 1] == '/' {
                        break;
                    }
                    // Re-emit terminators verbatim so a `\r\n` stays a pair
                    // and a lone `\r` is not silently dropped. Keeping only
                    // `\n` collapsed CRLF and lost classic-Mac line breaks
                    // outright, which contradicted the invariant above.
                    if b[j] == '\n' || b[j] == '\r' {
                        out.push(b[j]);
                    }
                    j += 1;
                }
                out.push(' ');
                i = j + 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

fn end_of_string(b: &[char], i: usize) -> Result<usize, AdapterError> {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            '\\' => j += 2,
            '"' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err(AdapterError::new("jsonc: unterminated string literal"))
}

/// Drop a comma whose next meaningful character closes its object or array.
fn strip_trailing_commas(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == '"' {
            match end_of_string(&b, i) {
                Ok(end) => {
                    out.extend(&b[i..end]);
                    i = end;
                    continue;
                }
                Err(_) => {
                    out.extend(&b[i..]);
                    return out;
                }
            }
        }
        if b[i] == ',' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            if j < b.len() && (b[j] == '}' || b[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sequence of line terminators, which is what "same line structure"
    /// means: a `\r\n` collapsed to `\n`, or a lone `\r` dropped, both show up
    /// here as a difference.
    fn terminators(s: &str) -> String {
        s.chars().filter(|c| *c == '\r' || *c == '\n').collect()
    }

    /// Both strip passes must preserve line structure exactly. Only the block
    /// comment branch was ever wrong, but pinning the whole pipeline is what
    /// makes that a property of the module rather than of one branch.
    #[test]
    fn stripping_preserves_line_structure() {
        for src in [
            // block comments, each line ending
            "{\n  /* a\n b */\n  \"k\": 1\n}",
            "{\r\n  /* a\r\n b */\r\n  \"k\": 1\r\n}",
            "{\r  /* a\r b */\r  \"k\": 1\r}",
            // a lone CR inside a comment on one line
            "{\"k\": /*a\rb*/ 1}",
            // mixed, and a CR immediately before a CRLF
            "{\"k\": /*a\r\r\nb*/ 1}",
            // line comments, whose terminator must survive as itself
            "{\n  // a\n  \"k\": 1\n}",
            "{\r\n  // a\r\n  \"k\": 1\r\n}",
            "{\r  // a\r  \"k\": 1\r}",
            // trailing commas removed next to line breaks
            "{\r\n  \"k\": 1,\r\n}",
            "[\r  1,\r]",
            // comment markers inside strings are data, not comments
            "{\"k\": \"a/*b*/c\",\r\n \"j\": 2}",
        ] {
            let stripped = strip_trailing_commas(&strip_comments(src).expect(src));
            assert_eq!(
                terminators(&stripped),
                terminators(src),
                "line structure changed for {src:?} -> {stripped:?}"
            );
        }
    }

    /// The token-separation invariant, at the same level as the one above.
    #[test]
    fn a_comment_never_joins_its_neighbours() {
        for src in ["1/*x*/2", "1/*\r*/2", "1/*\r\n*/2", "tr/*x*/ue"] {
            let stripped = strip_comments(src).expect(src);
            assert!(
                !stripped.contains("12") && !stripped.contains("true"),
                "{src:?} spliced into {stripped:?}"
            );
        }
    }
}
