//! Environment variables, and `.env` files, as HOCON config.
//!
//! This is the bulk-mount case: a whole prefixed namespace becomes a config
//! subtree. Reading one variable needs nothing from here — HOCON's own
//! `${?VAR}` already does that.

use std::collections::HashMap;

use indexmap::IndexMap;

use super::{config_from_object, AdapterError};
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
pub fn load(opts: Options) -> Result<Config, AdapterError> {
    if opts.prefix.is_empty() {
        return Err(AdapterError::new(
            "env: a prefix is required when mounting the environment (spec F1.1)",
        ));
    }
    // Filter while iterating rather than collecting the whole environment
    // first: everything else is never used, and some of it is secret.
    let vars: HashMap<String, String> = std::env::vars()
        .filter(|(name, _)| name.starts_with(&opts.prefix))
        .collect();
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

    let mut seen: HashMap<String, &str> = HashMap::new();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for name in names {
        let Some(rest) = name.strip_prefix(&opts.prefix) else {
            continue;
        };
        let path = to_path(rest, name)?;
        if let Some(prev) = seen.get(&path) {
            // F1.6: two names can reach one path and the environment has no
            // meaningful order to break the tie with, so neither wins.
            return Err(AdapterError::new(format!(
                "env: {prev} and {name} both map to \"{path}\""
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
    let mut pairs: Vec<(String, String)> = Vec::new();

    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
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
fn to_path(rest: &str, name: &str) -> Result<String, AdapterError> {
    let segs: Vec<String> = rest.split(SEPARATOR).map(|s| s.to_lowercase()).collect();
    if segs.iter().any(|s| s.is_empty()) {
        return Err(AdapterError::new(format!(
            "env: \"{name}\" produces an empty path segment"
        )));
    }
    Ok(segs.join("."))
}

/// Nest dotted paths, applying the objects-win rule over the whole set so the
/// outcome does not depend on input order (spec F1.8, mirroring F2.5).
fn nest(mut pairs: Vec<(String, String)>) -> HoconValue {
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut root: IndexMap<String, HoconValue> = IndexMap::new();
    for (path, value) in pairs {
        let segments: Vec<&str> = path.split('.').collect();
        set_nested(
            &mut root,
            &segments,
            HoconValue::Scalar(ScalarValue::string(value)),
        );
    }
    HoconValue::Object(root)
}

fn set_nested(map: &mut IndexMap<String, HoconValue>, segments: &[&str], value: HoconValue) {
    if segments.is_empty() {
        return;
    }
    if segments.len() == 1 {
        if !matches!(map.get(segments[0]), Some(HoconValue::Object(_))) {
            map.insert(segments[0].to_string(), value);
        }
        return;
    }
    let entry = map
        .entry(segments[0].to_string())
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
