// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! HOCON emitter (xx.hocon E18): [`Config::render_hocon`].
//!
//! Port of go.hocon's `RenderHOCON` (v1.11.0), lockstep with the ts.hocon
//! (`renderHocon`) and py.hocon (`render_hocon`) ports. The correctness
//! contract is the round trip — `parse(render(tree))` yields the same value
//! tree — not byte-for-byte formatting.

use crate::config::Config;
use crate::error::ConfigError;
use crate::value::{HoconValue, ScalarType, ScalarValue};
use indexmap::IndexMap;

impl Config {
    /// Render a resolved `Config` as HOCON text.
    ///
    /// The output round-trips: parsing it back yields the same value tree.
    /// That is the correctness contract, not byte-for-byte formatting — a
    /// scalar is quoted whenever leaving it bare would re-parse as a different
    /// type (a string `"8080"` becomes `"8080"`, not `8080`), and left bare
    /// only when it provably cannot.
    ///
    /// The `Config` must be resolved and hold only data (objects, arrays,
    /// string / number / boolean / null scalars) — exactly what `from_map` and
    /// the format adapters produce. An unresolved placeholder is an error;
    /// substitutions have no textual round trip through a value tree.
    ///
    /// The root object's fields are emitted without enclosing braces, nested
    /// objects as `key { … }`, arrays as newline-separated `[ … ]`, indented
    /// two spaces. Source comments are not represented — a value tree does not
    /// carry them.
    ///
    /// Cross-impl convention xx.hocon E18; lockstep with go.hocon
    /// `RenderHOCON` (v1.11.0), ts.hocon `renderHocon`, and py.hocon
    /// `render_hocon`.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when the tree contains an unresolved
    /// substitution placeholder (call [`Config::resolve`] first).
    pub fn render_hocon(&self) -> Result<String, ConfigError> {
        let mut out = String::new();
        render_object_body(&mut out, &self.root, 0)?;
        Ok(out)
    }
}

fn render_object_body(
    out: &mut String,
    fields: &IndexMap<String, HoconValue>,
    depth: usize,
) -> Result<(), ConfigError> {
    let indent = "  ".repeat(depth);
    for (k, v) in fields {
        out.push_str(&indent);
        out.push_str(&render_key(k));
        match v {
            HoconValue::Object(child) => {
                out.push_str(" {");
                if child.is_empty() {
                    out.push_str("}\n");
                    continue;
                }
                out.push('\n');
                render_object_body(out, child, depth + 1)?;
                out.push_str(&indent);
                out.push_str("}\n");
            }
            _ => {
                out.push_str(" = ");
                render_value(out, v, depth)?;
                out.push('\n');
            }
        }
    }
    Ok(())
}

fn render_value(out: &mut String, v: &HoconValue, depth: usize) -> Result<(), ConfigError> {
    match v {
        HoconValue::Object(fields) => {
            if fields.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push_str("{\n");
            render_object_body(out, fields, depth + 1)?;
            out.push_str(&"  ".repeat(depth));
            out.push('}');
            Ok(())
        }
        HoconValue::Array(items) => render_array(out, items, depth),
        HoconValue::Scalar(s) => {
            out.push_str(&render_scalar(s));
            Ok(())
        }
        HoconValue::Placeholder(p) => Err(ConfigError {
            message: format!(
                "render_hocon: unrenderable value ${{{}{}}} (config must be resolved data)",
                if p.optional { "?" } else { "" },
                p.path
            ),
            path: String::new(),
        }),
    }
}

fn render_array(out: &mut String, items: &[HoconValue], depth: usize) -> Result<(), ConfigError> {
    if items.is_empty() {
        out.push_str("[]");
        return Ok(());
    }
    let inner = "  ".repeat(depth + 1);
    out.push_str("[\n");
    for e in items {
        out.push_str(&inner);
        render_value(out, e, depth + 1)?;
        out.push('\n');
    }
    out.push_str(&"  ".repeat(depth));
    out.push(']');
    Ok(())
}

fn render_scalar(s: &ScalarValue) -> String {
    match s.value_type {
        ScalarType::Null => "null".to_string(),
        // `raw` already holds the canonical textual form; both re-parse to
        // their own type, so they are emitted bare.
        ScalarType::Boolean | ScalarType::Number => s.raw.clone(),
        ScalarType::String => render_string(&s.raw),
    }
}

/// A key is unambiguous unquoted iff it matches `^[A-Za-z0-9_-]+$`: no dot
/// (which would nest), no whitespace, no forbidden character.
fn is_safe_unquoted_key(k: &str) -> bool {
    !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn render_key(k: &str) -> String {
    if is_safe_unquoted_key(k) {
        k.to_string()
    } else {
        quote_string(k)
    }
}

/// A string value cannot be misread as another type iff it matches
/// `^[A-Za-z][A-Za-z0-9_-]*$`: an identifier that is not numeric. Boolean /
/// null keywords are checked separately. Any other string is quoted, which
/// always round-trips.
fn is_safe_bare_string(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

const STRING_KEYWORDS: [&str; 7] = ["true", "false", "null", "yes", "no", "on", "off"];

fn is_string_keyword(s: &str) -> bool {
    STRING_KEYWORDS.iter().any(|k| s.eq_ignore_ascii_case(k))
}

fn render_string(s: &str) -> String {
    if is_safe_bare_string(s) && !is_string_keyword(s) {
        s.to_string()
    } else {
        quote_string(s)
    }
}

fn quote_string(s: &str) -> String {
    // A string containing newlines is triple-quoted when that is unambiguous
    // and lossless: no embedded `"""`, no trailing `"`, and no carriage return
    // (the parser normalizes CRLF inside triple quotes, which would drop the
    // `\r`, so those fall through to escaped double quotes below).
    if s.contains('\n') && !s.contains('\r') && !s.contains("\"\"\"") && !s.ends_with('"') {
        return format!("\"\"\"{s}\"\"\"");
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
