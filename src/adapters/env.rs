//! Environment variables, and `.env` files, as HOCON config.
//!
//! This is the bulk-mount case: a whole prefixed namespace becomes a config
//! subtree. Reading one variable needs nothing from here — HOCON's own
//! `${?VAR}` already does that.
//!
//! # How names become paths
//!
//! `__` is the only thing that creates hierarchy. A single `_` stays part of
//! the segment, and a literal `.` is key *text*, not a separator (spec F1.2):
//!
//! ```text
//! APP_DB__MAX_CONN=10   ->  db.max_conn    (nested)
//! APP_FOO.BAR=flat      ->  "foo.bar"      (one key that contains a dot)
//! ```
//!
//! The two spellings are distinct paths, so they coexist: the first is read as
//! `cfg.get_string("foo.bar")`, the second needs the quoted path
//! `cfg.get_string("\"foo.bar\"")`. Segments are lowercased after mapping
//! (F1.3).
//!
//! Two names that really do map to one path (`APP_A__B` and `APP_a__b`) are an
//! error, not a last-wins, because environment iteration order is not
//! deterministic (F1.6). A `.env` file has a definite line order, so there the
//! later line wins (F0.7).
//!
//! # Non-UTF-8 entries
//!
//! [`load`] **errors** when an entry matching the mount prefix has a name or
//! value that is not valid UTF-8 (spec F1.9b). A bulk mount is a request for a
//! whole namespace, so omitting one key would produce a subtree that looks
//! complete while the operator's setting is missing, and a stale config
//! default would then win invisibly. It would also defeat F1.6: if the
//! undecodable entry were dropped, a colliding name would silently win, making
//! the surviving value depend on an encoding property of the *other* value —
//! precisely the nondeterminism F1.6 exists to forbid.
//!
//! Entries that do not match the prefix are ignored regardless, so an
//! undecodable variable elsewhere in the environment never fails a mount. That
//! is the opposite of the `${VAR}` path, where an undecodable entry is simply
//! absent (F1.9a) because `${?VAR}` explicitly means "optional".

use std::collections::HashMap;

use indexmap::IndexMap;

use super::{config_from_object, AdapterError};
use crate::depth::MAX_PATH_SEGMENTS;
use crate::value::{HoconValue, ScalarValue};
use crate::Config;

/// The double underscore that marks a path boundary; a single one stays part
/// of the segment, so `APP_DB__MAX_CONN` is `db.max_conn` (spec F1.2). Fixed
/// rather than configurable so every language's adapter nests identically.
const SEPARATOR: &str = "__";

/// How variable names become config paths.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Selects which variables to mount, and is stripped from the path.
    /// Required by [`load`]: mounting the whole environment would pull in
    /// PATH, HOME and whatever secrets happen to be set (spec F1.1).
    /// [`parse_dotenv`] allows it to be empty, a `.env` file being a closed
    /// set the caller chose deliberately.
    pub prefix: String,
    /// Source name for error messages.
    pub origin: Option<String>,
}

/// Mount a prefixed slice of the process environment.
///
/// An entry that matches the prefix but whose name or value is not valid UTF-8
/// is an **error** (spec F1.9b). A bulk mount is the caller asking for a whole
/// namespace, so dropping one key would hand back a subtree that looks
/// complete while the operator's setting is missing — and a stale config
/// default would then win with no signal anywhere. Entries that do *not* match
/// the prefix are ignored whether they decode or not, which bounds this to
/// variables the caller named: an unrelated undecodable entry elsewhere in the
/// environment can never fail the mount.
pub fn load(opts: Options) -> Result<Config, AdapterError> {
    if opts.prefix.is_empty() {
        return Err(AdapterError::new(
            "env: a prefix is required when mounting the environment (spec F1.1)",
        ));
    }
    // Filter while iterating rather than collecting the whole environment
    // first: everything else is never used, and some of it is secret. The
    // prefix test runs on the raw bytes so an undecodable *name* can still be
    // matched against the (ASCII) prefix — ASCII bytes survive
    // `as_encoded_bytes` unchanged on every platform.
    let mut vars: HashMap<String, String> = HashMap::new();
    for e in crate::sysenv::entries() {
        if !e.name_bytes.starts_with(opts.prefix.as_bytes()) {
            continue;
        }
        let undecodable = match (&e.name, &e.value) {
            (None, _) => "name",
            (Some(_), None) => "value",
            (Some(name), Some(value)) => {
                vars.insert(name.clone(), value.clone());
                continue;
            }
        };
        return Err(AdapterError::new(format!(
            "env: {} matches the mount prefix {:?} but its {undecodable} is not valid UTF-8; \
             a bulk mount cannot silently omit it (spec F1.9)",
            e.display_name(),
            opts.prefix,
        )));
    }
    load_from(&vars, opts)
}

/// Mount a prefixed slice of the supplied variables. Used by [`load`], and by
/// tests that must not touch the real environment.
pub fn load_from(vars: &HashMap<String, String>, opts: Options) -> Result<Config, AdapterError> {
    if opts.prefix.is_empty() {
        return Err(AdapterError::new(
            "env: a prefix is required when mounting the environment (spec F1.1)",
        ));
    }

    // Sorted so a collision is reported the same way on every run.
    let mut names: Vec<&String> = vars.keys().collect();
    names.sort();

    // Keyed by the segment list itself, so only identical segment lists can
    // collide — never two distinct paths that happen to share a rendering.
    let mut seen: HashMap<Vec<String>, &str> = HashMap::new();
    let mut pairs: Vec<(Vec<String>, String)> = Vec::new();
    for name in names {
        let Some(rest) = name.strip_prefix(&opts.prefix) else {
            continue;
        };
        let path = to_path(rest, name)?;
        if let Some(prev) = seen.get(&path) {
            // F1.6: two names can reach one path and the environment has no
            // meaningful order to break the tie with, so neither wins.
            return Err(AdapterError::new(format!(
                "env: {prev} and {name} both map to {}",
                display_path(&path)
            )));
        }
        seen.insert(path.clone(), name);
        pairs.push((path, vars[name].clone()));
    }

    let origin = opts.origin.as_deref().unwrap_or("environment variables");
    Ok(config_from_object(nest(pairs), Some(origin)))
}

/// Read `.env` file content.
///
/// The dialect is deliberately small (spec F1.7): `NAME=value`, an optional
/// `export ` prefix, whole-line `#` comments, single quotes taken literally,
/// double quotes with `\n \r \t \\ \"`. Multi-line values and trailing
/// comments are unsupported — an unquoted value containing ` #` is an error
/// rather than a guess. No `${...}` expansion.
pub fn parse_dotenv(input: &str, opts: Options) -> Result<Config, AdapterError> {
    let origin = opts.origin.clone().unwrap_or_else(|| ".env".to_string());
    let mut pairs: Vec<(Vec<String>, String)> = Vec::new();

    let normalized = super::strip_bom(input)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    for (i, raw) in normalized.split('\n').enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((name, rest)) = line.split_once('=') else {
            return Err(AdapterError::new(format!(
                "{origin}:{}: expected NAME=value",
                i + 1
            )));
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(AdapterError::new(format!(
                "{origin}:{}: empty variable name",
                i + 1
            )));
        }
        let value = dotenv_value(rest.trim_start_matches([' ', '\t']), &origin, i + 1, name)?;
        let Some(stripped) = name.strip_prefix(&opts.prefix) else {
            continue;
        };
        pairs.push((to_path(stripped, name)?, value));
    }

    // A file has a definite line order, so a repeated name is last-wins (F0.7)
    // rather than the collision error the environment gets.
    Ok(config_from_object(nest(pairs), Some(&origin)))
}

/// Strip the prefix, split on `__`, lowercase each segment (F1.2, F1.3).
///
/// The path stays a **segment list** from here to the built tree: joining on
/// `.` and re-splitting later would turn a literal `.` inside a variable name
/// into a manufactured boundary, when F1.2 says only `__` creates hierarchy.
/// `APP_FOO.BAR` is the single top-level key `"foo.bar"`, distinct from
/// `APP_FOO__BAR`'s `foo` → `bar`.
fn to_path(rest: &str, name: &str) -> Result<Vec<String>, AdapterError> {
    // ASCII-only folding (F1.3). Rust's `to_lowercase` applies the full
    // Unicode mapping, which turns `İ` (U+0130) into `i` + U+0307 while Go's
    // simple mapping yields plain `i` — that difference decides whether
    // `APP_İ` collides with `APP_I`, so the mapping is pinned here rather than
    // inherited from the stdlib. Environment names are ASCII in practice.
    let segs: Vec<String> = rest
        .split(SEPARATOR)
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if segs.iter().any(|s| s.is_empty()) {
        return Err(AdapterError::new(format!(
            "env: \"{name}\" produces an empty path segment"
        )));
    }
    // Depth cap. `set_nested` recurses once per segment and the resulting tree
    // drops recursively, so an unbounded segment count overflows the stack —
    // which aborts the process rather than raising a catchable panic. Linux
    // allows a 128 KiB environment entry and `parse_dotenv` takes arbitrary
    // file text, so this is reachable from input. A path this deep is a
    // mistake in every real config, and an error says so.
    if segs.len() > MAX_PATH_SEGMENTS {
        return Err(AdapterError::new(format!(
            "env: \"{name}\" maps to a path {} segments deep, over the limit of {MAX_PATH_SEGMENTS}",
            segs.len()
        )));
    }
    Ok(segs)
}

/// Render a segment list as a HOCON path expression for an error message.
///
/// A segment is written bare when it is safely unquoted (`[a-z0-9_-]`, the
/// shape F1.3 lowercasing produces) and double-quoted otherwise, with `\` and
/// `"` escaped so two different paths can never render identically. Matches
/// py.hocon's format so the four implementations report collisions alike.
fn display_path(segments: &[String]) -> String {
    segments
        .iter()
        .map(|s| render_segment(s))
        .collect::<Vec<_>>()
        .join(".")
}

/// Render one path segment as a HOCON path expression element (spec F0.10):
/// bare when it can be, otherwise as a **JSON string literal** — which is also
/// HOCON's own quoted-string syntax, and which go.hocon and py.hocon produce
/// for the same segment.
///
/// The escaping used to cover only `\` and `"`, so a NUL or a newline in a
/// variable name went into the error message raw and could break the line a
/// reader saw in a log. Two departures from plain JSON, both deliberate:
/// U+2028 and U+2029 are escaped although JSON permits them raw, being line
/// separators to enough log viewers to matter; and printable non-ASCII stays
/// itself, because F1.3 leaves such segments unfolded and escaping them would
/// bury the common case.
///
/// The result is **not** guaranteed to paste into a getter: no implementation's
/// path parser decodes escapes inside a quoted segment. What it does guarantee,
/// and what an error message needs, is that two different paths never render
/// alike.
fn render_segment(seg: &str) -> String {
    let bare = !seg.is_empty()
        && seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        return seg.to_string();
    }
    let mut out = String::with_capacity(seg.len() + 2);
    out.push('"');
    for c in seg.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' | '\u{2029}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Nest segment-list paths, applying the objects-win rule over the whole set
/// so the outcome does not depend on input order (spec F1.8, mirroring F2.5).
/// The sort is stable, so equal paths keep their line order and F0.7
/// last-wins still holds for `.env` files.
fn nest(mut pairs: Vec<(Vec<String>, String)>) -> HoconValue {
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut root: IndexMap<String, HoconValue> = IndexMap::new();
    for (path, value) in pairs {
        set_nested(
            &mut root,
            &path,
            HoconValue::Scalar(ScalarValue::string(value)),
        );
    }
    HoconValue::Object(root)
}

fn set_nested(map: &mut IndexMap<String, HoconValue>, segments: &[String], value: HoconValue) {
    if segments.is_empty() {
        return;
    }
    if segments.len() == 1 {
        if !matches!(map.get(&segments[0]), Some(HoconValue::Object(_))) {
            map.insert(segments[0].clone(), value);
        }
        return;
    }
    let entry = map
        .entry(segments[0].clone())
        .or_insert_with(|| HoconValue::Object(IndexMap::new()));
    if !matches!(entry, HoconValue::Object(_)) {
        *entry = HoconValue::Object(IndexMap::new());
    }
    if let HoconValue::Object(inner) = entry {
        set_nested(inner, &segments[1..], value);
    }
}

fn dotenv_value(v: &str, origin: &str, line: usize, name: &str) -> Result<String, AdapterError> {
    let fail = |msg: &str| AdapterError::new(format!("{origin}:{line}: {name}: {msg}"));

    if let Some(rest) = v.strip_prefix('\'') {
        let Some(end) = rest.find('\'') else {
            return Err(fail(
                "unterminated ' quote (multi-line values are not supported)",
            ));
        };
        if !rest[end + 1..].trim().is_empty() {
            return Err(fail("unexpected text after the closing quote"));
        }
        return Ok(rest[..end].to_string());
    }

    if let Some(rest) = v.strip_prefix('"') {
        let mut out = String::new();
        let chars: Vec<char> = rest.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            match chars[i] {
                '"' => {
                    let tail: String = chars[i + 1..].iter().collect();
                    if !tail.trim().is_empty() {
                        return Err(fail("unexpected text after the closing quote"));
                    }
                    return Ok(out);
                }
                '\\' => {
                    i += 1;
                    match chars.get(i) {
                        Some('n') => out.push('\n'),
                        Some('r') => out.push('\r'),
                        Some('t') => out.push('\t'),
                        Some('\\') => out.push('\\'),
                        Some('"') => out.push('"'),
                        Some(other) => {
                            return Err(fail(&format!(
                                "unknown escape \\{other} (supported: \\n \\r \\t \\\\ \\\")"
                            )))
                        }
                        None => return Err(fail("dangling \\ at end of line")),
                    }
                }
                c => out.push(c),
            }
            i += 1;
        }
        return Err(fail(
            "unterminated \" quote (multi-line values are not supported)",
        ));
    }

    let trimmed = v.trim_end_matches([' ', '\t']);
    let bytes = trimmed.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i] == b'#' && (bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            return Err(fail(&format!(
                "ambiguous value \"{trimmed}\": trailing comments are not supported, so quote the value if the # belongs to it"
            )));
        }
    }
    Ok(trimmed.to_string())
}
