//! YAML documents as HOCON config.
//!
//! This is a HOCON library, not a YAML implementation, and the API keeps that
//! boundary. What this module owns is the decoded-tree → HOCON step, exposed
//! directly as [`from_value`]: root must be a mapping, `${...}` stays literal,
//! NaN and infinity are refused, a multi-document stream is refused. How YAML
//! *text* becomes a tree — whether `010` is 8 or 10, whether `no` is a boolean
//! — is the YAML library's answer, not a contract here.
//!
//! [`parse`] is a convenience front on `yaml-rust2`. A caller who needs a
//! different library, version or schema decodes the text themselves and hands
//! the tree to [`from_value`]; that is the supported way to swap parsers, and
//! it keeps the choice, and its consequences, in the caller's hands.

use indexmap::IndexMap;
use yaml_rust2::{Yaml, YamlLoader};

use super::{config_from_object, AdapterError};
use crate::value::{HoconValue, ScalarValue};
use crate::Config;

/// Read YAML text with this module's default library.
pub fn parse(input: &str, origin: Option<&str>) -> Result<Config, AdapterError> {
    let docs = YamlLoader::load_from_str(input)
        .map_err(|e| AdapterError::new(format!("yaml: {e}")))?;
    if docs.len() > 1 {
        return Err(AdapterError::new(
            "yaml: multi-document streams are not supported (spec F5.7); a config is one document",
        ));
    }
    match docs.into_iter().next() {
        // An empty document is the empty object, as an empty HOCON document is
        // (S3.1), rather than a root-type failure (spec F5.9).
        None => Ok(config_from_object(
            HoconValue::Object(IndexMap::new()),
            origin,
        )),
        Some(doc) => from_value(&doc, origin),
    }
}

/// Read a YAML file, using its path as the origin description.
pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Config, AdapterError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::new(format!("yaml: {}: {e}", path.display())))?;
    parse(&text, Some(&path.display().to_string()))
}

/// Build a `Config` from an already-decoded YAML tree, produced by whatever
/// library and settings the caller chose. This is the tree-level boundary this
/// module owns; [`parse`] is just a default decoder in front of it.
pub fn from_value(doc: &Yaml, origin: Option<&str>) -> Result<Config, AdapterError> {
    if matches!(doc, Yaml::Null | Yaml::BadValue) {
        return Ok(config_from_object(
            HoconValue::Object(IndexMap::new()),
            origin,
        ));
    }
    let Yaml::Hash(_) = doc else {
        return Err(AdapterError::new(format!(
            "yaml: document root is {}, but a config root must be a mapping (spec F0.3)",
            describe(doc)
        )));
    };
    let value = convert(doc, "")?;
    Ok(config_from_object(value, origin))
}

fn describe(v: &Yaml) -> &'static str {
    match v {
        Yaml::Array(_) => "a sequence",
        Yaml::Hash(_) => "a mapping",
        Yaml::Null => "null",
        _ => "a scalar",
    }
}

fn convert(v: &Yaml, at: &str) -> Result<HoconValue, AdapterError> {
    match v {
        Yaml::Hash(h) => {
            let mut out: IndexMap<String, HoconValue> = IndexMap::new();
            // `yaml-rust2` leaves a merge key as an ordinary `<<` entry, so
            // merging is ours to do — a `<<` leaking through as a field would
            // be a structural difference, which this spec does own (F5.2).
            // Explicit keys win over merged ones.
            let mut merged: IndexMap<String, HoconValue> = IndexMap::new();
            for (k, e) in h {
                if matches!(k, Yaml::String(s) if s == "<<") {
                    collect_merge(e, at, &mut merged)?;
                    continue;
                }
                let ks = key_string(k, at)?;
                let path = if at.is_empty() {
                    ks.clone()
                } else {
                    format!("{at}.{ks}")
                };
                out.insert(ks, convert(e, &path)?);
            }
            for (k, e) in merged {
                out.entry(k).or_insert(e);
            }
            Ok(HoconValue::Object(out))
        }
        Yaml::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, e) in items.iter().enumerate() {
                out.push(convert(e, &format!("{at}[{i}]"))?);
            }
            Ok(HoconValue::Array(out))
        }
        Yaml::String(s) => Ok(HoconValue::Scalar(ScalarValue::string(s.clone()))),
        Yaml::Boolean(b) => Ok(HoconValue::Scalar(ScalarValue::boolean(*b))),
        Yaml::Integer(i) => Ok(HoconValue::Scalar(ScalarValue::number(i.to_string()))),
        Yaml::Real(raw) => {
            // YAML spells these `.nan`, `.inf`, `-.inf`, none of which Rust's
            // f64 parser accepts, so check the text before parsing (F0.6).
            let lower = raw.to_ascii_lowercase();
            if lower.ends_with(".nan") || lower.ends_with(".inf") {
                return Err(AdapterError::new(format!(
                    "yaml: at {at}: {raw} is not representable in HOCON (spec F0.6)"
                )));
            }
            let f: f64 = raw.parse().map_err(|_| {
                AdapterError::new(format!("yaml: at {at}: {raw:?} is not a number"))
            })?;
            if f.is_nan() || f.is_infinite() {
                return Err(AdapterError::new(format!(
                    "yaml: at {at}: {raw} is not representable in HOCON (spec F0.6)"
                )));
            }
            Ok(HoconValue::Scalar(ScalarValue::number(raw.clone())))
        }
        Yaml::Null => Ok(HoconValue::Scalar(ScalarValue::null())),
        Yaml::Alias(_) | Yaml::BadValue => Err(AdapterError::new(format!(
            "yaml: at {at}: unresolved node in the decoded tree"
        ))),
    }
}

fn collect_merge(
    e: &Yaml,
    at: &str,
    into: &mut IndexMap<String, HoconValue>,
) -> Result<(), AdapterError> {
    match e {
        Yaml::Hash(_) => {
            if let HoconValue::Object(fields) = convert(e, at)? {
                for (k, v) in fields {
                    into.entry(k).or_insert(v);
                }
            }
            Ok(())
        }
        // A sequence of mappings merges left to right, earlier winning.
        Yaml::Array(items) => {
            for item in items {
                collect_merge(item, at, into)?;
            }
            Ok(())
        }
        _ => Err(AdapterError::new(format!(
            "yaml: at {at}: a merge key must reference a mapping (spec F5.2)"
        ))),
    }
}

/// Non-string scalar keys map to their string forms; a collection key is an
/// error (spec F5.3).
fn key_string(k: &Yaml, at: &str) -> Result<String, AdapterError> {
    match k {
        Yaml::String(s) => Ok(s.clone()),
        Yaml::Integer(i) => Ok(i.to_string()),
        Yaml::Real(r) => Ok(r.clone()),
        Yaml::Boolean(b) => Ok(b.to_string()),
        Yaml::Null => Ok("null".to_string()),
        _ => Err(AdapterError::new(format!(
            "yaml: at {at}: a collection key is not usable as an object key (spec F5.3)"
        ))),
    }
}
