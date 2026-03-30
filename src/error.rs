use std::fmt;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
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

#[derive(Debug, Clone)]
pub struct ResolveError {
    pub message: String,
    pub path: String,
    pub line: usize,
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

#[derive(Debug, Clone)]
pub struct ConfigError {
    pub message: String,
    pub path: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConfigError: {} (path: {})", self.message, self.path)
    }
}

impl std::error::Error for ConfigError {}
