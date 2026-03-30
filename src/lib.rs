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
//! - [`ParseError`] -- syntax errors during lexing or parsing (includes line/column).
//! - [`ResolveError`] -- substitution resolution failures, cycle detection.
//! - [`ConfigError`] -- missing keys or type mismatches when accessing values.
//!
//! ## HOCON Specification
//!
//! For the full specification, see the
//! [Lightbend HOCON spec](https://github.com/lightbend/config/blob/main/HOCON.md).

pub mod error;
pub mod value;
pub mod config;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod resolver;
pub(crate) mod properties;

#[cfg(feature = "serde")]
pub mod serde;

pub use error::{ParseError, ResolveError, ConfigError};
pub use value::{HoconValue, ScalarValue};
pub use config::Config;

use std::collections::HashMap;
use std::path::Path;

/// Parse a HOCON string into a Config.
pub fn parse(input: &str) -> Result<Config, ParseError> {
    parse_with_env(input, &std::env::vars().collect())
}

/// Parse a HOCON file into a Config.
pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Config, ParseError> {
    parse_file_with_env(path, &std::env::vars().collect())
}

/// Parse a HOCON file with a custom environment variable map.
pub fn parse_file_with_env<P: AsRef<Path>>(path: P, env: &HashMap<String, String>) -> Result<Config, ParseError> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path).map_err(|e| ParseError {
        message: format!("failed to read file {}: {}", path.display(), e),
        line: 0,
        col: 0,
    })?;
    let tokens = lexer::tokenize(&content)?;
    let ast = parser::parse_tokens(&tokens)?;
    let mut opts = resolver::ResolveOptions::new(env.clone());
    if let Some(dir) = path.parent() {
        opts = opts.with_base_dir(dir.to_path_buf());
    }
    let value = resolver::resolve(ast, &opts).map_err(|e| ParseError {
        message: e.message,
        line: e.line,
        col: e.col,
    })?;
    match value {
        HoconValue::Object(fields) => Ok(Config::new(fields)),
        _ => Err(ParseError {
            message: "root must be an object".into(),
            line: 1,
            col: 1,
        }),
    }
}

/// Parse a HOCON string with a custom environment variable map.
pub fn parse_with_env(input: &str, env: &HashMap<String, String>) -> Result<Config, ParseError> {
    let tokens = lexer::tokenize(input)?;
    let ast = parser::parse_tokens(&tokens)?;
    let opts = resolver::ResolveOptions::new(env.clone());
    let value = resolver::resolve(ast, &opts).map_err(|e| ParseError {
        message: e.message,
        line: e.line,
        col: e.col,
    })?;
    match value {
        HoconValue::Object(fields) => Ok(Config::new(fields)),
        _ => Err(ParseError {
            message: "root must be an object".into(),
            line: 1,
            col: 1,
        }),
    }
}
