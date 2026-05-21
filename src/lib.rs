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
pub(crate) mod lexer;
pub(crate) mod numeric_array;
pub(crate) mod parser;
pub(crate) mod properties;
pub(crate) mod resolver;
pub mod value;

#[cfg(feature = "serde")]
pub mod serde;

pub use config::{Config, Period};
pub use error::{ConfigError, HoconError, ParseError, ResolveError};
pub use value::{HoconValue, ScalarType, ScalarValue};

// Lexer surface intentionally narrow — only the items integration tests
// and diagnostic tooling need. The full lexer module is not part of the
// public API.
pub use lexer::{tokenize, Segment, SubstPayload, Token, TokenKind};

#[cfg(feature = "serde")]
pub use serde::DeserializeError;

use std::collections::HashMap;
use std::path::Path;

// ── include-package feature: public Parser builder ───────────────────────────

/// Builder-style parser with a per-instance package registry for
/// `include package(...)` support (E11).
///
/// # Feature flag
///
/// This type is only available when the `include-package` Cargo feature is
/// enabled:
///
/// ```toml
/// [dependencies]
/// hocon-parser = { version = "...", features = ["include-package"] }
/// ```
///
/// # Usage
///
/// ```rust,ignore
/// # #[cfg(feature = "include-package")]
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = hocon::Parser::new()
///     .register_package("github.com/org/pkg", "reference.conf", include_str!("conf/reference.conf"))
///     .parse_file("app.conf")?;
/// # Ok(())
/// # }
/// ```
///
/// # Cascade convention
///
/// For packages that depend on other HOCON-config-providing packages, follow
/// this convention to cascade registrations:
///
/// ```rust,ignore
/// // In your package (e.g., pkg_a/src/hocon.rs):
/// pub fn register(parser: hocon::Parser) -> hocon::Parser {
///     let parser = parser
///         .register_package("github.com/org/pkg_a", "reference.conf", include_str!("../conf/reference.conf"));
///     // Cascade to dependencies:
///     // let parser = pkg_b::hocon::register(parser);
///     parser
/// }
/// ```
///
/// Callers:
/// ```rust,ignore
/// let config = pkg_a::hocon::register(hocon::Parser::new())
///     .parse_file("app.conf")?;
/// ```
///
/// # Collision policy
///
/// Registering two **different** content strings for the same `(identifier, file)`
/// key **panics** — this is a programming error (setup-time invariant). Re-registering
/// **byte-identical** content is idempotent (no panic).
#[cfg(feature = "include-package")]
pub struct Parser {
    registry: HashMap<(String, String), String>,
}

#[cfg(feature = "include-package")]
impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "include-package")]
impl Parser {
    /// Create a new `Parser` with an empty package registry.
    pub fn new() -> Self {
        Parser {
            registry: HashMap::new(),
        }
    }

    /// Register HOCON content for an `include package("identifier", "file")`
    /// include statement.
    ///
    /// # Arguments
    ///
    /// * `identifier` — the package identifier (e.g., `"github.com/org/pkg"`).
    ///   Should follow Go-module-path style for cross-impl portability (E11 decision 1),
    ///   but any non-empty string is accepted by the parser.
    /// * `file` — the file path within the package (e.g., `"reference.conf"`).
    ///   Must satisfy E11 decision 6 constraints.
    /// * `content` — the HOCON source text. Typically loaded via `include_str!`.
    ///   Empty content is valid and contributes `{}` to the merge.
    ///
    /// # Panics
    ///
    /// Panics if different content is registered for the same `(identifier, file)` pair.
    /// Re-registering byte-identical content is idempotent (no panic).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let parser = hocon::Parser::new()
    ///     .register_package("github.com/org/pkg", "reference.conf", include_str!("conf/reference.conf"))
    ///     .register_package("github.com/org/pkg", "overrides.conf", include_str!("conf/overrides.conf"));
    /// ```
    pub fn register_package(
        mut self,
        identifier: impl Into<String>,
        file: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let id = identifier.into();
        let f = file.into();
        let c = content.into();
        if let Some(existing) = self.registry.get(&(id.clone(), f.clone())) {
            if existing != &c {
                panic!(
                    "hocon: conflicting content registered for package ({:?}, {:?}): \
                     different content already registered for this (identifier, file) pair",
                    id, f
                );
            }
            // byte-identical: idempotent, no-op
        } else {
            self.registry.insert((id, f), c);
        }
        self
    }

    /// Parse a HOCON string using the registered package registry.
    pub fn parse(self, input: &str) -> Result<Config, HoconError> {
        self.parse_with_env(input, &std::env::vars().collect())
    }

    /// Parse a HOCON file using the registered package registry.
    pub fn parse_file(self, path: impl AsRef<Path>) -> Result<Config, HoconError> {
        self.parse_file_with_env(path, &std::env::vars().collect())
    }

    /// Parse a HOCON string with a custom environment map and the registered registry.
    pub fn parse_with_env(
        self,
        input: &str,
        env: &HashMap<String, String>,
    ) -> Result<Config, HoconError> {
        let tokens = lexer::tokenize(input)?;
        assert_non_empty_document(&tokens)?;
        let ast = parser::parse_tokens(&tokens)?;
        let opts = self.into_resolve_opts(env.clone(), None);
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

    /// Parse a HOCON file with a custom environment map and the registered registry.
    pub fn parse_file_with_env(
        self,
        path: impl AsRef<Path>,
        env: &HashMap<String, String>,
    ) -> Result<Config, HoconError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
        let tokens = lexer::tokenize(&content)?;
        assert_non_empty_document(&tokens)?;
        let ast = parser::parse_tokens(&tokens)?;
        let opts = self.into_resolve_opts(env.clone(), path.parent().map(|p| p.to_path_buf()));
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

    /// Convert this `Parser` into `ResolveOptions`, threading the registry in.
    fn into_resolve_opts(
        self,
        env: HashMap<String, String>,
        base_dir: Option<std::path::PathBuf>,
    ) -> resolver::ResolveOptions {
        let mut opts = resolver::ResolveOptions::new(env);
        if let Some(dir) = base_dir {
            opts = opts.with_base_dir(dir);
        }
        opts.package_registry = std::sync::Arc::new(self.registry);
        opts
    }
}

/// Parse a HOCON string into a Config.
pub fn parse(input: &str) -> Result<Config, HoconError> {
    parse_with_env(input, &std::env::vars().collect())
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
    let mut opts = resolver::ResolveOptions::new(env.clone());
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
    let opts = resolver::ResolveOptions::new(env.clone());
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
