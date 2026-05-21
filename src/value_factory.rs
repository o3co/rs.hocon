// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! Value-factory helpers: [`empty`] and (with `serde` feature) [`from_map`].
//!
//! `empty` is always available.  `from_map` requires the `serde` feature
//! (it accepts `serde_json::Map` as input).

use crate::config::Config;
use indexmap::IndexMap;

#[cfg(feature = "serde")]
use crate::error::ConfigError;
#[cfg(feature = "serde")]
use crate::value::{HoconValue, ScalarType, ScalarValue};

/// Return an empty `Config` with no keys.
///
/// Equivalent to constructing an empty HOCON document. The resulting `Config`
/// is always resolved (`is_resolved()` returns `true`).
///
/// `origin_description` is the user-visible source name for error messages.
/// Pass `None` to omit.
pub fn empty(origin_description: Option<&str>) -> Config {
    Config::new_with_meta(IndexMap::new(), origin_description.map(|s| s.to_owned()))
}

/// Construct a resolved `Config` from a `serde_json::Map<String, Value>`.
///
/// Keys are treated as plain keys (NOT path expressions — the key `"a.b"` creates
/// a top-level entry literally named `"a.b"`, not a nested `a.b`). Values are
/// coerced to the internal HOCON representation per the E12 value-factory
/// type-coercion table.
///
/// `from_map` never produces substitution placeholders; the returned `Config` is
/// always resolved (`is_resolved()` returns `true`).
///
/// `origin_description` is the user-visible source name for error messages.
/// Pass `None` to omit.
///
/// # Errors
///
/// Returns a `ConfigError` if a value cannot be coerced (e.g. a `serde_json::Number`
/// that is not representable as either `i64` or finite `f64`).
///
/// This function requires the `serde` feature flag.
#[cfg(feature = "serde")]
pub fn from_map(
    values: serde_json::Map<String, serde_json::Value>,
    origin_description: Option<&str>,
) -> Result<Config, ConfigError> {
    let root = coerce_map(values)?;
    Ok(Config::new_with_meta(
        root,
        origin_description.map(|s| s.to_owned()),
    ))
}

#[cfg(feature = "serde")]
fn coerce_map(
    map: serde_json::Map<String, serde_json::Value>,
) -> Result<IndexMap<String, HoconValue>, ConfigError> {
    // Sorted key iteration for stable cross-impl JSON output.
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    let mut result = IndexMap::new();
    for k in keys {
        let v = map.get(&k).unwrap().clone();
        let hv = coerce_value(v).map_err(|msg| ConfigError {
            path: k.clone(),
            message: msg,
        })?;
        result.insert(k, hv);
    }
    Ok(result)
}

#[cfg(feature = "serde")]
fn coerce_value(v: serde_json::Value) -> Result<HoconValue, String> {
    use serde_json::Value;
    match v {
        Value::Null => Ok(HoconValue::Scalar(ScalarValue {
            raw: "null".to_owned(),
            value_type: ScalarType::Null,
        })),
        Value::Bool(b) => Ok(HoconValue::Scalar(ScalarValue {
            raw: if b { "true" } else { "false" }.to_owned(),
            value_type: ScalarType::Boolean,
        })),
        Value::String(s) => Ok(HoconValue::Scalar(ScalarValue {
            raw: s,
            value_type: ScalarType::String,
        })),
        Value::Number(n) => {
            // T4 fix: use serde_json::Number's canonical JSON token form as the raw
            // representation. This preserves exact integer values for numbers that
            // fit in u64 but not i64 (e.g. u64::MAX), and avoids the precision loss
            // and formatting change that `format!("{}", f)` introduced.
            //
            // Guard: serde_json::Number does not allow NaN/Inf natively, but we
            // still check as_f64 finiteness for floats to be safe.
            if n.is_i64() || n.is_u64() {
                // Integer: canonical JSON token is exact; use it directly.
                Ok(HoconValue::Scalar(ScalarValue {
                    raw: n.to_string(),
                    value_type: ScalarType::Number,
                }))
            } else if let Some(f) = n.as_f64() {
                if !f.is_finite() {
                    return Err(format!(
                        "number {} is not finite (NaN/Inf not representable in HOCON)",
                        n
                    ));
                }
                // Float: canonical JSON token form from serde_json preserves the
                // original source representation better than format!("{}", f).
                Ok(HoconValue::Scalar(ScalarValue {
                    raw: n.to_string(),
                    value_type: ScalarType::Number,
                }))
            } else {
                Err(format!(
                    "number {} cannot be represented as i64, u64, or f64",
                    n
                ))
            }
        }
        Value::Array(arr) => {
            let mut items = Vec::with_capacity(arr.len());
            for (i, elem) in arr.into_iter().enumerate() {
                let hv = coerce_value(elem).map_err(|msg| format!("element[{}]: {}", i, msg))?;
                items.push(hv);
            }
            Ok(HoconValue::Array(items))
        }
        Value::Object(obj) => {
            let inner = coerce_map(obj).map_err(|e| e.message)?;
            Ok(HoconValue::Object(inner))
        }
    }
}
