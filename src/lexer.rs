use crate::error::ParseError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LBrace, RBrace, LBracket, RBracket,
    Comma, Colon, Equals, PlusEquals,
    Newline,
    QuotedString, TripleQuotedString, Unquoted,
    Substitution, OptionalSubstitution,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub line: usize,
    pub col: usize,
    pub is_quoted: bool,
    pub preceding_space: bool,
}

pub fn tokenize(_input: &str) -> Result<Vec<Token>, ParseError> {
    Ok(vec![Token {
        kind: TokenKind::Eof,
        value: String::new(),
        line: 1, col: 1,
        is_quoted: false,
        preceding_space: false,
    }])
}
