use crate::error::ParseError;
use crate::lexer::{Segment, Token, TokenKind};
use crate::value::{ScalarType, ScalarValue};

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
    /// A `${...}` or `${?...}` substitution node.
    ///
    /// `#[non_exhaustive]` on the variant means callers that pattern-match
    /// must use `..` for any fields they do not bind — ensures that adding
    /// new fields (e.g. `list_suffix`) does not silently break downstream
    /// exhaustive matches.
    #[non_exhaustive]
    Substitution {
        segments: Vec<Segment>,
        optional: bool,
        /// True when the substitution carries a `[]` suffix for env-var-list
        /// expansion (`${X[]}` / `${?X[]}`).
        list_suffix: bool,
        pos: Pos,
    },
    Include {
        path: String,
        required: bool,
        is_file: bool,
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
                let key_line = self.peek_line();
                let key_col = self.peek_col();
                self.advance();
                // Split unquoted key at dots, tracking the char offset of each
                // segment within the original raw token so S8.6 errors below
                // can point at the offending segment, not the token start.
                let mut seg_char_offset: usize = 0;
                for part in val.split('.') {
                    if !part.is_empty() {
                        // S8.6 (HOCON.md L270–276): each unquoted key segment
                        // that begins with '-' must be followed by a digit. The
                        // lexer sees `a.-foo` as a single unquoted token, so we
                        // validate per-segment here after splitting. Symmetric
                        // with the lexer and parse_subst_body checks.
                        let mut seg_chars = part.chars();
                        if seg_chars.next() == Some('-') {
                            let after = seg_chars.next();
                            if !after.is_some_and(|c| c.is_ascii_digit()) {
                                let after_str = match after {
                                    Some(c) => format!("{:?}", c),
                                    None => String::from("EOF"),
                                };
                                return Err(ParseError {
                                    message: format!(
                                        "unquoted key segment cannot begin with '-' unless followed by a digit (got '-' then {} in {:?}, HOCON.md L270-276)",
                                        after_str, part
                                    ),
                                    line: key_line,
                                    // Point at the segment start, not the token start.
                                    // Lexer columns are 1-based char positions on the same
                                    // line; substitutions/keys cannot span newlines, so
                                    // adding the char offset is safe.
                                    col: key_col + seg_char_offset,
                                });
                            }
                        }
                        segments.push(part.to_string());
                    }
                    // Advance offset past this segment + its trailing '.' separator
                    // (the '.' is consumed by split; account for it by adding 1
                    // unless this is the last segment).
                    seg_char_offset += part.chars().count() + 1;
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

            // Check for explicit dot separator between segments (e.g. "a"."b" or "a".b).
            // A standalone "." token or an unquoted token starting with "." (e.g. ".d" from
            // `"b.c".d`) both indicate a path separator; in the latter case the token is
            // re-read in the next iteration and the leading dot is consumed via split('.').
            if self.peek_kind() == TokenKind::Unquoted
                && self.peek_value().starts_with('.')
                && !self.peek_preceding_space()
            {
                if self.peek_value() == "." {
                    self.advance(); // consume the standalone dot separator
                }
                // For ".d"-style tokens, fall through to the next loop iteration
                // which will split ".d" on '.' → ["", "d"] and push "d".
                continue;
            }

            break;
        }

        Ok(segments)
    }

    fn parse_include(&mut self) -> Result<AstField, ParseError> {
        let p = self.current_pos();
        self.skip(&[TokenKind::Newline]);

        // Determine whether `required(...)` is present.
        //
        // The lexer produces unquoted tokens by consuming everything that is not
        // a stop character.  Parentheses are NOT stop characters, so the lexer
        // can produce tokens like:
        //   "required("          — from `required(`
        //   "required(file("     — from `required(file(`
        //   "required"           — from `required` (space before `(`)
        //
        // We normalise all of these into: required=true, cursor pointing at the
        // inner content after the `(` of `required(`.
        let kind = self.peek_kind();
        let raw = if kind == TokenKind::Unquoted {
            self.peek_value().to_string()
        } else {
            String::new()
        };

        let required = raw == "required" || raw.starts_with("required(");

        // Tracks whether `file(` has already been consumed as part of the
        // `required(file(` mega-token.
        let mut file_prefix_consumed = false;

        if required {
            if raw == "required" {
                // Separate tokens: consume "required", then expect "(" (possibly fused with "file(")
                self.advance();
                if self.peek_kind() == TokenKind::Unquoted && self.peek_value().starts_with('(') {
                    let val = self.peek_value().to_string();
                    if val == "(" {
                        self.advance(); // standalone "(" — inner content is next token
                    } else {
                        // Token is "(file(...)" or similar — strip leading "(" and treat
                        // the remainder as if it were emitted without "required(".
                        // We do this by rewriting the token in place via file_prefix_consumed.
                        let after_paren = &val[1..]; // strip leading "("
                        if after_paren == "file("
                            || after_paren.starts_with("file(")
                            || after_paren == "file"
                        {
                            file_prefix_consumed = true;
                            self.advance(); // consume "(file(..." token; path follows
                        }
                        // else: bare "(content" — inner content; fall through to path reading below
                    }
                }
            } else {
                // raw starts with "required(" — consume this token.
                // Check if `file(` is also embedded (e.g. "required(file(").
                let after_req = &raw["required(".len()..];
                if after_req == "file(" || after_req.starts_with("file(") {
                    file_prefix_consumed = true;
                }
                // Also handle "required(file" (split at space — unlikely but safe)
                if after_req == "file" {
                    file_prefix_consumed = true; // next token will be "("
                }
                self.advance(); // consume "required(..." token
            }
        }

        let path;
        let mut is_file = false;
        if self.peek_kind() == TokenKind::QuotedString {
            // Simple: include required("path") or include "path"
            path = self.peek_value().to_string();
            self.advance();
            if required {
                // Consume closing ")" — may be part of an Unquoted token or standalone
                if self.peek_kind() == TokenKind::Unquoted && self.peek_value().starts_with(')') {
                    self.advance();
                }
            }
        } else if (self.peek_kind() == TokenKind::Unquoted
            && (self.peek_value() == "file" || self.peek_value().starts_with("file(")))
            || file_prefix_consumed
        {
            // file("path") form — possibly with required( already consumed.
            is_file = true;
            let err_line = self.peek_line();
            let err_col = self.peek_col();

            if !file_prefix_consumed {
                // Consume the "file(" (or "file") token
                self.advance();
            }

            // Skip any remaining unquoted junk between file( and the quoted path
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
                message: format!("expected include path, got {:?}", self.peek_kind()),
                line,
                col,
            });
        }

        Ok(AstField {
            key: vec![],
            value: AstNode::Include {
                path,
                required,
                is_file,
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
                TokenKind::Substitution => {
                    let (optional, segs, list_suffix) = self
                        .tokens
                        .get(self.pos)
                        .and_then(|t| t.subst.as_ref())
                        .map(|p| (p.optional, p.segments.clone(), p.list_suffix))
                        .unwrap_or((false, Vec::new(), false));
                    let (_, _value, line, col) = self.advance_get();
                    AstNode::Substitution {
                        segments: segs,
                        optional,
                        list_suffix,
                        pos: Pos { line, col },
                    }
                }
                TokenKind::QuotedString | TokenKind::TripleQuotedString => {
                    let (_, val, line, col) = self.advance_get();
                    AstNode::Scalar {
                        value: ScalarValue::string(val),
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
                        value: ScalarValue::string(val),
                        pos: Pos { line, col },
                        separator: false,
                    }
                }
                _ => break,
            };

            if had_space {
                parts.push(AstNode::Scalar {
                    value: ScalarValue::string(" ".into()),
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
        "true" | "false" => {
            return ScalarValue::new(raw.to_string(), ScalarType::Boolean);
        }
        "null" => return ScalarValue::null(),
        _ => {}
    }

    // Number detection: first char must be 0-9 or - (Lightbend-aligned)
    if let Some(first) = raw.bytes().next() {
        if (first.is_ascii_digit() || first == b'-') && raw.parse::<f64>().is_ok() {
            return ScalarValue::number(raw.to_string());
        }
    }

    ScalarValue::string(raw.to_string())
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
        if let AstNode::Scalar { value, .. } = &fs[0].value {
            assert_eq!(value.value_type, ScalarType::Boolean);
            assert_eq!(value.raw, "true");
        } else {
            panic!("expected scalar");
        }
        if let AstNode::Scalar { value, .. } = &fs[1].value {
            assert_eq!(value.value_type, ScalarType::Boolean);
            assert_eq!(value.raw, "false");
        } else {
            panic!("expected scalar");
        }
        if let AstNode::Scalar { value, .. } = &fs[2].value {
            assert_eq!(value.value_type, ScalarType::Null);
        } else {
            panic!("expected scalar");
        }
    }

    #[test]
    fn parses_integer_scalars() {
        let node = parse("port = 8080");
        if let AstNode::Scalar { value, .. } = &fields(&node)[0].value {
            assert_eq!(value.value_type, ScalarType::Number);
            assert_eq!(value.raw, "8080");
        } else {
            panic!("expected scalar");
        }
    }

    #[test]
    fn parses_float_scalars() {
        let node = parse("ratio = 1.5");
        if let AstNode::Scalar { value, .. } = &fields(&node)[0].value {
            assert_eq!(value.value_type, ScalarType::Number);
            assert_eq!(value.raw, "1.5");
        } else {
            panic!("expected scalar");
        }
    }

    #[test]
    fn dot_prefix_is_string_not_number() {
        let node = parse("v = .33");
        if let AstNode::Scalar { value, .. } = &fields(&node)[0].value {
            assert_eq!(value.value_type, ScalarType::String);
            assert_eq!(value.raw, ".33");
        } else {
            panic!("expected scalar");
        }
    }

    #[test]
    fn parses_substitutions() {
        let node = parse("host = ${server.host}");
        if let AstNode::Substitution {
            segments, optional, ..
        } = &fields(&node)[0].value
        {
            let texts: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(texts, vec!["server", "host"]);
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
        if let AstNode::Include { is_file, .. } = &f.value {
            assert!(!is_file, "bare include should have is_file=false");
        } else {
            panic!("expected Include");
        }
    }

    #[test]
    fn parses_include_file_syntax() {
        let node = parse("include file(\"other.conf\")");
        if let AstNode::Include { is_file, .. } = &fields(&node)[0].value {
            assert!(is_file, "file() include should have is_file=true");
        } else {
            panic!("expected Include");
        }
    }

    // ── S12.5: `include` reserved at start of key path (HOCON.md L570) ────────

    #[test]
    fn include_dot_key_is_parse_error() {
        // ir03: unquoted dotted form must be rejected
        assert!(matches!(
            parse_tokens(&tokenize("include.foo = 1").unwrap()),
            Err(ParseError { .. })
        ));
    }

    #[test]
    fn include_nested_object_body_is_parse_error() {
        // ir04: reservation applies uniformly inside object literals
        assert!(matches!(
            parse_tokens(&tokenize("a = { include.bar = 1 }").unwrap()),
            Err(ParseError { .. })
        ));
    }

    #[test]
    fn quoted_include_bypasses_reservation() {
        // ir06: "include" = 1 must succeed
        assert!(parse_tokens(&tokenize(r#""include" = 1"#).unwrap()).is_ok());
    }

    #[test]
    fn quoted_include_dotted_bypasses_reservation() {
        // ir11: "include".foo = 1 must succeed
        assert!(parse_tokens(&tokenize(r#""include".foo = 1"#).unwrap()).is_ok());
    }

    #[test]
    fn include_bare_equals_is_parse_error() {
        // ir01 regression guard (already handled via parse_include path)
        assert!(parse_tokens(&tokenize("include = 1").unwrap()).is_err());
    }

    #[test]
    fn include_plus_equals_is_parse_error() {
        // ir10: += separator form
        assert!(parse_tokens(&tokenize("include += [1]").unwrap()).is_err());
    }

    #[test]
    fn include_object_body_is_parse_error() {
        // ir13: object-body field write form
        assert!(parse_tokens(&tokenize("include { x = 1 }").unwrap()).is_err());
    }

    #[test]
    fn foo_include_non_initial_is_ok() {
        // ir07 regression guard: non-initial include is not reserved
        assert!(parse_tokens(&tokenize("foo.include = 1").unwrap()).is_ok());
    }
}
