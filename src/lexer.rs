use crate::error::ParseError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Equals,
    PlusEquals,
    Newline,
    QuotedString,
    TripleQuotedString,
    Unquoted,
    Substitution,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct SubstPayload {
    pub segments: Vec<Segment>,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub line: usize,
    pub col: usize,
    #[allow(dead_code)]
    pub is_quoted: bool,
    pub preceding_space: bool,
    pub subst: Option<SubstPayload>,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut pos = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut had_space = false;

    // Strip UTF-8 BOM
    if !chars.is_empty() && chars[0] == '\u{FEFF}' {
        pos = 1;
    }

    let peek =
        |pos: usize, offset: usize| -> char { chars.get(pos + offset).copied().unwrap_or('\0') };

    while pos < chars.len() {
        let sl = line;
        let sc = col;
        let ch = chars[pos];

        // Whitespace (not newline)
        if ch == ' ' || ch == '\t' || ch == '\r' {
            pos += 1;
            col += 1;
            had_space = true;
            continue;
        }

        // Newline
        if ch == '\n' {
            pos += 1;
            line += 1;
            col = 1;
            if tokens
                .last()
                .is_none_or(|t: &Token| t.kind != TokenKind::Newline)
            {
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    value: "\n".into(),
                    line: sl,
                    col: sc,
                    is_quoted: false,
                    preceding_space: had_space,
                    subst: None,
                });
                had_space = false;
            }
            continue;
        }

        // Comments
        if ch == '/' && peek(pos, 1) == '/' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
                col += 1;
            }
            had_space = true;
            continue;
        }
        if ch == '#' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
                col += 1;
            }
            had_space = true;
            continue;
        }

        // Single-char punctuation
        let single_kind = match ch {
            '{' => Some(TokenKind::LBrace),
            '}' => Some(TokenKind::RBrace),
            '[' => Some(TokenKind::LBracket),
            ']' => Some(TokenKind::RBracket),
            ',' => Some(TokenKind::Comma),
            ':' => Some(TokenKind::Colon),
            _ => None,
        };
        if let Some(kind) = single_kind {
            pos += 1;
            col += 1;
            tokens.push(Token {
                kind,
                value: ch.to_string(),
                line: sl,
                col: sc,
                is_quoted: false,
                preceding_space: had_space,
                subst: None,
            });
            had_space = false;
            continue;
        }

        // = and +=
        if ch == '=' {
            pos += 1;
            col += 1;
            tokens.push(Token {
                kind: TokenKind::Equals,
                value: "=".into(),
                line: sl,
                col: sc,
                is_quoted: false,
                preceding_space: had_space,
                subst: None,
            });
            had_space = false;
            continue;
        }
        if ch == '+' && peek(pos, 1) == '=' {
            pos += 2;
            col += 2;
            tokens.push(Token {
                kind: TokenKind::PlusEquals,
                value: "+=".into(),
                line: sl,
                col: sc,
                is_quoted: false,
                preceding_space: had_space,
                subst: None,
            });
            had_space = false;
            continue;
        }

        // Substitution ${...} or ${?...}
        if ch == '$' && peek(pos, 1) == '{' {
            pos += 2;
            col += 2;
            let payload = parse_subst_body(&chars, &mut pos, &mut col, sl, sc)?;
            // Reconstruct a canonical value string from segments.
            // Segments that need quoting (contain dot, space, empty, etc.) are wrapped in "...".
            let value = payload
                .segments
                .iter()
                .map(|s| {
                    let t = &s.text;
                    if t.is_empty()
                        || t.contains('.')
                        || t.contains(' ')
                        || t.contains('\t')
                        || t.contains('"')
                        || t.contains('\\')
                        || t != t.trim()
                    {
                        let escaped = t.replace('\\', "\\\\").replace('"', "\\\"");
                        format!("\"{}\"", escaped)
                    } else {
                        t.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(".");
            tokens.push(Token {
                kind: TokenKind::Substitution,
                value,
                line: sl,
                col: sc,
                is_quoted: false,
                preceding_space: had_space,
                subst: Some(payload),
            });
            had_space = false;
            continue;
        }

        // Triple-quoted string
        if ch == '"' && peek(pos, 1) == '"' && peek(pos, 2) == '"' {
            pos += 3;
            col += 3;
            let mut value = String::new();
            let mut found_closing = false;
            loop {
                if pos >= chars.len() {
                    break;
                }
                if chars[pos] == '"' {
                    let mut quote_count = 0;
                    while pos < chars.len() && chars[pos] == '"' {
                        quote_count += 1;
                        pos += 1;
                        col += 1;
                    }
                    if quote_count >= 3 {
                        for _ in 0..(quote_count - 3) {
                            value.push('"');
                        }
                        found_closing = true;
                        break;
                    }
                    for _ in 0..quote_count {
                        value.push('"');
                    }
                    continue;
                }
                if chars[pos] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                value.push(chars[pos]);
                pos += 1;
            }
            if !found_closing {
                return Err(ParseError {
                    message: "unterminated triple-quoted string".into(),
                    line: sl,
                    col: sc,
                });
            }
            if value.starts_with('\n') {
                value = value[1..].to_string();
            }
            tokens.push(Token {
                kind: TokenKind::TripleQuotedString,
                value,
                line: sl,
                col: sc,
                is_quoted: true,
                preceding_space: had_space,
                subst: None,
            });
            had_space = false;
            continue;
        }

        // Quoted string
        if ch == '"' {
            pos += 1;
            col += 1;
            let value = read_quoted_body(&chars, &mut pos, &mut col, sl, sc)?;
            tokens.push(Token {
                kind: TokenKind::QuotedString,
                value,
                line: sl,
                col: sc,
                is_quoted: true,
                preceding_space: had_space,
                subst: None,
            });
            had_space = false;
            continue;
        }

        // Unquoted string
        if is_unquoted_start(ch) {
            let mut value = String::new();
            while pos < chars.len() && is_unquoted_continue(chars[pos], || peek(pos, 1)) {
                value.push(chars[pos]);
                pos += 1;
                col += 1;
            }
            let trimmed = value.trim_end().to_string();
            tokens.push(Token {
                kind: TokenKind::Unquoted,
                value: trimmed,
                line: sl,
                col: sc,
                is_quoted: false,
                preceding_space: had_space,
                subst: None,
            });
            had_space = false;
            continue;
        }

        return Err(ParseError {
            message: format!("unexpected character: {:?}", ch),
            line: sl,
            col: sc,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        value: String::new(),
        line,
        col,
        is_quoted: false,
        preceding_space: false,
        subst: None,
    });
    Ok(tokens)
}

/// Read the body of a quoted string (opening `"` already consumed).
/// Returns the decoded string or a ParseError.
/// `open_line`/`open_col` are the position of the opening `"` for error reporting.
fn read_quoted_body(
    chars: &[char],
    pos: &mut usize,
    col: &mut usize,
    open_line: usize,
    open_col: usize,
) -> Result<String, ParseError> {
    let mut value = String::new();
    while *pos < chars.len() && chars[*pos] != '"' {
        if chars[*pos] == '\n' {
            return Err(ParseError {
                message: "unterminated string".into(),
                line: open_line,
                col: open_col,
            });
        }
        if chars[*pos] == '\\' {
            let esc_col = *col;
            *pos += 1;
            *col += 1;
            if *pos >= chars.len() {
                return Err(ParseError {
                    message: "unterminated string".into(),
                    line: open_line,
                    col: open_col,
                });
            }
            let esc = chars[*pos];
            *pos += 1;
            *col += 1;
            match esc {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                'b' => value.push('\u{0008}'),
                'f' => value.push('\u{000C}'),
                'u' => {
                    let hex: String = chars[*pos..].iter().take(4).collect();
                    if hex.len() < 4 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(ParseError {
                            message: "invalid unicode escape".into(),
                            line: open_line,
                            col: esc_col,
                        });
                    }
                    let code =
                        u32::from_str_radix(&hex, 16).map_err(|_| ParseError {
                            message: "invalid unicode escape".into(),
                            line: open_line,
                            col: esc_col,
                        })?;
                    let c = char::from_u32(code).ok_or_else(|| ParseError {
                        message: "invalid unicode escape".into(),
                        line: open_line,
                        col: esc_col,
                    })?;
                    value.push(c);
                    *pos += 4;
                    *col += 4;
                }
                _ => {
                    return Err(ParseError {
                        message: "invalid escape sequence".into(),
                        line: open_line,
                        col: esc_col,
                    });
                }
            }
        } else {
            value.push(chars[*pos]);
            *pos += 1;
            *col += 1;
        }
    }
    if *pos >= chars.len() || chars[*pos] != '"' {
        return Err(ParseError {
            message: "unterminated string".into(),
            line: open_line,
            col: open_col,
        });
    }
    *pos += 1;
    *col += 1;
    Ok(value)
}

/// Returns true if `ch` is a valid unquoted character inside a `${...}` body.
/// Forbidden: whitespace (space/tab), `"`, `\`, `{`, `}`, `[`, `]`, `:`, `=`, `,`,
///            `+`, `#`, `` ` ``, `^`, `?`, `!`, `@`, `*`, `&`, `$`, `.`, newline, CR.
fn is_unquoted_subst_char(ch: char) -> bool {
    !matches!(
        ch,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '"'
            | '\\'
            | '{'
            | '}'
            | '['
            | ']'
            | ':'
            | '='
            | ','
            | '+'
            | '#'
            | '`'
            | '^'
            | '?'
            | '!'
            | '@'
            | '*'
            | '&'
            | '$'
            | '.'
    )
}

/// Parse the body of a `${...}` substitution (called after `${` has been consumed).
/// Returns the `SubstPayload` or a `ParseError`.
fn parse_subst_body(
    chars: &[char],
    pos: &mut usize,
    col: &mut usize,
    start_line: usize,
    start_col: usize,
) -> Result<SubstPayload, ParseError> {
    // Assumes `${` already consumed. Position is at char after `{`.

    // START: check for optional sigil
    let optional = if *pos < chars.len() && chars[*pos] == '?' {
        *pos += 1;
        *col += 1;
        true
    } else {
        false
    };

    // COLLECT
    // current segment state
    let mut cur_text = String::new();
    let mut cur_started = false;
    let mut cur_line = 0usize;
    let mut cur_col = 0usize;

    let mut pending_ws = String::new();
    let mut segments: Vec<Segment> = Vec::new();
    // Track last-seen DOT position for trailing-dot error reporting.
    let mut last_dot: Option<(usize, usize)> = None;

    loop {
        if *pos >= chars.len() {
            return Err(ParseError {
                message: "unterminated substitution".into(),
                line: start_line,
                col: start_col,
            });
        }
        let ch = chars[*pos];

        match ch {
            '}' => {
                // END
                *pos += 1;
                *col += 1;
                // Drop pending_ws (trailing whitespace)
                pending_ws.clear();
                break;
            }
            '"' => {
                // QUOTED token
                let q_line = start_line; // all on same conceptual line (no literal newlines allowed)
                let q_col = *col;
                if cur_started {
                    cur_text.push_str(&pending_ws);
                }
                pending_ws.clear();
                *pos += 1;
                *col += 1;
                let decoded = read_quoted_body(chars, pos, col, q_line, q_col)?;
                cur_text.push_str(&decoded);
                if !cur_started {
                    cur_line = q_line;
                    cur_col = q_col;
                    cur_started = true;
                }
            }
            ch if is_unquoted_subst_char(ch) => {
                // UNQUOTED token: read a run of unquoted chars
                let uq_col = *col;
                if cur_started {
                    cur_text.push_str(&pending_ws);
                }
                pending_ws.clear();
                if !cur_started {
                    cur_line = start_line;
                    cur_col = uq_col;
                    cur_started = true;
                }
                while *pos < chars.len() && is_unquoted_subst_char(chars[*pos]) {
                    cur_text.push(chars[*pos]);
                    *pos += 1;
                    *col += 1;
                }
            }
            '.' => {
                // DOT: flush current segment (or error if not started)
                let dot_col = *col;
                pending_ws.clear();
                if !cur_started {
                    return Err(ParseError {
                        message: "empty segment in path".into(),
                        line: start_line,
                        col: dot_col,
                    });
                }
                segments.push(Segment {
                    text: std::mem::take(&mut cur_text),
                    line: cur_line,
                    col: cur_col,
                });
                cur_started = false;
                cur_line = 0;
                cur_col = 0;
                last_dot = Some((start_line, dot_col));
                *pos += 1;
                *col += 1;
            }
            ' ' | '\t' => {
                // WS: buffer into pending_ws
                pending_ws.push(ch);
                *pos += 1;
                *col += 1;
            }
            '\n' | '\r' => {
                return Err(ParseError {
                    message: "unterminated substitution".into(),
                    line: start_line,
                    col: start_col,
                });
            }
            _ => {
                return Err(ParseError {
                    message: "unexpected character in substitution path".into(),
                    line: start_line,
                    col: *col,
                });
            }
        }
    }

    // END validation
    if cur_started {
        segments.push(Segment {
            text: cur_text,
            line: cur_line,
            col: cur_col,
        });
    } else if segments.is_empty() {
        // ${}
        return Err(ParseError {
            message: "empty substitution path".into(),
            line: start_line,
            col: start_col,
        });
    } else {
        // trailing dot: ${foo.} — report at the offending dot position
        let (err_line, err_col) = last_dot.unwrap_or((start_line, start_col));
        return Err(ParseError {
            message: "empty segment in path".into(),
            line: err_line,
            col: err_col,
        });
    }

    Ok(SubstPayload { segments, optional })
}

fn is_unquoted_start(ch: char) -> bool {
    !matches!(
        ch,
        '{' | '}'
            | '['
            | ']'
            | ','
            | ':'
            | '='
            | '+'
            | '#'
            | '\n'
            | '\r'
            | '\t'
            | ' '
            | '"'
            | '$'
            | '?'
            | '!'
            | '@'
            | '*'
            | '&'
            | '^'
            | '\\'
    )
}

fn is_unquoted_continue(ch: char, next_fn: impl Fn() -> char) -> bool {
    if matches!(
        ch,
        '{' | '}'
            | '['
            | ']'
            | ','
            | ':'
            | '='
            | '\n'
            | '\r'
            | '\t'
            | '#'
            | '"'
            | '$'
            | ' '
            | '?'
            | '!'
            | '@'
            | '*'
            | '&'
            | '^'
            | '\\'
    ) {
        return false;
    }
    if ch == '+' && next_fn() == '=' {
        return false;
    }
    if ch == '/' && next_fn() == '/' {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input)
            .unwrap()
            .iter()
            .map(|t| t.kind.clone())
            .collect()
    }

    fn first(input: &str) -> Token {
        tokenize(input).unwrap().into_iter().next().unwrap()
    }

    #[test]
    fn tokenizes_empty_string() {
        let tokens = tokenize("").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn tokenizes_braces_and_brackets() {
        assert_eq!(
            kinds("{}[]"),
            vec![
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn tokenizes_equals_and_plus_equals() {
        let tokens = tokenize("=+=").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Equals);
        assert_eq!(tokens[1].kind, TokenKind::PlusEquals);
    }

    #[test]
    fn tokenizes_colon_and_comma() {
        assert_eq!(
            kinds(":,"),
            vec![TokenKind::Colon, TokenKind::Comma, TokenKind::Eof]
        );
    }

    #[test]
    fn skips_slash_comments_keeps_newline() {
        let tokens = tokenize("// comment\nfoo").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Newline);
        assert_eq!(tokens[1].kind, TokenKind::Unquoted);
        assert_eq!(tokens[1].value, "foo");
    }

    #[test]
    fn skips_hash_comments() {
        let tokens = tokenize("# comment\nfoo").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Newline);
        assert_eq!(tokens[1].value, "foo");
    }

    #[test]
    fn tokenizes_quoted_strings() {
        let t = first("\"hello world\"");
        assert_eq!(t.kind, TokenKind::QuotedString);
        assert_eq!(t.value, "hello world");
        assert!(t.is_quoted);
    }

    #[test]
    fn handles_escape_sequences() {
        let t = first("\"a\\nb\\tc\"");
        assert_eq!(t.value, "a\nb\tc");
    }

    #[test]
    fn handles_unicode_escapes() {
        let t = first("\"\\u0041\"");
        assert_eq!(t.value, "A");
    }

    #[test]
    fn tokenizes_triple_quoted_strings() {
        let t = first("\"\"\"hello\nworld\"\"\"");
        assert_eq!(t.kind, TokenKind::TripleQuotedString);
        assert_eq!(t.value, "hello\nworld");
        assert!(t.is_quoted);
    }

    #[test]
    fn strips_leading_newline_from_triple_quoted() {
        let t = first("\"\"\"\nhello\"\"\"");
        assert_eq!(t.value, "hello");
    }

    #[test]
    fn tokenizes_unquoted_strings() {
        let t = first("localhost");
        assert_eq!(t.kind, TokenKind::Unquoted);
        assert_eq!(t.value, "localhost");
        assert!(!t.is_quoted);
    }

    #[test]
    fn tokenizes_numbers_as_unquoted() {
        let t = first("8080");
        assert_eq!(t.kind, TokenKind::Unquoted);
        assert_eq!(t.value, "8080");
    }

    #[test]
    fn tokenizes_substitutions() {
        let t = first("${server.host}");
        assert_eq!(t.kind, TokenKind::Substitution);
        assert_eq!(t.value, "server.host");
    }

    #[test]
    fn tokenizes_optional_substitutions() {
        let t = first("${?foo}");
        assert_eq!(t.kind, TokenKind::Substitution);
        assert_eq!(t.value, "foo");
        assert!(t.subst.as_ref().unwrap().optional);
    }

    #[test]
    fn tokenizes_newlines() {
        let tokens = tokenize("a\nb").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Newline);
    }

    #[test]
    fn deduplicates_consecutive_newlines() {
        let tokens = tokenize("a\n\n\nb").unwrap();
        let newlines: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Newline)
            .collect();
        assert_eq!(newlines.len(), 1);
    }

    #[test]
    fn tracks_line_and_col() {
        let tokens = tokenize("a\nb").unwrap();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].col, 1);
        assert_eq!(tokens[2].line, 2);
        assert_eq!(tokens[2].col, 1);
    }

    #[test]
    fn sets_preceding_space() {
        let tokens = tokenize("a b").unwrap();
        assert!(tokens[1].preceding_space);
        assert!(!tokens[0].preceding_space);
    }

    #[test]
    fn strips_utf8_bom() {
        let tokens = tokenize("\u{FEFF}foo").unwrap();
        assert_eq!(tokens[0].value, "foo");
    }

    #[test]
    fn stops_unquoted_at_dollar_for_concat() {
        let tokens = tokenize("foo${bar}").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Unquoted);
        assert_eq!(tokens[0].value, "foo");
        assert_eq!(tokens[1].kind, TokenKind::Substitution);
        assert_eq!(tokens[1].value, "bar");
        assert!(!tokens[1].preceding_space);
    }

    #[test]
    fn throws_on_unterminated_string() {
        assert!(tokenize("\"unterminated").is_err());
    }

    #[test]
    fn throws_on_unterminated_substitution() {
        assert!(tokenize("${foo").is_err());
    }

    #[test]
    fn throws_on_unterminated_triple_quoted_string() {
        assert!(tokenize(r#""""unterminated"#).is_err());
    }
}
