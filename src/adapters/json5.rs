//! JSON5 (<https://json5.org>, spec 1.0.0) as HOCON config.
//!
//! Unlike the jsonc adapter — which strips comments and hands the rest to
//! `serde_json` — JSON5 changes the token grammar itself (unquoted identifier
//! keys, single-quoted strings with line continuations, hex integers, leading
//! and trailing decimal points, an explicit plus sign), so this module is a
//! hand-rolled scanner and recursive-descent parser. Zero extra dependencies.
//!
//! The accepted grammar is JSON5 1.0.0 as defined by the reference
//! implementation (the json5 npm package), the dialect owner this spec item
//! tracks — the same ownership rule F3.2 applies to JSONC. Where the mapping
//! spec is stricter than JSON5, the spec wins:
//!
//! - Infinity and NaN (signed or bare) are errors, not values (spec F0.6).
//! - Integers — decimal or hex — must fit in i64 (spec F0.5); floats decode
//!   as f64. A number written with `.`, `e` or `E` is a float, all other
//!   decimal forms and every hex form are integers.
//! - An unpaired `\uXXXX` surrogate is an error, and a valid pair combines
//!   into the astral codepoint (spec F3.5).
//! - Duplicate keys follow HOCON semantics: objects merge, otherwise the
//!   later value wins (spec F0.7).
//! - The document holds exactly one value; whitespace and comments may follow
//!   it, anything else is an error (the F3.2 strictness rule).
//!
//! One divergence this implementation adds, shared with the crate's core
//! parser rather than with the go adapter: nesting is capped at
//! [`MAX_DOCUMENT_DEPTH`] levels, because a Rust stack overflow is `SIGABRT`
//! and cannot be converted into an `Err` after the fact.
//!
//! See the F3.x items in the format-ingestion mapping spec:
//! <https://github.com/o3co/xx.hocon/blob/main/docs/format-ingestion-mapping.md>

use indexmap::IndexMap;

use super::{config_from_object, AdapterError};
use crate::depth::MAX_DOCUMENT_DEPTH;
use crate::value::{HoconValue, ScalarValue};
use crate::Config;

/// Read JSON5 text. `origin` names the source in error messages; `None`
/// reports it as "document".
pub fn parse(input: &str, origin: Option<&str>) -> Result<Config, AdapterError> {
    // F0.9: a leading BOM is not data. (JSON5 additionally treats U+FEFF as
    // whitespace anywhere, which the scanner handles; stripping here keeps
    // the origin column of the first token honest.)
    let src = super::strip_bom(input);
    let mut p = Parser {
        src,
        pos: 0,
        line: 0,
        line_at: 0,
    };
    let doc = p
        .parse_document()
        .map_err(|msg| adapter_err(origin, &msg))?;
    match doc {
        HoconValue::Object(_) => Ok(config_from_object(doc, origin)),
        // F0.3: the root has to be an object, like every other adapter.
        _ => Err(adapter_err(
            origin,
            "document root must be an object (spec F0.3)",
        )),
    }
}

/// Read a JSON5 file, using its path as the origin description.
pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Config, AdapterError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::new(format!("json5: {}: {e}", path.display())))?;
    parse(&text, Some(&path.display().to_string()))
}

fn adapter_err(origin: Option<&str>, msg: &str) -> AdapterError {
    AdapterError::new(format!("json5: {}: {msg}", origin.unwrap_or("document")))
}

// ---------------------------------------------------------------------------
// Scanner / parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    src: &'a str,
    pos: usize,     // byte offset; always a char boundary by construction
    line: usize,    // 0-based; reported 1-based
    line_at: usize, // byte offset where the current line starts
}

/// The JSON5 LineTerminator set: LF, CR, LS, PS. This deliberately differs
/// from JSONC (F3.2), whose dialect owner ends `//` comments at LF/CR only —
/// the JSON5 spec includes LS and PS.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// The JSON5 WhiteSpace set: TAB, VT, FF, SP, NBSP, BOM, and any Unicode Zs
/// character.
///
/// std has no Zs predicate, but `char::is_whitespace` is the Unicode
/// `White_Space` property, and `White_Space` = Zs ∪ {TAB, LF, VT, FF, CR,
/// NEL, LS, PS} exactly — so subtracting the enumerated non-Zs members gives
/// the precise Zs set, no approximation. NEL (U+0085) is neither JSON5
/// whitespace nor a JSON5 line terminator, so it must fall out here and
/// surface as an unexpected character.
fn is_json5_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\u{000b}' | '\u{000c}' | ' ' | '\u{00a0}' | '\u{feff}'
    ) || (c.is_whitespace() && !matches!(c, '\n' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}'))
}

/// ES5 IdentifierName characters, the key grammar the JSON5 spec adopts:
/// start = UnicodeLetter (Lu Ll Lt Lm Lo Nl) | `$` | `_`; continue adds
/// Mn Mc Nd Pc and ZWNJ/ZWJ.
///
/// The crate carries no Unicode category tables and this adapter deliberately
/// adds no dependency, so the checks are built from std's char methods, which
/// expose Unicode *properties* rather than general categories. The deviations
/// from the exact ES5 sets (which the go implementation gets from
/// `unicode.In`) are:
///
/// - `char::is_alphabetic` is the `Alphabetic` property =
///   Lu∪Ll∪Lt∪Lm∪Lo∪Nl ∪ Other_Alphabetic. The first six are exactly ES5's
///   letter set, so the divergence is Other_Alphabetic — a few hundred
///   combining marks (e.g. U+0345, Indic matras). Those are Mn/Mc and hence
///   legal *continue* characters anyway; the lenient divergence is accepting
///   them as the *first* character of a key, which go rejects.
/// - `char::is_numeric` is Nd∪Nl∪No. Nd is what ES5 wants and Nl is already a
///   legal letter; the lenient divergence is accepting No characters (e.g.
///   `²`, `½`) in the continue position, which go rejects.
/// - Pc is ten stable codepoints, matched exactly (`_` via the start set).
/// - Mn/Mc marks *outside* Other_Alphabetic — e.g. the plain combining
///   accents U+0300..U+036F — are **rejected** in the continue position where
///   go accepts them: the one strict divergence. A key that needs one can be
///   quoted.
///
/// Every deviation therefore affects only which *unquoted* keys are accepted,
/// never how an accepted document decodes.
fn is_ident_start(c: char) -> bool {
    c == '$' || c == '_' || c.is_alphabetic()
}

fn is_ident_part(c: char) -> bool {
    is_ident_start(c)
        || c == '\u{200c}'
        || c == '\u{200d}'
        || c.is_numeric()
        || matches!(
            c,
            // Pc (connector punctuation) minus '_', in full.
            '\u{203f}'
                | '\u{2040}'
                | '\u{2054}'
                | '\u{fe33}'
                | '\u{fe34}'
                | '\u{fe4d}'
                | '\u{fe4e}'
                | '\u{fe4f}'
                | '\u{ff3f}'
        )
}

/// Whether `s` continues with an identifier character at byte offset `i` —
/// used to reject tokens like `nullx` and to give `Infinityx` the ordinary
/// unexpected-token error rather than the F0.6 one.
fn continues_identifier(s: &str, i: usize) -> bool {
    s[i..].chars().next().is_some_and(is_ident_part)
}

/// Merge `src` over `dst` per HOCON duplicate-key semantics (F0.7), returning
/// a new map.
fn merge_objects(
    dst: &IndexMap<String, HoconValue>,
    src: &IndexMap<String, HoconValue>,
) -> IndexMap<String, HoconValue> {
    let mut out = dst.clone();
    for (k, v) in src {
        let merged = match (out.get(k), v) {
            (Some(HoconValue::Object(po)), HoconValue::Object(vo)) => {
                HoconValue::Object(merge_objects(po, vo))
            }
            _ => v.clone(),
        };
        out.insert(k.clone(), merged);
    }
    out
}

impl<'a> Parser<'a> {
    fn err(&self, msg: impl std::fmt::Display) -> String {
        let col = self.src[self.line_at..self.pos].chars().count() + 1;
        format!("line {} col {}: {msg}", self.line + 1, col)
    }

    /// The char at `pos`, without advancing.
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    /// The byte at offset `pos + ahead`, if any.
    fn byte_at(&self, ahead: usize) -> Option<u8> {
        self.src.as_bytes().get(self.pos + ahead).copied()
    }

    fn advance(&mut self, c: char) {
        self.pos += c.len_utf8();
        if is_line_terminator(c) {
            // Treat CRLF as one terminator for line counting.
            if c == '\r' && self.byte_at(0) == Some(b'\n') {
                self.pos += 1;
            }
            self.line += 1;
            self.line_at = self.pos;
        }
    }

    /// Parse exactly one JSON5 value, allowing only whitespace and comments
    /// after it (the F3.2 strictness rule).
    fn parse_document(&mut self) -> Result<HoconValue, String> {
        self.skip_space()?;
        let v = self.parse_value(0)?;
        self.skip_space()?;
        if self.pos < self.src.len() {
            return Err(self.err("unexpected content after top-level value"));
        }
        Ok(v)
    }

    /// Consume whitespace, line terminators, and both comment forms.
    fn skip_space(&mut self) -> Result<(), String> {
        while let Some(c) = self.peek() {
            if is_json5_space(c) || is_line_terminator(c) {
                self.advance(c);
            } else if c == '/' && self.byte_at(1) == Some(b'/') {
                self.pos += 2;
                // A LS or PS terminates a // comment too (unlike JSONC).
                while let Some(c2) = self.peek() {
                    if is_line_terminator(c2) {
                        break;
                    }
                    self.pos += c2.len_utf8();
                }
            } else if c == '/' && self.byte_at(1) == Some(b'*') {
                self.pos += 2;
                let mut closed = false;
                while let Some(c2) = self.peek() {
                    if c2 == '*' && self.byte_at(1) == Some(b'/') {
                        self.pos += 2;
                        closed = true;
                        break;
                    }
                    self.advance(c2);
                }
                if !closed {
                    return Err(self.err("unterminated /* comment"));
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    fn parse_value(&mut self, depth: usize) -> Result<HoconValue, String> {
        let Some(c) = self.peek() else {
            return Err(self.err("unexpected end of input, expected a value"));
        };
        match c {
            '{' => self.parse_object(depth),
            '[' => self.parse_array(depth),
            '"' | '\'' => Ok(HoconValue::Scalar(ScalarValue::string(
                self.parse_string(c)?,
            ))),
            '+' | '-' | '.' | '0'..='9' => self.parse_number(),
            _ => self.parse_keyword(),
        }
    }

    /// Handle true/false/null and reject Infinity/NaN by name so the error
    /// explains itself (spec F0.6).
    fn parse_keyword(&mut self) -> Result<HoconValue, String> {
        let rest = &self.src[self.pos..];
        for (kw, v) in [
            ("true", HoconValue::Scalar(ScalarValue::boolean(true))),
            ("false", HoconValue::Scalar(ScalarValue::boolean(false))),
            ("null", HoconValue::Scalar(ScalarValue::null())),
        ] {
            if rest.starts_with(kw) && !continues_identifier(rest, kw.len()) {
                self.pos += kw.len();
                return Ok(v);
            }
        }
        for kw in ["Infinity", "NaN"] {
            if rest.starts_with(kw) && !continues_identifier(rest, kw.len()) {
                return Err(self.err(format!(
                    "{kw} is not representable in the HOCON number model (spec F0.6)"
                )));
            }
        }
        let c = rest.chars().next().expect("parse_value checked non-empty");
        Err(self.err(format!("unexpected character {c:?}")))
    }

    /// Refuse to open a container beyond [`MAX_DOCUMENT_DEPTH`] — the same
    /// cap and reasoning as the core parser: exhausting the stack in Rust
    /// aborts the process rather than raising.
    fn check_depth(&self, depth: usize) -> Result<(), String> {
        if depth >= MAX_DOCUMENT_DEPTH {
            return Err(self.err(format!(
                "document nests deeper than {MAX_DOCUMENT_DEPTH} levels; this \
                 limit exists because exhausting the stack in Rust aborts the \
                 process rather than raising"
            )));
        }
        Ok(())
    }

    fn parse_object(&mut self, depth: usize) -> Result<HoconValue, String> {
        self.check_depth(depth)?;
        self.pos += 1; // '{'
        let mut obj: IndexMap<String, HoconValue> = IndexMap::new();
        loop {
            self.skip_space()?;
            let Some(c) = self.peek() else {
                return Err(self.err("unterminated object, expected '}'"));
            };
            if c == '}' {
                self.pos += 1;
                return Ok(HoconValue::Object(obj));
            }
            let key = self.parse_member_name()?;
            self.skip_space()?;
            if self.peek() != Some(':') {
                return Err(self.err(format!("expected ':' after object key {key:?}")));
            }
            self.pos += 1;
            self.skip_space()?;
            let mut val = self.parse_value(depth + 1)?;
            // F0.7: duplicate keys follow HOCON semantics — two objects
            // merge, any other combination is last-wins.
            if let (Some(HoconValue::Object(po)), HoconValue::Object(vo)) = (obj.get(&key), &val) {
                val = HoconValue::Object(merge_objects(po, vo));
            }
            obj.insert(key, val);
            self.skip_space()?;
            match self.peek() {
                None => return Err(self.err("unterminated object, expected ',' or '}'")),
                Some(',') => self.pos += 1, // trailing comma before '}' is legal
                Some('}') => {
                    self.pos += 1;
                    return Ok(HoconValue::Object(obj));
                }
                Some(_) => return Err(self.err("expected ',' or '}' in object")),
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<HoconValue, String> {
        self.check_depth(depth)?;
        self.pos += 1; // '['
        let mut arr: Vec<HoconValue> = Vec::new();
        loop {
            self.skip_space()?;
            let Some(c) = self.peek() else {
                return Err(self.err("unterminated array, expected ']'"));
            };
            if c == ']' {
                self.pos += 1;
                return Ok(HoconValue::Array(arr));
            }
            arr.push(self.parse_value(depth + 1)?);
            self.skip_space()?;
            match self.peek() {
                None => return Err(self.err("unterminated array, expected ',' or ']'")),
                Some(',') => self.pos += 1, // trailing comma before ']' is legal
                Some(']') => {
                    self.pos += 1;
                    return Ok(HoconValue::Array(arr));
                }
                Some(_) => return Err(self.err("expected ',' or ']' in array")),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Member names (quoted or ES5 IdentifierName)
    // -----------------------------------------------------------------------

    fn parse_member_name(&mut self) -> Result<String, String> {
        match self.peek() {
            Some(c @ ('"' | '\'')) => self.parse_string(c),
            _ => self.parse_identifier(),
        }
    }

    /// Scan an ES5 IdentifierName, honouring `\uXXXX` escapes in the name
    /// (the escaped codepoint must itself be a legal identifier character for
    /// its position, per ES5 — `1` cannot start a key; a surrogate escape is
    /// never a legal identifier character).
    fn parse_identifier(&mut self) -> Result<String, String> {
        let mut out = String::new();
        let mut first = true;
        while let Some(c) = self.peek() {
            let (ch, escaped) = if c == '\\' {
                if self.byte_at(1) != Some(b'u') {
                    return Err(self.err("only \\uXXXX escapes are allowed in identifiers"));
                }
                self.pos += 2;
                let cp = self.read_hex4()?;
                match char::from_u32(cp) {
                    Some(ch) => (ch, true),
                    // A surrogate half: not a char, and never a legal
                    // identifier character in go either.
                    None => {
                        return Err(self.err(format!(
                            "escape \\u{cp:04X} is not a valid identifier character here"
                        )))
                    }
                }
            } else {
                (c, false)
            };
            let legal = if first {
                is_ident_start(ch)
            } else {
                is_ident_part(ch)
            };
            if !legal {
                if escaped {
                    return Err(self.err(format!(
                        "escape \\u{:04X} is not a valid identifier character here",
                        ch as u32
                    )));
                }
                if first {
                    return Err(self.err(format!("expected an object key, got {ch:?}")));
                }
                break;
            }
            out.push(ch);
            if !escaped {
                self.pos += ch.len_utf8();
            }
            first = false;
        }
        if out.is_empty() {
            return Err(self.err("expected an object key"));
        }
        Ok(out)
    }

    /// Read exactly four hex digits at `pos` and return the codepoint value
    /// (which may be a surrogate half — the callers decide what that means).
    fn read_hex4(&mut self) -> Result<u32, String> {
        let bytes = &self.src.as_bytes()[self.pos..];
        if bytes.len() < 4 {
            return Err(self.err("truncated \\u escape"));
        }
        let mut n: u32 = 0;
        for &b in &bytes[..4] {
            let Some(d) = (b as char).to_digit(16) else {
                let quad = String::from_utf8_lossy(&bytes[..4]);
                return Err(self.err(format!("invalid \\u escape {:?}", format!("\\u{quad}"))));
            };
            n = n * 16 + d;
        }
        self.pos += 4;
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // Strings
    // -----------------------------------------------------------------------

    /// Scan a single- or double-quoted JSON5 string; `quote` is the opening
    /// quote. JSON5 differences from JSON: either quote character, `\xHH`
    /// escapes, `\v`, `\0`, line continuations (backslash before a line
    /// terminator, including CRLF as one), any other non-digit character
    /// escaping to itself, and unescaped LS/PS allowed inside the string.
    fn parse_string(&mut self, quote: char) -> Result<String, String> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.err("unterminated string"));
            };
            match c {
                _ if c == quote => {
                    self.pos += 1;
                    return Ok(out);
                }
                '\n' | '\r' => return Err(self.err("unescaped line terminator in string")),
                '\\' => {
                    self.pos += 1;
                    self.read_escape(&mut out)?;
                }
                // LS/PS are legal unescaped inside JSON5 strings.
                _ => {
                    out.push(c);
                    self.advance(c);
                }
            }
        }
    }

    /// Consume one escape sequence (the backslash is already consumed) and
    /// append its value to `out`.
    fn read_escape(&mut self, out: &mut String) -> Result<(), String> {
        let Some(c) = self.peek() else {
            return Err(self.err("unterminated escape sequence"));
        };
        // Line continuation: backslash before a line terminator joins the
        // lines, contributing nothing. CRLF counts as one terminator.
        if is_line_terminator(c) {
            self.advance(c);
            return Ok(());
        }
        match c {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000c}'),
            'v' => out.push('\u{000b}'),
            '0' => {
                // \0 is NUL unless followed by a decimal digit (octal escapes
                // are not part of JSON5).
                if matches!(self.byte_at(1), Some(b'0'..=b'9')) {
                    return Err(self.err("octal escape sequences are not allowed"));
                }
                out.push('\0');
            }
            '1'..='9' => {
                return Err(self.err(format!(
                    "escape \\{c} is not allowed (digits cannot be escaped)"
                )))
            }
            'x' => {
                self.pos += 1;
                if self.pos + 2 > self.src.len() {
                    return Err(self.err("truncated \\x escape"));
                }
                let hi = (self.src.as_bytes()[self.pos] as char).to_digit(16);
                let lo = (self.src.as_bytes()[self.pos + 1] as char).to_digit(16);
                let (Some(hi), Some(lo)) = (hi, lo) else {
                    return Err(self.err("invalid \\x escape"));
                };
                self.pos += 2;
                out.push(char::from_u32(hi * 16 + lo).expect("\\xHH is at most U+00FF"));
                return Ok(());
            }
            'u' => {
                self.pos += 1;
                let cp = self.read_hex4()?;
                // F3.5: a lone surrogate is an error; a valid pair combines.
                if (0xD800..=0xDFFF).contains(&cp) {
                    if self.pos + 6 <= self.src.len()
                        && self.byte_at(0) == Some(b'\\')
                        && self.byte_at(1) == Some(b'u')
                    {
                        self.pos += 2;
                        let lo = self.read_hex4()?;
                        if (0xD800..=0xDBFF).contains(&cp) && (0xDC00..=0xDFFF).contains(&lo) {
                            let combined = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            out.push(char::from_u32(combined).expect("valid astral codepoint"));
                            return Ok(());
                        }
                    }
                    return Err(self.err(format!("unpaired \\u{cp:04X} surrogate (spec F3.5)")));
                }
                out.push(char::from_u32(cp).expect("non-surrogate BMP codepoint"));
                return Ok(());
            }
            // Any other character escapes to itself (JSON5's
            // SingleEscapeCharacter and NonEscapeCharacter collapse to this).
            _ => {
                out.push(c);
                self.advance(c);
                return Ok(());
            }
        }
        self.pos += 1; // the single-character escapes above
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Numbers
    // -----------------------------------------------------------------------

    /// Scan a JSON5 numeric literal: optional sign, then a hex integer or a
    /// decimal with optional leading/trailing point and exponent. Signed
    /// Infinity/NaN are routed to the F0.6 error here.
    fn parse_number(&mut self) -> Result<HoconValue, String> {
        let src = self.src;
        let bytes = src.as_bytes();
        let start = self.pos;
        let mut neg = false;
        if matches!(bytes[self.pos], b'+' | b'-') {
            neg = bytes[self.pos] == b'-';
            self.pos += 1;
        }
        let rest = &src[self.pos..];
        for kw in ["Infinity", "NaN"] {
            if rest.starts_with(kw) && !continues_identifier(rest, kw.len()) {
                return Err(self.err(format!(
                    "{kw} is not representable in the HOCON number model (spec F0.6)"
                )));
            }
        }

        if rest.starts_with("0x") || rest.starts_with("0X") {
            self.pos += 2;
            let ds = self.pos;
            while self.pos < bytes.len() && bytes[self.pos].is_ascii_hexdigit() {
                self.pos += 1;
            }
            if self.pos == ds {
                return Err(self.err("hex literal needs at least one digit"));
            }
            let limit: u64 = if neg { 1 << 63 } else { (1 << 63) - 1 };
            let Some(mag) = u64::from_str_radix(&src[ds..self.pos], 16)
                .ok()
                .filter(|m| *m <= limit)
            else {
                return Err(self.err(format!(
                    "integer {} does not fit in i64 (spec F0.5)",
                    &src[start..self.pos]
                )));
            };
            // −mag is exactly representable for every mag ≤ 2^63: 2^63 as
            // i64 wraps to i64::MIN, which is the value -0x8000000000000000
            // denotes, and wrapping_neg leaves it in place.
            let value = if neg {
                (mag as i64).wrapping_neg()
            } else {
                mag as i64
            };
            return Ok(HoconValue::Scalar(ScalarValue::number(value.to_string())));
        }

        let (mut saw_digit, mut saw_dot, mut saw_exp) = (false, false, false);
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b'0'..=b'9' => saw_digit = true,
                b'.' if !saw_dot && !saw_exp => saw_dot = true,
                b'e' | b'E' if saw_digit && !saw_exp => {
                    saw_exp = true;
                    if matches!(bytes.get(self.pos + 1), Some(b'+' | b'-')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
            self.pos += 1;
        }
        let text = &src[start..self.pos];
        if !saw_digit {
            return Err(self.err(format!("malformed number {text:?}")));
        }
        // F0.5: '.', 'e', 'E' make a float; everything else is an i64 or an
        // error. The leading '+' is stripped for str::parse's sake.
        let unsigned = text.strip_prefix('+').unwrap_or(text);
        if saw_dot || saw_exp {
            let f: f64 = unsigned
                .parse()
                .map_err(|_| self.err(format!("malformed number {text:?}")))?;
            // Go's strconv.ParseFloat reports out-of-range values as errors;
            // Rust's parse saturates to ±Inf on overflow and to 0 on
            // underflow, so both conditions are re-checked here to keep the
            // implementations agreeing. Underflow = a mantissa with a
            // significant digit that still came out as zero.
            let mantissa = text.split(['e', 'E']).next().unwrap_or(text);
            let underflow = f == 0.0 && mantissa.bytes().any(|b| b.is_ascii_digit() && b != b'0');
            if !f.is_finite() || underflow {
                return Err(self.err(format!("malformed number {text:?}")));
            }
            // {:?} keeps a whole-valued float visibly a float ("1000.0"),
            // where {} would render "1000".
            return Ok(HoconValue::Scalar(ScalarValue::number(format!("{f:?}"))));
        }
        let i: i64 = unsigned
            .parse()
            .map_err(|_| self.err(format!("integer {text} does not fit in i64 (spec F0.5)")))?;
        Ok(HoconValue::Scalar(ScalarValue::number(i.to_string())))
    }
}
