use std::fmt;

/// Error returned when HOCON input contains a syntax error.
///
/// Includes the line and column where the error was detected.
#[non_exhaustive]
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
#[non_exhaustive]
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

impl ResolveError {
    /// Construct a type-mismatch error for value-concat (S10.4/S10.13/S10.19).
    ///
    /// Called from `join_pair` which operates on resolved values without span
    /// info; `line` and `col` are 0 and `path` is empty to indicate the error
    /// arose inside the concat fold rather than at a substitution site.
    pub(crate) fn concat_type_mismatch(left_type: &str, right_type: &str) -> Self {
        ResolveError {
            message: format!(
                "value concatenation requires same-kind operands per HOCON S10; \
                 got {} + {} (S10.4/S10.13/S10.19)",
                left_type, right_type
            ),
            path: String::new(),
            line: 0,
            col: 0,
        }
    }
}

impl std::error::Error for ResolveError {}

/// Error returned by [`Config`](crate::Config) getters when a key is missing
/// or the value has the wrong type.
#[non_exhaustive]
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

/// Unified error type returned by top-level parse functions.
///
/// Wraps the three possible failure modes: syntax errors ([`ParseError`]),
/// substitution resolution failures ([`ResolveError`]), and file I/O
/// errors ([`std::io::Error`]).
#[non_exhaustive]
#[derive(Debug)]
pub enum HoconError {
    /// Syntax error during lexing or parsing.
    Parse(ParseError),
    /// Substitution resolution failure (missing key, cycle, etc.).
    Resolve(ResolveError),
    /// File I/O error when reading the top-level config file.
    Io(std::io::Error),
}

impl fmt::Display for HoconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HoconError::Parse(e) => write!(f, "{}", e),
            HoconError::Resolve(e) => write!(f, "{}", e),
            HoconError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for HoconError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HoconError::Parse(e) => Some(e),
            HoconError::Resolve(e) => Some(e),
            HoconError::Io(e) => Some(e),
        }
    }
}

impl From<ParseError> for HoconError {
    fn from(e: ParseError) -> Self {
        HoconError::Parse(e)
    }
}

impl From<ResolveError> for HoconError {
    fn from(e: ResolveError) -> Self {
        HoconError::Resolve(e)
    }
}

impl From<std::io::Error> for HoconError {
    fn from(e: std::io::Error) -> Self {
        HoconError::Io(e)
    }
}
