pub mod error;
pub mod value;
pub mod config;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod resolver;

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
