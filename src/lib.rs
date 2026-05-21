//! # hocon
//!
//! Full [Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md)-compliant
//! parser for Rust.
//!
//! ## Quick Example
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = hocon::parse(r#"
//!     server {
//!         host = "localhost"
//!         port = 8080
//!     }
//! "#)?;
//!
//! assert_eq!(config.get_string("server.host")?, "localhost");
//! assert_eq!(config.get_i64("server.port")?, 8080);
//! # Ok(())
//! # }
//! ```
//!
//! ## Parsing
//!
//! - [`parse`] -- parse a HOCON string into a [`Config`].
//! - [`parse_file`] -- parse a HOCON file. Include directives are resolved
//!   relative to the file's directory.
//! - [`parse_with_env`] / [`parse_file_with_env`] -- parse with a custom
//!   environment variable map instead of inheriting the process environment.
//!
//! ## Accessing Values
//!
//! [`Config`] provides typed getters that accept dot-separated paths:
//!
//! | Method | Return type |
//! |--------|-------------|
//! | [`Config::get_string`] | `Result<String, ConfigError>` |
//! | [`Config::get_i64`] | `Result<i64, ConfigError>` |
//! | [`Config::get_f64`] | `Result<f64, ConfigError>` |
//! | [`Config::get_bool`] | `Result<bool, ConfigError>` |
//! | [`Config::get_config`] | `Result<Config, ConfigError>` |
//! | [`Config::get_list`] | `Result<Vec<HoconValue>, ConfigError>` |
//! | [`Config::get_duration`] | `Result<Duration, ConfigError>` |
//! | [`Config::get_bytes`] | `Result<i64, ConfigError>` |
//!
//! Each typed getter has an `_option` variant (e.g., [`Config::get_string_option`])
//! that returns `Option<T>` instead.
//!
//! ## Duration and Byte-Size Values
//!
//! ```rust
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = hocon::parse(r#"
//!     timeout = 30 seconds
//!     max-upload = 512 MB
//! "#)?;
//!
//! let timeout = config.get_duration("timeout")?;
//! let max_upload = config.get_bytes("max-upload")?;
//! # Ok(())
//! # }
//! ```
//!
//! Duration units: `ns`, `us`, `ms`, `s`/`seconds`, `m`/`minutes`, `h`/`hours`, `d`/`days`.
//!
//! Byte-size units: `B`, `KB`, `KiB`, `MB`, `MiB`, `GB`, `GiB`, `TB`, `TiB`
//! (and their long forms like `megabytes`, `mebibytes`).
//!
//! ## Serde Deserialization
//!
//! With the `serde` feature enabled, deserialize a [`Config`] (or sub-config)
//! into any type implementing `serde::Deserialize`:
//!
//! ```rust,ignore
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Server {
//!     host: String,
//!     port: u16,
//! }
//!
//! let server: Server = config.get_config("server")?.deserialize()?;
//! ```
//!
//! ## Include Files
//!
//! HOCON supports `include` directives to compose configuration from multiple files:
//!
//! ```hocon
//! include "defaults.conf"
//!
//! server.port = 9090  # override a value from defaults
//! ```
//!
//! When parsing with [`parse_file`], include paths are resolved relative to the
//! file being parsed.
//!
//! ## Error Types
//!
//! - [`HoconError`] -- unified error returned by parse functions. Wraps:
//!   - [`ParseError`] -- syntax errors during lexing or parsing (includes line/column).
//!   - [`ResolveError`] -- substitution resolution failures, cycle detection.
//!   - `std::io::Error` -- file I/O errors (top-level file read; include file errors appear as [`ResolveError`]).
//! - [`ConfigError`] -- missing keys or type mismatches when accessing values.
//!
//! ## HOCON Specification
//!
//! For the full specification, see the
//! [Lightbend HOCON spec](https://github.com/lightbend/config/blob/main/HOCON.md).

pub mod config;
pub mod error;
pub mod lexer;
pub(crate) mod numeric_array;
pub mod options;
pub mod parser;
pub(crate) mod properties;
pub mod resolver;
pub mod value;
mod value_factory;

#[cfg(feature = "serde")]
pub mod serde;

pub use config::{Config, Period};
pub use error::{ConfigError, HoconError, NotResolvedError, ParseError, ResolveError};
pub use options::{ParseOptions, ResolveOptions};
pub use value::{HoconValue, ScalarType, ScalarValue};
pub use value_factory::empty;

#[cfg(feature = "serde")]
pub use value_factory::from_map;

// Lexer surface intentionally narrow — only the items integration tests
// and diagnostic tooling need. The full lexer module is not part of the
// public API.
pub use lexer::{tokenize, Segment, SubstPayload, Token, TokenKind};

#[cfg(feature = "serde")]
pub use serde::DeserializeError;

use std::collections::HashMap;
use std::path::Path;

/// Parse a HOCON string into a Config.
pub fn parse(input: &str) -> Result<Config, HoconError> {
    parse_with_env(input, &std::env::vars().collect())
}

/// Parse a HOCON string with explicit [`ParseOptions`].
///
/// `opts.resolve_substitutions = true` (default): fused parse + resolve, same
/// as [`parse`]. `opts.resolve_substitutions = false`: phase 1 only; returned
/// `Config` may have `is_resolved() = false`. Use [`Config::resolve`] later.
pub fn parse_string_with_options(input: &str, opts: ParseOptions) -> Result<Config, HoconError> {
    let tokens = lexer::tokenize(input)?;
    assert_non_empty_document(&tokens)?;
    let ast = parser::parse_tokens(&tokens)?;

    let env: HashMap<String, String> = opts.env.clone().unwrap_or_else(|| {
        if opts.resolve_substitutions {
            std::env::vars().collect()
        } else {
            HashMap::new()
        }
    });

    let mut internal_opts = resolver::InternalResolveOptions::new(env);
    if let Some(ref bd) = opts.base_dir {
        internal_opts = internal_opts.with_base_dir(bd.clone());
    }

    if opts.resolve_substitutions {
        // Fused path: phase 1 + phase 2.
        let value = resolver::resolve(ast, &internal_opts)?;
        match value {
            HoconValue::Object(fields) => {
                let mut cfg = Config::new(fields);
                cfg.parse_base_dir = opts.base_dir;
                cfg.origin_description = opts.origin_description;
                Ok(cfg)
            }
            _ => Err(HoconError::Parse(ParseError {
                message: "root must be an object".into(),
                line: 1,
                col: 1,
            })),
        }
    } else {
        // Deferred path: phase 1 only.
        let tree = resolver::build_tree(ast, &internal_opts)?;
        Ok(Config::new_from_res_obj(
            tree,
            opts.base_dir,
            opts.origin_description,
        ))
    }
}

/// Parse a HOCON file with explicit [`ParseOptions`].
/// File's parent directory is used as base_dir (overrides opts.base_dir).
pub fn parse_file_with_options<P: AsRef<Path>>(
    path: P,
    opts: ParseOptions,
) -> Result<Config, HoconError> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let opts = ParseOptions { base_dir, ..opts };
    parse_string_with_options(&content, opts)
}

/// Parse a HOCON file into a Config.
pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Config, HoconError> {
    parse_file_with_env(path, &std::env::vars().collect())
}

/// Parse a HOCON file with a custom environment variable map.
pub fn parse_file_with_env<P: AsRef<Path>>(
    path: P,
    env: &HashMap<String, String>,
) -> Result<Config, HoconError> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
    let tokens = lexer::tokenize(&content)?;
    assert_non_empty_document(&tokens)?;
    let ast = parser::parse_tokens(&tokens)?;
    let mut opts = resolver::InternalResolveOptions::new(env.clone());
    if let Some(dir) = path.parent() {
        opts = opts.with_base_dir(dir.to_path_buf());
    }
    let value = resolver::resolve(ast, &opts)?;
    match value {
        HoconValue::Object(fields) => Ok(Config::new(fields)),
        _ => Err(HoconError::Parse(ParseError {
            message: "root must be an object".into(),
            line: 1,
            col: 1,
        })),
    }
}

/// Parse a HOCON string with a custom environment variable map.
pub fn parse_with_env(input: &str, env: &HashMap<String, String>) -> Result<Config, HoconError> {
    let tokens = lexer::tokenize(input)?;
    assert_non_empty_document(&tokens)?;
    let ast = parser::parse_tokens(&tokens)?;
    let opts = resolver::InternalResolveOptions::new(env.clone());
    let value = resolver::resolve(ast, &opts)?;
    match value {
        HoconValue::Object(fields) => Ok(Config::new(fields)),
        _ => Err(HoconError::Parse(ParseError {
            message: "root must be an object".into(),
            line: 1,
            col: 1,
        })),
    }
}

/// Internal JSON renderer for use by Layer-2 fixture tests.
///
/// Emits compact sorted-key JSON. Not semver-stable.
/// Callers: `tests/deferred_resolution_fixtures.rs`.
#[doc(hidden)]
pub fn _render_json_for_test(config: &Config) -> String {
    use crate::value::HoconValue;
    use std::fmt::Write;

    fn render_value(val: &HoconValue, out: &mut String) {
        match val {
            HoconValue::Scalar(sv) => {
                use crate::value::ScalarType;
                match sv.value_type {
                    ScalarType::Null => out.push_str("null"),
                    ScalarType::Boolean => out.push_str(&sv.raw),
                    ScalarType::Number => out.push_str(&sv.raw),
                    ScalarType::String => {
                        let escaped = sv
                            .raw
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\n', "\\n")
                            .replace('\r', "\\r")
                            .replace('\t', "\\t");
                        let _ = write!(out, "\"{}\"", escaped);
                    }
                }
            }
            HoconValue::Object(map) => {
                out.push('{');
                let mut keys: Vec<&str> = map.keys().map(|s| s.as_str()).collect();
                keys.sort_unstable();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let _ = write!(out, "\"{}\":", k);
                    render_value(map.get(*k).unwrap(), out);
                }
                out.push('}');
            }
            HoconValue::Array(arr) => {
                out.push('[');
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    render_value(v, out);
                }
                out.push(']');
            }
            HoconValue::Placeholder(pv) => {
                let _ = write!(out, "\"<unresolved:{}>\"", pv.path);
            }
        }
    }

    let mut out = String::from("{");
    let mut keys: Vec<&str> = config.root.keys().map(|s| s.as_str()).collect();
    keys.sort_unstable();
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":", k);
        render_value(config.root.get(*k).unwrap(), &mut out);
    }
    out.push('}');
    out
}

/// Guard: reject token streams that carry no semantic content (HOCON.md L130).
///
/// An empty document is one whose token stream contains only `Newline` and `Eof`
/// tokens after the lexer has already stripped whitespace, BOM, and comments.
/// A document with at least one structural or value token (including `{`, `}`,
/// unquoted/quoted text, substitutions, …) is not empty even if it resolves to
/// an empty object.
fn assert_non_empty_document(tokens: &[lexer::Token]) -> Result<(), HoconError> {
    let has_content = tokens
        .iter()
        .any(|t| !matches!(t.kind, lexer::TokenKind::Newline | lexer::TokenKind::Eof));
    if !has_content {
        return Err(HoconError::Parse(ParseError {
            message: "empty file is not a valid HOCON document (HOCON.md L130)".into(),
            line: 1,
            col: 1,
        }));
    }
    Ok(())
}
