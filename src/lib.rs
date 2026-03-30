pub mod error;
pub mod value;
pub mod config;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod resolver;

pub use error::{ParseError, ResolveError, ConfigError};
pub use value::{HoconValue, ScalarValue};
pub use config::Config;

use std::collections::HashMap;

/// Parse a HOCON string into a Config.
pub fn parse(input: &str) -> Result<Config, ParseError> {
    parse_with_env(input, &std::env::vars().collect())
}

/// Parse a HOCON string with a custom environment variable map.
pub fn parse_with_env(input: &str, env: &HashMap<String, String>) -> Result<Config, ParseError> {
    let tokens = lexer::tokenize(input)?;
    let ast = parser::parse_tokens(&tokens)?;
    let value = resolver::resolve(ast, env).map_err(|e| ParseError {
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
