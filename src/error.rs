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
    /// `line` and `col` are the position of the concat expression in the source
    /// (from `ConcatPlaceholder`). `path` is the field path being resolved.
    pub(crate) fn concat_type_mismatch(
        left_type: &str,
        right_type: &str,
        line: usize,
        col: usize,
    ) -> Self {
        ResolveError {
            message: format!(
                "value concatenation requires same-kind operands per HOCON S10; \
                 got {} + {}",
                left_type, right_type
            ),
            path: String::new(),
            line,
            col,
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
    /// Where the error happened.
    ///
    /// - From a getter: the dot-separated path that was looked up.
    /// - From a serde decode (`Config::get_as`, `from_str`, `from_file`;
    ///   feature `serde`): the path of the failing key, which can be
    ///   deeper than the path looked up and uses the `DeserializeError`
    ///   path form (`app.servers[0].port`, `m."web.api"`) — a location to
    ///   report, not a path a getter accepts (`[i]`, escape sequences).
    ///
    /// Empty for file-root errors (e.g. the S3.5 array-at-file-root
    /// rejection) and for a decode failure that concerns the document root.
    pub path: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            // File-root errors (S3.5 array-at-root) have no access path.
            return write!(f, "ConfigError: {}", self.message);
        }
        write!(f, "ConfigError: {} (path: {})", self.message, self.path)
    }
}

impl std::error::Error for ConfigError {}

impl ConfigError {
    /// Returns `true` if this error was produced because the value at the path
    /// contains an unresolved substitution placeholder.
    ///
    /// Use this instead of message-string matching to detect the "not resolved"
    /// condition when the caller holds a `ConfigError` (i.e., from a typed getter
    /// such as `get_string`, `get_i64`, etc.).
    ///
    /// # Example
    /// ```rust,ignore
    /// let err = config.get_string("my.key").unwrap_err();
    /// if err.is_not_resolved() {
    ///     // Call config.resolve(ResolveOptions::defaults()) first
    /// }
    /// ```
    pub fn is_not_resolved(&self) -> bool {
        self.message.starts_with("value is not resolved")
    }
}

/// Error returned by [`Config::resolve_with`](crate::Config::resolve_with)
/// when its source config still contains unresolved substitutions (E12
/// decision 10). A getter on an unresolved path returns a [`ConfigError`]
/// whose `is_not_resolved()` is `true` instead.
///
/// Detect via `downcast_ref::<NotResolvedError>()` or by matching
/// `HoconError::NotResolved(e)`.  Per E12 decision 12.
#[derive(Debug, Clone)]
pub struct NotResolvedError {
    /// The dot-separated path that was accessed.
    pub path: String,
}

impl fmt::Display for NotResolvedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "value at path {:?} is not resolved (call resolve() before accessing values)",
            self.path
        )
    }
}

impl std::error::Error for NotResolvedError {}

/// Unified error type returned by top-level parse functions.
///
/// Wraps the possible failure modes: syntax errors ([`ParseError`]),
/// substitution resolution failures ([`ResolveError`]), file I/O errors
/// ([`std::io::Error`]), unresolved-getter errors ([`NotResolvedError`]),
/// and Config-boundary type errors ([`ConfigError`] — e.g. the S3.5
/// array-at-file-root rejection).
#[non_exhaustive]
#[derive(Debug)]
pub enum HoconError {
    /// Syntax error during lexing or parsing.
    Parse(ParseError),
    /// Substitution resolution failure (missing key, cycle, etc.).
    Resolve(ResolveError),
    /// File I/O error when reading the top-level config file.
    Io(std::io::Error),
    /// `Config::resolve_with` was given a source config that is not
    /// resolved (E12 decision 10).
    NotResolved(NotResolvedError),
    /// Type-mismatch error at the Config boundary (the Lightbend
    /// `ConfigException.WrongType` analog). Returned by the parse functions
    /// for an array-root document (S3.5, HOCON.md L989-991): the document is
    /// valid syntax, but the object-rooted Config API requires an object at
    /// file root. `ConfigError.path` is empty for file-root errors.
    Config(ConfigError),
}

impl fmt::Display for HoconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HoconError::Parse(e) => write!(f, "{}", e),
            HoconError::Resolve(e) => write!(f, "{}", e),
            HoconError::Io(e) => write!(f, "I/O error: {}", e),
            HoconError::NotResolved(e) => write!(f, "{}", e),
            HoconError::Config(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for HoconError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HoconError::Parse(e) => Some(e),
            HoconError::Resolve(e) => Some(e),
            HoconError::Io(e) => Some(e),
            HoconError::NotResolved(e) => Some(e),
            HoconError::Config(e) => Some(e),
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
