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
    let cleaned = strip_trailing_commas(&strip_comments(input)?);
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
/// alone. A comment becomes whitespace, never the empty string: erasing it
/// outright would splice its neighbors into one token (`1/*x*/2` → `12`,
/// which is valid JSON), so each block comment leaves at least one space
/// behind (spec F3.2). Newlines inside removed spans are kept so the JSON
/// parser still reports useful positions; a `//` comment keeps its
/// terminating newline the same way, which already separates tokens.
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
                while i < b.len() && b[i] != '\n' {
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
                    if b[j] == '\n' {
                        out.push('\n');
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
