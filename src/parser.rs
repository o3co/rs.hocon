use crate::error::ParseError;
use crate::lexer::{Token, TokenKind};
use crate::value::ScalarValue;

#[derive(Debug, Clone)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AstNode {
    Object {
        fields: Vec<AstField>,
        pos: Pos,
    },
    Array {
        items: Vec<AstNode>,
        pos: Pos,
    },
    Scalar {
        value: ScalarValue,
        pos: Pos,
        /// True when this scalar was synthesized by the parser as whitespace
        /// between concatenated tokens (not user-authored).
        separator: bool,
    },
    Concat {
        nodes: Vec<AstNode>,
        pos: Pos,
    },
    Substitution {
        path: String,
        optional: bool,
        pos: Pos,
    },
    Include {
        path: String,
        pos: Pos,
    },
}

#[derive(Debug, Clone)]
pub struct AstField {
    pub key: Vec<String>,
    pub value: AstNode,
    pub append: bool,
    pub pos: Pos,
}

/// Entry point: parse a slice of tokens into an AST.
pub fn parse_tokens(tokens: &[Token]) -> Result<AstNode, ParseError> {
    let mut parser = Parser { tokens, pos: 0 };
    parser.skip(&[TokenKind::Newline]);
    if parser.peek_kind() == TokenKind::LBrace {
        let first_pos = parser.current_pos();
        parser.pos += 1;
        let node = parser.parse_object(true)?;
        let mut all_fields = match node {
            AstNode::Object { fields, .. } => fields,
            _ => unreachable!(),
        };

        // Loop: merge additional braced objects or trailing unbraced fields
        loop {
            parser.skip(&[TokenKind::Newline]);
            if parser.peek_kind() == TokenKind::Eof {
                break;
            }
            if parser.peek_kind() == TokenKind::LBrace {
                parser.pos += 1;
                let extra = parser.parse_object(true)?;
                if let AstNode::Object { fields, .. } = extra {
                    all_fields.extend(fields);
                }
            } else {
                // Remaining tokens are unbraced root fields
                let extra = parser.parse_object(false)?;
                if let AstNode::Object { fields, .. } = extra {
                    all_fields.extend(fields);
                }
                break; // unbraced parse consumes to EOF
            }
        }

        // Verify no remaining tokens after braced root (e.g. stray `}`)
        parser.skip(&[TokenKind::Newline]);
        if parser.peek_kind() != TokenKind::Eof {
            let pos = parser.current_pos();
            return Err(ParseError {
                message: format!(
                    "unexpected token after closing brace: {:?}",
                    parser.peek_kind()
                ),
                line: pos.line,
                col: pos.col,
            });
        }

        Ok(AstNode::Object {
            fields: all_fields,
            pos: first_pos,
        })
    } else {
        parser.parse_object(false)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek_kind(&self) -> TokenKind {
        self.tokens
            .get(self.pos)
            .map_or(TokenKind::Eof, |t| t.kind.clone())
    }

    fn peek_value(&self) -> &str {
        self.tokens.get(self.pos).map_or("", |t| t.value.as_str())
    }

    fn peek_line(&self) -> usize {
        self.tokens.get(self.pos).map_or(0, |t| t.line)
    }

    fn peek_col(&self) -> usize {
        self.tokens.get(self.pos).map_or(0, |t| t.col)
    }

    fn peek_preceding_space(&self) -> bool {
        self.tokens.get(self.pos).is_some_and(|t| t.preceding_space)
    }

    fn current_pos(&self) -> Pos {
        Pos {
            line: self.peek_line(),
            col: self.peek_col(),
        }
    }

    fn advance_get(&mut self) -> (TokenKind, String, usize, usize) {
        if let Some(t) = self.tokens.get(self.pos) {
            let result = (t.kind.clone(), t.value.clone(), t.line, t.col);
            self.pos += 1;
            result
        } else {
            (TokenKind::Eof, String::new(), 0, 0)
        }
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn skip(&mut self, kinds: &[TokenKind]) {
        while kinds.contains(&self.peek_kind()) {
            self.advance();
        }
    }

    fn parse_object(&mut self, expect_closing_brace: bool) -> Result<AstNode, ParseError> {
        let p = self.current_pos();
        let mut fields: Vec<AstField> = Vec::new();

        loop {
            self.skip(&[TokenKind::Newline]);
            let kind = self.peek_kind();
            if kind == TokenKind::Eof || kind == TokenKind::RBrace {
                break;
            }

            // include directive
            if kind == TokenKind::Unquoted && self.peek_value() == "include" {
                self.advance();
                fields.push(self.parse_include()?);
                self.skip(&[TokenKind::Newline]);
                if self.peek_kind() == TokenKind::Comma {
                    self.advance();
                }
                self.skip(&[TokenKind::Newline]);
                continue;
            }

            // key
            let key_pos = self.current_pos();
            let key = self.parse_key()?;

            // value separator (optional)
            self.skip(&[TokenKind::Newline]);
            let mut append = false;
            let sep_kind = self.peek_kind();
            match sep_kind {
                TokenKind::Equals => {
                    self.advance();
                }
                TokenKind::PlusEquals => {
                    self.advance();
                    append = true;
                }
                TokenKind::Colon => {
                    self.advance();
                }
                TokenKind::LBrace => { /* key { ... } shorthand — no advance */ }
                TokenKind::Newline | TokenKind::Eof => {}
                _ => {
                    let line = self.peek_line();
                    let col = self.peek_col();
                    return Err(ParseError {
                        message: format!("unexpected token after key: {:?}", sep_kind),
                        line,
                        col,
                    });
                }
            }

            self.skip(&[TokenKind::Newline]);
            let value = self.parse_value()?;
            fields.push(AstField {
                key,
                value,
                append,
                pos: key_pos,
            });

            // trailing separator
            self.skip(&[TokenKind::Newline]);
            if self.peek_kind() == TokenKind::Comma {
                self.advance();
            }
            self.skip(&[TokenKind::Newline]);
        }

        if expect_closing_brace {
            if self.peek_kind() != TokenKind::RBrace {
                let line = self.peek_line();
                let col = self.peek_col();
                return Err(ParseError {
                    message: "expected }".into(),
                    line,
                    col,
                });
            }
            self.advance();
        }

        Ok(AstNode::Object { fields, pos: p })
    }

    fn parse_key(&mut self) -> Result<Vec<String>, ParseError> {
        let mut segments: Vec<String> = Vec::new();
        let mut trailing_dot;

        loop {
            let kind = self.peek_kind();
            if kind == TokenKind::QuotedString {
                let val = self.peek_value().to_string();
                self.advance();
                segments.push(val); // quoted: no dot split
                trailing_dot = false;
            } else if kind == TokenKind::Unquoted {
                let val = self.peek_value().to_string();
                self.advance();
                // Split unquoted key at dots
                for part in val.split('.') {
                    if !part.is_empty() {
                        segments.push(part.to_string());
                    }
                }
                trailing_dot = val.ends_with('.');
            } else {
                if segments.is_empty() {
                    let line = self.peek_line();
                    let col = self.peek_col();
                    return Err(ParseError {
                        message: format!("expected key, got {:?}", kind),
                        line,
                        col,
                    });
                }
                break;
            }

            if trailing_dot {
                continue;
            }

            // Check for explicit dot separator between segments (e.g. "a"."b")
            if self.peek_kind() == TokenKind::Unquoted
                && self.peek_value() == "."
                && !self.peek_preceding_space()
            {
                self.advance(); // consume the dot separator
                continue;
            }

            break;
        }

        Ok(segments)
    }

    fn parse_include(&mut self) -> Result<AstField, ParseError> {
        let p = self.current_pos();
        self.skip(&[TokenKind::Newline]);
        let kind = self.peek_kind();

        let path;
        if kind == TokenKind::QuotedString {
            path = self.peek_value().to_string();
            self.advance();
        } else if kind == TokenKind::Unquoted
            && (self.peek_value() == "file(" || self.peek_value() == "file")
        {
            let err_line = self.peek_line();
            let err_col = self.peek_col();
            self.advance();
            // Skip tokens until we find the quoted path string
            while self.peek_kind() != TokenKind::QuotedString && self.peek_kind() != TokenKind::Eof
            {
                self.advance();
            }
            if self.peek_kind() == TokenKind::Eof {
                return Err(ParseError {
                    message: "expected include path".into(),
                    line: err_line,
                    col: err_col,
                });
            }
            path = self.peek_value().to_string();
            self.advance();
            // Skip closing ) and anything else on this line
            while self.peek_kind() != TokenKind::Newline
                && self.peek_kind() != TokenKind::RBrace
                && self.peek_kind() != TokenKind::Eof
            {
                self.advance();
            }
        } else {
            let line = self.peek_line();
            let col = self.peek_col();
            return Err(ParseError {
                message: format!("expected include path, got {:?}", kind),
                line,
                col,
            });
        }

        Ok(AstField {
            key: vec![],
            value: AstNode::Include {
                path,
                pos: p.clone(),
            },
            append: false,
            pos: p,
        })
    }

    fn parse_value(&mut self) -> Result<AstNode, ParseError> {
        let p = self.current_pos();
        let mut parts: Vec<AstNode> = Vec::new();

        loop {
            let kind = self.peek_kind();
            match kind {
                TokenKind::Eof
                | TokenKind::Newline
                | TokenKind::RBrace
                | TokenKind::RBracket
                | TokenKind::Comma => break,
                _ => {}
            }

            let had_space = self.peek_preceding_space() && !parts.is_empty();
            let t_line = self.peek_line();
            let t_col = self.peek_col();

            let node = match kind {
                TokenKind::LBrace => {
                    self.advance();
                    self.parse_object(true)?
                }
                TokenKind::LBracket => {
                    self.advance();
                    self.parse_array()?
                }
                TokenKind::Substitution | TokenKind::OptionalSubstitution => {
                    let optional = kind == TokenKind::OptionalSubstitution;
                    let (_, path, line, col) = self.advance_get();
                    AstNode::Substitution {
                        path,
                        optional,
                        pos: Pos { line, col },
                    }
                }
                TokenKind::QuotedString | TokenKind::TripleQuotedString => {
                    let (_, val, line, col) = self.advance_get();
                    AstNode::Scalar {
                        value: ScalarValue::String(val),
                        pos: Pos { line, col },
                        separator: false,
                    }
                }
                TokenKind::Unquoted => {
                    let (_, val, line, col) = self.advance_get();
                    AstNode::Scalar {
                        value: parse_scalar_value(&val),
                        pos: Pos { line, col },
                        separator: false,
                    }
                }
                TokenKind::Colon | TokenKind::Equals if !parts.is_empty() => {
                    let (_, val, line, col) = self.advance_get();
                    AstNode::Scalar {
                        value: ScalarValue::String(val),
                        pos: Pos { line, col },
                        separator: false,
                    }
                }
                _ => break,
            };

            if had_space {
                parts.push(AstNode::Scalar {
                    value: ScalarValue::String(" ".into()),
                    pos: Pos {
                        line: t_line,
                        col: t_col,
                    },
                    separator: true,
                });
            }
            parts.push(node);
        }

        if parts.is_empty() {
            let line = self.peek_line();
            let col = self.peek_col();
            return Err(ParseError {
                message: "expected value".into(),
                line,
                col,
            });
        }

        if parts.len() == 1 {
            return Ok(parts.into_iter().next().unwrap());
        }

        Ok(AstNode::Concat {
            nodes: parts,
            pos: p,
        })
    }

    fn parse_array(&mut self) -> Result<AstNode, ParseError> {
        let p = self.current_pos();
        let mut items: Vec<AstNode> = Vec::new();

        loop {
            self.skip(&[TokenKind::Newline]);
            if self.peek_kind() == TokenKind::RBracket || self.peek_kind() == TokenKind::Eof {
                break;
            }
            items.push(self.parse_value()?);
            self.skip(&[TokenKind::Newline]);
            if self.peek_kind() == TokenKind::Comma {
                self.advance();
            }
            self.skip(&[TokenKind::Newline]);
        }

        if self.peek_kind() != TokenKind::RBracket {
            let line = self.peek_line();
            let col = self.peek_col();
            return Err(ParseError {
                message: "expected ]".into(),
                line,
                col,
            });
        }
        self.advance();

        Ok(AstNode::Array { items, pos: p })
    }
}

fn parse_scalar_value(raw: &str) -> ScalarValue {
    match raw {
        "true" => return ScalarValue::Bool(true),
        "false" => return ScalarValue::Bool(false),
        "null" => return ScalarValue::Null,
        _ => {}
    }

    // Try integer first (no dot, no exponent)
    if !raw.contains('.') && !raw.contains('e') && !raw.contains('E') {
        if let Ok(n) = raw.parse::<i64>() {
            return ScalarValue::Int(n);
        }
    }

    // Try float
    if let Ok(f) = raw.parse::<f64>() {
        if !raw.is_empty() {
            return ScalarValue::Float(f);
        }
    }

    ScalarValue::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse(input: &str) -> AstNode {
        let tokens = tokenize(input).unwrap();
        parse_tokens(&tokens).unwrap()
    }

    fn fields(node: &AstNode) -> &[AstField] {
        match node {
            AstNode::Object { fields, .. } => fields,
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn parses_empty_input() {
        let node = parse("");
        assert!(matches!(node, AstNode::Object { .. }));
        assert_eq!(fields(&node).len(), 0);
    }

    #[test]
    fn parses_key_equals_value() {
        let node = parse("host = \"localhost\"");
        let f = &fields(&node)[0];
        assert_eq!(f.key, vec!["host"]);
        assert!(matches!(f.value, AstNode::Scalar { .. }));
    }

    #[test]
    fn parses_key_colon_value() {
        let node = parse("port: 8080");
        assert_eq!(fields(&node)[0].key, vec!["port"]);
    }

    #[test]
    fn parses_dot_notation_keys() {
        let node = parse("server.host = \"localhost\"");
        assert_eq!(fields(&node)[0].key, vec!["server", "host"]);
    }

    #[test]
    fn does_not_split_quoted_keys() {
        let node = parse("\"a.b\" = 1");
        assert_eq!(fields(&node)[0].key, vec!["a.b"]);
    }

    #[test]
    fn parses_nested_objects() {
        let node = parse("server { host = \"localhost\" }");
        assert_eq!(fields(&node)[0].key, vec!["server"]);
        assert!(matches!(fields(&node)[0].value, AstNode::Object { .. }));
    }

    #[test]
    fn parses_arrays() {
        let node = parse("list = [1, 2, 3]");
        let val = &fields(&node)[0].value;
        if let AstNode::Array { items, .. } = val {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn parses_boolean_and_null() {
        let node = parse("a = true\nb = false\nc = null");
        let fs = fields(&node);
        assert!(matches!(
            &fs[0].value,
            AstNode::Scalar {
                value: ScalarValue::Bool(true),
                ..
            }
        ));
        assert!(matches!(
            &fs[1].value,
            AstNode::Scalar {
                value: ScalarValue::Bool(false),
                ..
            }
        ));
        assert!(matches!(
            &fs[2].value,
            AstNode::Scalar {
                value: ScalarValue::Null,
                ..
            }
        ));
    }

    #[test]
    fn parses_integer_scalars() {
        let node = parse("port = 8080");
        assert!(matches!(
            &fields(&node)[0].value,
            AstNode::Scalar {
                value: ScalarValue::Int(8080),
                ..
            }
        ));
    }

    #[test]
    fn parses_float_scalars() {
        let node = parse("ratio = 1.5");
        if let AstNode::Scalar {
            value: ScalarValue::Float(f),
            ..
        } = &fields(&node)[0].value
        {
            assert!((f - 1.5).abs() < f64::EPSILON);
        } else {
            panic!("expected float");
        }
    }

    #[test]
    fn parses_substitutions() {
        let node = parse("host = ${server.host}");
        if let AstNode::Substitution { path, optional, .. } = &fields(&node)[0].value {
            assert_eq!(path, "server.host");
            assert!(!optional);
        } else {
            panic!("expected substitution");
        }
    }

    #[test]
    fn parses_optional_substitutions() {
        let node = parse("host = ${?server.host}");
        if let AstNode::Substitution { optional, .. } = &fields(&node)[0].value {
            assert!(optional);
        } else {
            panic!("expected substitution");
        }
    }

    #[test]
    fn parses_concat() {
        let node = parse("url = \"http://\"${host}\":8080\"");
        assert!(matches!(&fields(&node)[0].value, AstNode::Concat { .. }));
    }

    #[test]
    fn parses_plus_equals() {
        let node = parse("list += 1");
        assert!(fields(&node)[0].append);
    }

    #[test]
    fn parses_include_directive() {
        let node = parse("include \"other.conf\"");
        let f = &fields(&node)[0];
        assert!(f.key.is_empty());
        assert!(matches!(f.value, AstNode::Include { .. }));
    }

    #[test]
    fn parses_include_file_syntax() {
        let node = parse("include file(\"other.conf\")");
        assert!(matches!(&fields(&node)[0].value, AstNode::Include { .. }));
    }
}
