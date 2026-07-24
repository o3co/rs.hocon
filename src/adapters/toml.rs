//! TOML documents as HOCON config.
//!
//! TOML's types line up with HOCON's apart from dates: HOCON has no datetime,
//! so all four TOML date-time types become their RFC 3339 string forms, which
//! is the honest representation rather than a lossy number (spec F4.2).

use indexmap::IndexMap;
use toml::Value as TomlValue;

use super::{config_from_object, AdapterError};
use crate::value::{HoconValue, ScalarValue};
use crate::Config;

/// Read TOML text.
pub fn parse(input: &str, origin: Option<&str>) -> Result<Config, AdapterError> {
    // A TOML document is a table; `Value: FromStr` parses a single value, so
    // parse the table type to get document semantics (F0.3 comes for free —
    // TOML has no other root shape).
    let table: toml::Table = input
        .parse()
        .map_err(|e| AdapterError::new(format!("toml: {e}")))?;
    let doc = TomlValue::Table(table);
    Ok(config_from_object(convert(&doc, "")?, origin))
}

/// Read a TOML file, using its path as the origin description.
pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Config, AdapterError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::new(format!("toml: {}: {e}", path.display())))?;
    parse(&text, Some(&path.display().to_string()))
}

fn convert(v: &TomlValue, at: &str) -> Result<HoconValue, AdapterError> {
    match v {
        TomlValue::Table(t) => {
            let mut out: IndexMap<String, HoconValue> = IndexMap::new();
            for (k, e) in t {
                let path = if at.is_empty() {
                    k.clone()
                } else {
                    format!("{at}.{k}")
                };
                out.insert(k.clone(), convert(e, &path)?);
            }
            Ok(HoconValue::Object(out))
        }
        TomlValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, e) in items.iter().enumerate() {
                out.push(convert(e, &format!("{at}[{i}]"))?);
            }
            Ok(HoconValue::Array(out))
        }
        TomlValue::String(s) => Ok(HoconValue::Scalar(ScalarValue::string(s.clone()))),
        TomlValue::Integer(i) => Ok(HoconValue::Scalar(ScalarValue::number(i.to_string()))),
        TomlValue::Float(f) => {
            if f.is_nan() || f.is_infinite() {
                return Err(AdapterError::new(format!(
                    "toml: at {at}: {f} is not representable in HOCON (spec F0.6)"
                )));
            }
            Ok(HoconValue::Scalar(ScalarValue::number(f.to_string())))
        }
        TomlValue::Boolean(b) => Ok(HoconValue::Scalar(ScalarValue::boolean(*b))),
        // All four date-time types render themselves in their own shape.
        TomlValue::Datetime(dt) => Ok(HoconValue::Scalar(ScalarValue::string(dt.to_string()))),
    }
}
