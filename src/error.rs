use std::fmt;

/// Error returned when HOCON input contains a syntax error.
///
/// Includes the line and column where the error was detected.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Human-readable description of the error.
    pub message: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ParseError at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Error returned when substitution resolution fails (e.g., missing
/// required substitution, cyclic reference).
#[derive(Debug, Clone)]
pub struct ResolveError {
    /// Human-readable description of the error.
    pub message: String,
    /// The substitution path that failed (e.g., `"db.host"`).
    pub path: String,
    /// 1-based line number where the substitution appeared.
    pub line: usize,
    /// 1-based column number where the substitution appeared.
    pub col: usize,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ResolveError at {}:{}: {} (path: {})",
            self.line, self.col, self.message, self.path
        )
    }
}

impl std::error::Error for ResolveError {}

/// Error returned by [`Config`](crate::Config) getters when a key is missing
/// or the value has the wrong type.
#[derive(Debug, Clone)]
pub struct ConfigError {
    /// Human-readable description of the error.
    pub message: String,
    /// The dot-separated path that was looked up.
    pub path: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConfigError: {} (path: {})", self.message, self.path)
    }
}

impl std::error::Error for ConfigError {}
