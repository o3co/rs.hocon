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

/// Payload carried by a `${...}` or `${?...}` substitution token.
///
/// `#[non_exhaustive]` ensures that adding new fields here (e.g. future spec
/// extensions) does not break downstream crates that pattern-match or
/// construct this struct.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct SubstPayload {
    pub segments: Vec<Segment>,
    pub optional: bool,
    /// True when the substitution body carries a `[]` suffix, signalling
    /// env-var-list expansion (`${X[]}` / `${?X[]}`).
    pub list_suffix: bool,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub line: usize,
    pub col: usize,
    #[allow(dead_code)]
    pub is_quoted: bool,
    /// True if preceded by whitespace OR a comment (concat detection, S10.5 / S10.8).
    pub preceding_space: bool,
    /// Literal preceding-whitespace chars consumed since the previous token.
    /// Used by `parse_key` to preserve path-expression whitespace per E13 — for
    /// `a b. c = 1` the ' ' before `c` becomes a leading-space prefix on the
    /// post-dot segment.
    ///
    /// Note: `preceding_space` may be true while `preceding_whitespace` is empty
    /// when the token is preceded only by a comment (no literal WS chars). The
    /// boolean is the right signal for concat detection; the string is the right
    /// signal for path-WS preservation. The comment-only case cannot fire in
    /// practice in current grammar (comments run to `\n` which emits a newline
    /// token), but the distinction is preserved structurally.
    ///
    /// Visibility is `pub(crate)` — `Token` is publicly re-exported as
    /// `hocon::Token` and downstream crates may construct it via struct
    /// literals; adding a required `pub` field would be source-breaking for a
    /// patch release. Internal callers (`parser.rs::parse_key`) consume the
    /// field directly; external callers continue to use `preceding_space` for
    /// their concat-detection needs. Promotion to `pub` is deferred to v2.0.
    pub(crate) preceding_whitespace: String,
    pub subst: Option<SubstPayload>,
}

/// Returns true for every character in the HOCON whitespace set.
///
/// The set is defined by Lightbend HOCON.md §Whitespace (L165-184) as:
///   Java Character.isWhitespace set
///   ∪ { U+00A0, U+2007, U+202F }  (NBSP variants Java excludes)
///   ∪ { U+FEFF }                  (BOM)
///
/// Expanded:
///   ASCII:  0x09 (TAB), 0x0A (LF), 0x0B (VTAB), 0x0C (FF), 0x0D (CR),
///           0x1C (FS), 0x1D (GS), 0x1E (RS), 0x1F (US)
///   Zs:     0x20, 0x00A0, 0x1680, 0x2000-0x200A, 0x202F, 0x205F, 0x3000
///   Zl:     0x2028
///   Zp:     0x2029
///   BOM:    0xFEFF
///
/// NOTE: U+000A (LF) is included here because it is in the Java
/// Character.isWhitespace set.  Callers that need to distinguish newline from
/// inter-token whitespace must call is_hocon_newline first.
pub(crate) fn is_hocon_whitespace(ch: char) -> bool {
    matches!(ch,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r'
      | '\u{001C}'..='\u{001F}'
      | ' ' | '\u{00A0}' | '\u{FEFF}'
      | '\u{1680}'
      | '\u{2000}'..='\u{200A}'
      | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}'
      | '\u{3000}'
    )
}

/// Returns true if `ch` is the HOCON newline character (ASCII LF, U+000A only).
///
/// Per HOCON.md L182-184: "newline refers only and specifically to ASCII
/// newline 0x000A".  Unicode line/paragraph separators (U+2028, U+2029) are
/// whitespace but NOT newlines.
fn is_hocon_newline(ch: char) -> bool {
    ch == '\n'
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut pos = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut had_space = false;
    // E13 — accumulates literal whitespace chars consumed between tokens.
    // Drained (via std::mem::take) on every token push. Comment text is NOT
    // accumulated; only the actual WS chars.
    let mut whitespace_buffer = String::new();

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

        // Newline (must be checked before general whitespace because
        // is_hocon_whitespace also returns true for LF — see spec §D).
        if is_hocon_newline(ch) {
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
                    preceding_whitespace: std::mem::take(&mut whitespace_buffer),
                    subst: None,
                });
                had_space = false;
            }
            continue;
        }

        // Whitespace (not newline) — full HOCON_WS set per spec L165-184.
        if is_hocon_whitespace(ch) {
            whitespace_buffer.push(ch);
            pos += 1;
            col += 1;
            had_space = true;
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
                subst: None,
            });
            had_space = false;
            continue;
        }

        // Unquoted string
        if is_unquoted_start(ch) {
            // S8.6 / E8 (xx.hocon#31, xx.hocon#32 / commit dd102e8): the
            // value-position read of HOCON.md L270-276 admits `-` even when
            // not followed by a digit (bare `-` and `-foo` are unquoted
            // strings, matching Lightbend's reference) and admits digit-
            // leading runs (greedy: parse as number first, fall back to
            // unquoted string when the run isn't a valid number — rs.hocon
            // has no separate Number token kind, so this is realized at the
            // parser/coerce layer in parse_scalar_value). The strict reject
            // at this site was removed by the E8 amendment; concat-
            // continuation cases like `${a}-bar` rely on the absence of
            // that reject to extend the unquoted run after a value-token.
            // Path-element strict checks live elsewhere — see
            // parse_subst_body (this file) and parse_key (parser.rs).
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
                preceding_whitespace: std::mem::take(&mut whitespace_buffer),
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
        preceding_whitespace: String::new(),
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
                    let code = u32::from_str_radix(&hex, 16).map_err(|_| ParseError {
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
/// Forbidden: any HOCON whitespace (full set per is_hocon_whitespace), `"`, `\`,
///            `{`, `}`, `[`, `]`, `:`, `=`, `,`, `+`, `#`, `` ` ``, `^`, `?`,
///            `!`, `@`, `*`, `&`, `$`, `.`.
fn is_unquoted_subst_char(ch: char) -> bool {
    if is_hocon_whitespace(ch) {
        return false;
    }
    !matches!(
        ch,
        '"' | '\\'
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

/// Consume the literal two-character sequence `[]` at the current position.
///
/// Called by `parse_subst_body` when the `[` arm fires. Expects `chars[*pos] == '['`
/// on entry. Strict: no whitespace inside the brackets (`${X[ ]}` is a lex error).
fn parse_literal_brackets(
    chars: &[char],
    pos: &mut usize,
    col: &mut usize,
    start_line: usize,
) -> Result<(), ParseError> {
    // Consume `[`.
    debug_assert!(*pos < chars.len() && chars[*pos] == '[');
    *pos += 1;
    *col += 1;
    // Next char must be `]` (no whitespace inside the brackets).
    if *pos >= chars.len() || chars[*pos] != ']' {
        let got = chars
            .get(*pos)
            .map(|c| c.escape_debug().to_string())
            .unwrap_or_else(|| "EOF".into());
        return Err(ParseError {
            message: format!(
                "expected ']' after '[' in substitution list suffix, got {}",
                got
            ),
            line: start_line,
            col: *col,
        });
    }
    *pos += 1;
    *col += 1;
    Ok(())
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
    // Set to true when a `[]` suffix is encountered (S13c env-var-list).
    let mut list_suffix = false;

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
                // S8.6 (HOCON.md L270–276) also applies to unquoted path
                // segments inside ${...}: a segment beginning with '-' must be
                // followed by a digit. Gate on `!cur_started` so the check
                // fires only at **segment start** — a `-` that follows a
                // quoted fragment in the same segment (e.g. `${"a"-foo}`
                // resolving the key `"a-foo"` via quoted/unquoted concat) is
                // not policed, mirroring how the existing `${"a"x}` flow
                // builds `"ax"`. Digit-leading segments are not policed here
                // either (consistent with the value-position rule and
                // rs.hocon's unquoted-only token model — see
                // docs/spec-compliance.md §S8.6).
                if ch == '-' && !cur_started {
                    let next = chars.get(*pos + 1).copied().unwrap_or('\0');
                    if !next.is_ascii_digit() {
                        let after = if next == '\0' {
                            String::from("EOF")
                        } else {
                            format!("{:?}", next)
                        };
                        return Err(ParseError {
                            message: format!(
                                "unquoted path segment cannot begin with '-' unless followed by a digit (got '-' then {}, HOCON.md L270-276)",
                                after
                            ),
                            line: start_line,
                            col: *col,
                        });
                    }
                }
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
            '[' => {
                // S13c: `[]` suffix — end of path expression, start of list-suffix.
                // Two convergent multi-impl checks (mirrors go.hocon + ts.hocon fixes):
                //
                //   (a) Empty-segment guard: error if no segment has been started AND
                //       either there are no segments yet (`${[]}` / `${ []}`) or a
                //       trailing dot was just consumed (`${X.[]}` / `${X . []}`).
                //       Both reduce to `!cur_started` — uniform error.
                //   (b) E7 narrow: pending_ws may contain only ASCII SPACE (0x20) or
                //       TAB (0x09). Wider HOCON whitespace (NBSP, CR, Zs, BOM, …) is
                //       accumulated by the broader inter-token WS arm below (S6 set)
                //       but is rejected here for the `[` boundary per extra-spec E7
                //       ("narrow allow-list intentionally avoids semantic surprise").
                if !cur_started {
                    return Err(ParseError {
                        message: "empty segment before '[]' suffix in substitution".into(),
                        line: start_line,
                        col: *col,
                    });
                }
                for w in pending_ws.chars() {
                    if w != ' ' && w != '\t' {
                        return Err(ParseError {
                            message: format!(
                                "only ASCII space or tab allowed between substitution path and '[]' suffix (got {:?}, HOCON extra-spec E7)",
                                w
                            ),
                            line: start_line,
                            col: *col,
                        });
                    }
                }
                // Flush in-progress unquoted segment (same as the `}` path).
                segments.push(Segment {
                    text: std::mem::take(&mut cur_text),
                    line: cur_line,
                    col: cur_col,
                });
                cur_started = false;
                // E7-conformant pending_ws is intentionally discarded.
                pending_ws.clear();
                // Consume the literal `[]`.
                parse_literal_brackets(chars, pos, col, start_line)?;
                list_suffix = true;
                // After `[]` the only legal next char is `}`.
                if *pos >= chars.len() || chars[*pos] != '}' {
                    return Err(ParseError {
                        message: "expected '}' after '[]' in substitution".into(),
                        line: start_line,
                        col: *col,
                    });
                }
                *pos += 1;
                *col += 1;
                break;
            }
            ch if is_hocon_whitespace(ch) && !is_hocon_newline(ch) => {
                // Inter-token whitespace (full HOCON_WS minus LF): buffer into
                // pending_ws; column advances but line is unchanged.
                pending_ws.push(ch);
                *pos += 1;
                *col += 1;
            }
            '\n' => {
                // LF inside ${...} is not allowed (unterminated substitution).
                return Err(ParseError {
                    message: "unterminated substitution".into(),
                    line: start_line,
                    col: start_col,
                });
            }
            other => {
                return Err(ParseError {
                    message: format!(
                        "unexpected character in substitution path: {}",
                        other.escape_debug()
                    ),
                    line: start_line,
                    col: *col,
                });
            }
        }
    }

    // END validation (only reached via `}` break; `[]` break already pushes segment).
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
    } else if !list_suffix {
        // trailing dot: ${foo.} — report at the offending dot position.
        // Not an error when list_suffix=true; the `[]` arm already flushed.
        let (err_line, err_col) = last_dot.unwrap_or((start_line, start_col));
        return Err(ParseError {
            message: "empty segment in path".into(),
            line: err_line,
            col: err_col,
        });
    }

    Ok(SubstPayload {
        segments,
        optional,
        list_suffix,
    })
}

fn is_unquoted_start(ch: char) -> bool {
    if is_hocon_whitespace(ch) {
        return false;
    }
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
    if is_hocon_whitespace(ch) {
        return false;
    }
    if matches!(
        ch,
        '{' | '}'
            | '['
            | ']'
            | ','
            | ':'
            | '='
            | '#'
            | '"'
            | '$'
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

    // -------------------------------------------------------------------------
    // Spec compliance Phase 1 (issue #60): lexer-level rules.
    //
    // Each test is annotated with its xx.hocon spec checklist ID (S<n>.<m>).
    //
    // Convention for known spec violations:
    //   - The spec-correct test is annotated with #[ignore = "spec violation, see #NN"].
    //     CI stays green while the impl is buggy; removing the attribute once a fix
    //     lands flips the test to required-pass.
    //   - Where the ambiguity of it.fails()-equivalent is high (e.g., S6.x where
    //     a "fix" could plausibly reject or accept), a companion `_pin` test (no
    //     #[ignore]) asserts the *current* broken behavior as a regression net.
    // -------------------------------------------------------------------------

    // --- S2.3: comment markers inside quoted strings are literal -------------
    // Spec L126: "//" and "#" inside double-quoted strings must NOT be treated as
    // comment starters — they are literal string content.
    #[test]
    fn s2_3_comment_markers_inside_quoted_string_are_literal() {
        // "http://example.com" — the "//" must not start a comment
        let tokens = tokenize(r#""http://example.com""#).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::QuotedString);
        assert_eq!(tokens[0].value, "http://example.com");

        // "# not a comment" — the "#" must not start a comment
        let tokens = tokenize("\"# not a comment\"").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::QuotedString);
        assert_eq!(tokens[0].value, "# not a comment");
    }

    // --- S6.1: Unicode Zs / Zl / Zp category chars are whitespace -----------
    // Spec L170: the lexer must treat any Unicode whitespace category character
    // (Zs, Zl, Zp) as a token separator, not as unquoted string content.
    // All Zs/Zl/Zp members are covered by is_hocon_whitespace.
    //
    // Spec-correct test: em space must separate two unquoted tokens.
    #[test]
    fn s6_1_em_space_separates_tokens_spec() {
        let tokens = tokenize("a\u{2003}b").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "em space should separate two tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // Spec-correct test: line separator (U+2028, Zl) must be whitespace.
    #[test]
    fn s6_1_line_separator_separates_tokens_spec() {
        let tokens = tokenize("a\u{2028}b").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "U+2028 (Zl) should separate two tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // --- S6.2: non-breaking spaces are whitespace ----------------------------
    // Spec L171: U+00A0 (NBSP), U+2007 (figure space), U+202F (narrow NBSP)
    // must be treated as whitespace. All three are in is_hocon_whitespace.

    // Spec-correct test: NBSP (U+00A0) must separate tokens.
    #[test]
    fn s6_2_nbsp_separates_tokens_spec() {
        let tokens = tokenize("a\u{00A0}b").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "NBSP should separate two tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // Spec-correct test: figure space (U+2007) must separate tokens.
    #[test]
    fn s6_2_figure_space_separates_tokens_spec() {
        let tokens = tokenize("a\u{2007}b").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "figure space should separate two tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // Spec-correct test: narrow NBSP (U+202F) must separate tokens.
    #[test]
    fn s6_2_narrow_nbsp_separates_tokens_spec() {
        let tokens = tokenize("a\u{202F}b").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "narrow NBSP should separate two tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // --- S6.4: ASCII control whitespace --------------------------------------
    // Spec L174 lists 8 chars that are whitespace: tab (0x09), vtab (0x0B),
    // FF (0x0C), CR (0x0D), FS (0x1C), GS (0x1D), RS (0x1E), US (0x1F).
    // All 8 are now covered by is_hocon_whitespace.

    #[test]
    fn s6_4_tab_is_whitespace() {
        // Tab (0x09): in the HOCON whitespace set.
        let tokens = tokenize("a\tb").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2);
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    #[test]
    fn s6_4_cr_is_whitespace() {
        // CR (0x0D): in the HOCON whitespace set.
        // CR alone (without LF) acts as inter-token whitespace, not a newline emitter.
        let tokens = tokenize("a\rb").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2);
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // Spec-correct test: vtab (0x0B) must be whitespace.
    #[test]
    fn s6_4_vtab_is_whitespace_spec() {
        let tokens = tokenize("a\x0Bb").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "vtab should separate tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // Spec-correct test: form feed (0x0C) must be whitespace.
    #[test]
    fn s6_4_ff_is_whitespace_spec() {
        let tokens = tokenize("a\x0Cb").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 2, "FF should separate tokens");
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // Spec-correct test: FS, GS, RS, US (0x1C–0x1F) must be whitespace.
    // These are grouped because they share the same root cause (not in the
    // lexer's whitespace check) and the same fix will address all four.
    #[test]
    fn s6_4_fs_gs_rs_us_are_whitespace_spec() {
        for (label, ch) in [
            ("FS (0x1C)", '\x1C'),
            ("GS (0x1D)", '\x1D'),
            ("RS (0x1E)", '\x1E'),
            ("US (0x1F)", '\x1F'),
        ] {
            let input = format!("a{}b", ch);
            let tokens = tokenize(&input).unwrap();
            let unquoted: Vec<_> = tokens
                .iter()
                .filter(|t| t.kind == TokenKind::Unquoted)
                .collect();
            assert_eq!(unquoted.len(), 2, "{label} should separate tokens");
            assert_eq!(unquoted[0].value, "a", "{label}");
            assert_eq!(unquoted[1].value, "b", "{label}");
        }
    }

    // --- LF regression guard: LF must still emit Newline token ---------------
    // After predicate centralization, is_hocon_whitespace returns true for LF.
    // The newline branch must check BEFORE the whitespace skip so LF still
    // produces TokenKind::Newline (per spec §D, design invariant).
    #[test]
    fn s6_lf_still_emits_newline_token() {
        let tokens = tokenize("a\nb").unwrap();
        assert!(
            tokens.iter().any(|t| matches!(t.kind, TokenKind::Newline)),
            "LF must still emit a Newline token after whitespace predicate centralization"
        );
    }

    // --- S6.3 (broadened): BOM mid-stream is whitespace ----------------------
    // Spec L173: BOM (U+FEFF) is whitespace, not a start-of-input marker.
    // The lexer still strips BOM at char index 0 (harmless redundancy), and
    // BOM mid-stream is now consumed as inter-token whitespace via
    // is_hocon_whitespace.
    //
    // Spec-correct test: BOM mid-stream must separate two unquoted tokens.
    #[test]
    fn s6_3_bom_midstream_is_whitespace() {
        let tokens = tokenize("a\u{FEFF}b").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(
            unquoted.len(),
            2,
            "BOM mid-stream should separate two tokens"
        );
        assert_eq!(unquoted[0].value, "a");
        assert_eq!(unquoted[1].value, "b");
    }

    // --- S8.6 / E8: unquoted string begin rules (post-E8 amendment) ---------
    //
    // E8 amendment (xx.hocon#31 / commit dd102e8) reads HOCON.md L270-276
    // "begin" as value-position begin (first component of a concatenation),
    // not token-position begin at any lexer offset. At value-start:
    //   - the lexer reads the entire run as a single unquoted token (no
    //     separate number token kind); numeric coercion happens later in
    //     parse_scalar_value. Tokens that don't parse as numbers (e.g.
    //     `123abc`) stay as strings.
    //   - `-` not followed by a digit is treated as the start of an unquoted
    //     run (the strict reject at the lexer was removed per E8).
    // Path-element rules (substitution body, dotted key segments) remain
    // strict — covered in tests/s8_unquoted_starts.rs.

    #[test]
    fn e8_value_start_digit_leading_with_letters_is_string() {
        // `123abc` is not a valid number; parse_scalar_value falls back to
        // ScalarType::String. Same observable behavior as Lightbend (whose
        // parseLong/parseFloat both fail and produce an unquoted concat).
        // Assert the resolved value (not just is_ok) so accidental coercion
        // or truncation would surface here.
        let cfg = crate::parse("x = 123abc").expect("parse failed");
        assert_eq!(
            cfg.get_string("x").expect("x not found"),
            "123abc",
            "E8: `123abc` must lex+resolve as unquoted string \"123abc\""
        );
    }

    #[test]
    fn e8_value_start_hyphen_leading_non_number_is_string() {
        // Pre-E8 this was a lex error (S8.6 strict reading). Post-E8, `-foo`
        // is an unquoted string at value-position — RFC 8259 JSON-number
        // requires a digit after `-`, so bare `-foo` falls outside L270's
        // disallow scope. Lightbend reference produces `{"x":"-foo"}`.
        // Assert the resolved value (not just is_ok) so accidental coercion
        // or truncation would surface here.
        let cfg = crate::parse("x = -foo").expect("parse failed");
        assert_eq!(
            cfg.get_string("x").expect("x not found"),
            "-foo",
            "E8: `-foo` must lex+resolve as unquoted string \"-foo\""
        );
    }

    // --- S8.7: no escape sequences in unquoted strings -----------------------
    // Spec L253: unquoted strings do not interpret any escape sequences.
    // A backslash inside an unquoted run is forbidden (it terminates the run
    // in rs.hocon because '\' is excluded from is_unquoted_start and
    // is_unquoted_continue), and the bare backslash produces a lexer error.
    #[test]
    fn s8_7_backslash_is_rejected_in_unquoted_context() {
        // "a\n" outside quotes: the lexer reads 'a' as unquoted, then hits '\',
        // which is not a valid unquoted character and not a recognised token
        // introducer — the lexer should error.
        assert!(
            tokenize(r"a\n").is_err(),
            "bare backslash outside quotes must be rejected"
        );
    }

    // --- S8.8: unquoted strings allow control chars except forbidden set -----
    // Spec L280: control characters OTHER than the forbidden set (L245:
    // $ " { } [ ] : = , + # ` ^ ? ! @ * & \ and whitespace are permitted
    // inside unquoted strings.
    #[test]
    fn s8_8_soh_allowed_in_unquoted_string() {
        // SOH (0x01) is a control character not in the forbidden set.
        let tokens = tokenize("foo\x01bar").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 1);
        assert_eq!(unquoted[0].value, "foo\x01bar");
    }

    #[test]
    fn s8_8_bel_allowed_in_unquoted_string() {
        // BEL (0x07) is a control character not in the forbidden set.
        let tokens = tokenize("foo\x07bar").unwrap();
        let unquoted: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Unquoted)
            .collect();
        assert_eq!(unquoted.len(), 1);
        assert_eq!(unquoted[0].value, "foo\x07bar");
    }
}
