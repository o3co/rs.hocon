//! JSON with comments and trailing commas — the dialect VS Code and TypeScript
//! use for their config files — as HOCON config.
//!
//! Plain JSON needs no adapter: HOCON is a JSON superset, so `hocon::parse`
//! already accepts a `.json` file. This exists for the two things HOCON does
//! not accept, block comments and trailing commas.
//!
//! Comments are **token-separating** (spec F3.2): a comment is replaced by
//! whitespace, never deleted, so it can never splice its neighbors into one
//! token. `{"a": 1/*x*/2}` is a syntax error rather than the number `12`.

use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use super::{config_from_object, AdapterError};
use crate::value::{HoconValue, ScalarValue};
use crate::Config;

/// Read JSONC text.
pub fn parse(input: &str, origin: Option<&str>) -> Result<Config, AdapterError> {
    let cleaned = strip_trailing_commas(&strip_comments(super::strip_bom(input))?);
    let doc: JsonValue =
        serde_json::from_str(&cleaned).map_err(|e| AdapterError::new(format!("jsonc: {e}")))?;
    if !doc.is_object() {
        return Err(AdapterError::new(
            "jsonc: document root must be an object (spec F0.3)",
        ));
    }
    Ok(config_from_object(convert(&doc, "")?, origin))
}

/// Read a JSONC file, using its path as the origin description.
pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Config, AdapterError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::new(format!("jsonc: {}: {e}", path.display())))?;
    parse(&text, Some(&path.display().to_string()))
}

fn convert(v: &JsonValue, at: &str) -> Result<HoconValue, AdapterError> {
    match v {
        JsonValue::Object(m) => {
            let mut out: IndexMap<String, HoconValue> = IndexMap::new();
            for (k, e) in m {
                let path = if at.is_empty() {
                    k.clone()
                } else {
                    format!("{at}.{k}")
                };
                out.insert(k.clone(), convert(e, &path)?);
            }
            Ok(HoconValue::Object(out))
        }
        JsonValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, e) in items.iter().enumerate() {
                out.push(convert(e, &format!("{at}[{i}]"))?);
            }
            Ok(HoconValue::Array(out))
        }
        JsonValue::String(s) => Ok(HoconValue::Scalar(ScalarValue::string(s.clone()))),
        JsonValue::Bool(b) => Ok(HoconValue::Scalar(ScalarValue::boolean(*b))),
        JsonValue::Null => Ok(HoconValue::Scalar(ScalarValue::null())),
        // serde_json keeps the integer/float distinction, so F0.5's rule holds
        // without re-reading the source text.
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(HoconValue::Scalar(ScalarValue::number(i.to_string())))
            } else if let Some(f) = n.as_f64() {
                if f.is_nan() || f.is_infinite() {
                    return Err(AdapterError::new(format!(
                        "jsonc: at {at}: {f} is not representable in HOCON (spec F0.6)"
                    )));
                }
                Ok(HoconValue::Scalar(ScalarValue::number(n.to_string())))
            } else {
                Err(AdapterError::new(format!(
                    "jsonc: at {at}: {n} does not fit in i64 (spec F0.5)"
                )))
            }
        }
    }
}

/// Remove `//` line comments and block comments, leaving string literals
/// alone.
///
/// Two invariants, both load-bearing:
///
/// 1. **A comment becomes whitespace, never the empty string.** Erasing it
///    outright would splice its neighbours into one token (`1/*x*/2` → `12`,
///    which is valid JSON), so each block comment leaves at least one space
///    behind (spec F3.2). A `//` comment keeps its terminator, which already
///    separates tokens.
/// 2. **Line structure is preserved exactly.** Every line terminator inside a
///    removed span is re-emitted verbatim and in order, so a `\r\n` survives
///    as a pair and a lone `\r` survives as itself. The stripped text
///    therefore has the same lines as the source and a decoder's reported
///    line numbers point at the right source line.
///
/// Column offsets within a line are deliberately *not* preserved — a comment
/// body collapses to a single space — so only line numbers are meaningful,
/// which is the same trade `//` stripping has always made.
fn strip_comments(src: &str) -> Result<String, AdapterError> {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            '"' => {
                let end = end_of_string(&b, i)?;
                out.extend(&b[i..end]);
                i = end;
            }
            '/' if i + 1 < b.len() && b[i + 1] == '/' => {
                // A lone CR ends the comment too: a classic-Mac or otherwise
                // CR-delimited file would otherwise have the rest of the
                // document swallowed by the first `//` (spec F3.2). The
                // terminator itself is left in place, so it still separates
                // tokens.
                while i < b.len() && b[i] != '\n' && b[i] != '\r' {
                    i += 1;
                }
            }
            '/' if i + 1 < b.len() && b[i + 1] == '*' => {
                let mut j = i + 2;
                loop {
                    if j + 1 >= b.len() {
                        return Err(AdapterError::new("jsonc: unterminated block comment"));
                    }
                    if b[j] == '*' && b[j + 1] == '/' {
                        break;
                    }
                    // Re-emit terminators verbatim so a `\r\n` stays a pair
                    // and a lone `\r` is not silently dropped. Keeping only
                    // `\n` collapsed CRLF and lost classic-Mac line breaks
                    // outright, which contradicted the invariant above.
                    if b[j] == '\n' || b[j] == '\r' {
                        out.push(b[j]);
                    }
                    j += 1;
                }
                out.push(' ');
                i = j + 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

fn end_of_string(b: &[char], i: usize) -> Result<usize, AdapterError> {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            '\\' => j += 2,
            '"' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err(AdapterError::new("jsonc: unterminated string literal"))
}

/// Drop a comma whose next meaningful character closes its object or array.
fn strip_trailing_commas(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == '"' {
            match end_of_string(&b, i) {
                Ok(end) => {
                    out.extend(&b[i..end]);
                    i = end;
                    continue;
                }
                Err(_) => {
                    out.extend(&b[i..]);
                    return out;
                }
            }
        }
        if b[i] == ',' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            if j < b.len() && (b[j] == '}' || b[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adversarial corpus behind issue #155.
    ///
    /// The security review on #154 ran 36 hostile inputs through the strip
    /// passes and found no panic, and that result is the reason the passes
    /// still build a `Vec<char>`: indexing chars makes a non-char-boundary
    /// slice structurally impossible. The inputs themselves were never
    /// committed, so the evidence lived only in a sentence in the issue —
    /// which is worth exactly nothing to the next person who rewrites this.
    ///
    /// This is that corpus, rebuilt from the eight categories the issue names
    /// (unterminated block comments, multibyte characters straddling comment
    /// boundaries, comment markers inside strings, escaped quotes at string
    /// end, BOM, CRLF, U+2028, deep nesting) and extended where reading the
    /// code suggested a neighbouring shape. It is a reconstruction, not the
    /// original list — the original is not recoverable — so the count differs
    /// and the ids are ours.
    ///
    /// The bar every entry must clear is the one the review established:
    /// **`parse` returns, one way or the other.** `Ok` and `Err` are both
    /// acceptable answers for a hostile input; a panic is not, because
    /// `parse_file` is public and a panic in a config loader takes the process
    /// with it.
    const ADVERSARIAL: &[(&str, &str)] = &[
        // --- unterminated block comments ---
        ("ub01-bare", "/*"),
        ("ub02-after-doc", "{\"a\":1} /*"),
        ("ub03-open-brace", "{/*"),
        ("ub04-mid-value", "{\"a\": /*x"),
        ("ub05-slash-star-slash", "/*/"),
        ("ub06-star-at-eof", "{\"a\":1} /*x*"),
        // --- unterminated strings, including an escape that runs off the end ---
        ("us01-bare-quote", "\""),
        ("us02-in-key", "{\"a"),
        ("us03-in-value", "{\"a\": \"b"),
        ("us04-trailing-backslash", "{\"a\": \"b\\"),
        ("us05-escape-eats-quote", "{\"a\": \"b\\\""),
        // --- escaped quotes and backslashes at a string boundary ---
        ("eq01-escaped-quote", "{\"a\": \"b\\\"\"}"),
        ("eq02-escaped-backslash", "{\"a\": \"b\\\\\"}"),
        ("eq03-double-escaped", "{\"a\": \"b\\\\\\\\\"}"),
        ("eq04-escape-in-key", "{\"a\\\\\": 1}"),
        (
            "eq05-escaped-quote-then-comment",
            "{\"a\": \"b\\\"\" /*c*/}",
        ),
        // --- comment markers that are data, not comments ---
        ("cm01-line-in-string", "{\"a\": \"//x\"}"),
        ("cm02-block-in-string", "{\"a\": \"/*x*/\"}"),
        ("cm03-closer-in-string", "{\"a\": \"*/\"}"),
        ("cm04-marker-as-key", "{\"//\": 1}"),
        ("cm05-quoted-quote-then-marker", "{\"a\": \"\\\"/*\\\"\"}"),
        ("cm06-opener-only-in-string", "{\"a\": \"/*\"}"),
        // --- multibyte characters straddling a comment or string boundary ---
        ("mb01-after-block", "{\"a\": 1 /*\u{3042}*/}"),
        ("mb02-before-value", "{\"a\": /*\u{3042}*/ 1}"),
        ("mb03-in-key", "{\"\u{3042}\": 1}"),
        (
            "mb04-around-marker-in-string",
            "{\"a\": \"\u{3042}/*b*/\u{3044}\"}",
        ),
        ("mb05-unterminated-block", "/*\u{3042}"),
        ("mb06-astral", "{\"a\": \"\u{1f600}\" /*\u{1f600}*/}"),
        ("mb07-astral-unterminated", "{\"a\": \"\u{1f600}"),
        // --- BOM (F0.9) ---
        ("bo01-doc", "\u{feff}{\"a\":1}"),
        ("bo02-alone", "\u{feff}"),
        ("bo03-then-comment", "\u{feff}/*c*/{\"a\":1}"),
        // --- CRLF and lone CR (F3.2: a `//` comment ends at LF *or* CR) ---
        ("cr01-trailing-crlf", "{\"a\":1}\r\n"),
        ("cr02-inside-doc", "{\r\n\"a\":1\r\n}"),
        ("cr03-lone-cr-ends-line-comment", "{\"a\":1 //c\r}"),
        ("cr04-leading-line-comment", "//c\r{\"a\":1}"),
        ("cr05-cr-in-block", "{\"a\": /*x\ry*/ 1}"),
        // --- U+2028 / U+2029, which deliberately do NOT end a comment ---
        ("ls01-in-line-comment", "{\"a\":1} //c\u{2028}"),
        ("ls02-raw-in-string", "{\"a\": \"\u{2028}\"}"),
        ("ls03-in-block-comment", "{\"a\": 1 /*\u{2028}*/}"),
        ("ls04-paragraph-separator", "{\"a\":1} //c\u{2029}"),
        // --- degenerate punctuation the trailing-comma pass has to survive ---
        ("tc01-empty-object-comma", "{,}"),
        ("tc02-empty-array-comma", "[,]"),
        ("tc03-double-comma", "{\"a\":1,,}"),
        ("tc04-comma-alone", ","),
        ("tc05-comma-then-eof", "{\"a\":1,"),
        // --- lone markers and the empty document ---
        ("lm01-empty", ""),
        ("lm02-slash", "/"),
        ("lm03-closer", "*/"),
        ("lm04-slash-then-eof", "{\"a\":1}/"),
    ];

    /// Deep nesting is generated rather than written out, so it lives beside
    /// the table instead of in it.
    fn deeply_nested() -> Vec<(String, String)> {
        vec![
            (
                "dn01-arrays".to_string(),
                "[".repeat(200) + &"]".repeat(200),
            ),
            (
                "dn02-objects".to_string(),
                "{\"a\":".repeat(200) + "1" + &"}".repeat(200),
            ),
            ("dn03-unterminated-arrays".to_string(), "[".repeat(200)),
            (
                "dn04-nested-in-comment".to_string(),
                format!("/*{}*/{{\"a\":1}}", "[".repeat(200)),
            ),
        ]
    }

    /// The property the security review established, now enforced.
    ///
    /// `catch_unwind` rather than a bare call so a regression names the input
    /// that caused it — a panic escaping the loop would otherwise report only
    /// the slice index, which is the least useful half of the story.
    #[test]
    fn adversarial_inputs_never_panic() {
        let mut cases: Vec<(String, String)> = ADVERSARIAL
            .iter()
            .map(|(id, src)| (id.to_string(), src.to_string()))
            .collect();
        cases.extend(deeply_nested());

        let mut accepted = 0;
        for (id, src) in &cases {
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(src, None).is_ok()));
            match outcome {
                Ok(true) => accepted += 1,
                Ok(false) => {}
                Err(_) => panic!("{id} panicked on {src:?}"),
            }
        }

        // A corpus where everything errors would pass the panic check while
        // exercising almost nothing — the interesting inputs are the ones that
        // survive stripping and reach serde_json. Guarding the mix keeps a
        // future "reject earlier" change from quietly hollowing this out.
        assert!(
            accepted >= 15 && accepted < cases.len(),
            "{accepted} of {} accepted — the corpus has stopped exercising both paths",
            cases.len()
        );
    }

    /// A hostile input may be accepted or refused, but the refusals that carry
    /// a diagnosis must keep carrying it — an unterminated construct reported
    /// as a generic JSON syntax error sends the reader to the wrong line.
    #[test]
    fn unterminated_constructs_are_named() {
        for src in ["/*", "{\"a\":1} /*", "/*/", "/*\u{3042}", "{\"a\":1} /*x*"] {
            let err = strip_comments(src).expect_err(src);
            assert!(
                err.message.contains("unterminated block comment"),
                "{src:?}: {}",
                err.message
            );
        }
        for src in [
            "\"",
            "{\"a",
            "{\"a\": \"b",
            "{\"a\": \"b\\",
            "{\"a\": \"\u{1f600}",
        ] {
            let err = strip_comments(src).expect_err(src);
            assert!(
                err.message.contains("unterminated string literal"),
                "{src:?}: {}",
                err.message
            );
        }
    }

    /// U+2028 and U+2029 are line breaks to JavaScript but not to the JSONC
    /// dialect this tracks (spec F3.2). If one ended a `//` comment, the
    /// closing braces after it would become trailing content and the document
    /// would be refused — so accepting it is the assertion.
    #[test]
    fn a_line_separator_does_not_end_a_line_comment() {
        for sep in ['\u{2028}', '\u{2029}'] {
            let src = format!("{{\"a\":1 //c{sep}}}}}}}");
            let stripped = strip_comments(&src).expect(&src);
            assert!(
                !stripped.contains('}'),
                "{sep:?} ended the comment: {stripped:?}"
            );
        }
    }

    /// The sequence of line terminators, which is what "same line structure"
    /// means: a `\r\n` collapsed to `\n`, or a lone `\r` dropped, both show up
    /// here as a difference.
    fn terminators(s: &str) -> String {
        s.chars().filter(|c| *c == '\r' || *c == '\n').collect()
    }

    /// Both strip passes must preserve line structure exactly. Only the block
    /// comment branch was ever wrong, but pinning the whole pipeline is what
    /// makes that a property of the module rather than of one branch.
    #[test]
    fn stripping_preserves_line_structure() {
        for src in [
            // block comments, each line ending
            "{\n  /* a\n b */\n  \"k\": 1\n}",
            "{\r\n  /* a\r\n b */\r\n  \"k\": 1\r\n}",
            "{\r  /* a\r b */\r  \"k\": 1\r}",
            // a lone CR inside a comment on one line
            "{\"k\": /*a\rb*/ 1}",
            // mixed, and a CR immediately before a CRLF
            "{\"k\": /*a\r\r\nb*/ 1}",
            // line comments, whose terminator must survive as itself
            "{\n  // a\n  \"k\": 1\n}",
            "{\r\n  // a\r\n  \"k\": 1\r\n}",
            "{\r  // a\r  \"k\": 1\r}",
            // trailing commas removed next to line breaks
            "{\r\n  \"k\": 1,\r\n}",
            "[\r  1,\r]",
            // comment markers inside strings are data, not comments
            "{\"k\": \"a/*b*/c\",\r\n \"j\": 2}",
        ] {
            let stripped = strip_trailing_commas(&strip_comments(src).expect(src));
            assert_eq!(
                terminators(&stripped),
                terminators(src),
                "line structure changed for {src:?} -> {stripped:?}"
            );
        }
    }

    /// The token-separation invariant, at the same level as the one above.
    #[test]
    fn a_comment_never_joins_its_neighbours() {
        for src in ["1/*x*/2", "1/*\r*/2", "1/*\r\n*/2", "tr/*x*/ue"] {
            let stripped = strip_comments(src).expect(src);
            assert!(
                !stripped.contains("12") && !stripped.contains("true"),
                "{src:?} spliced into {stripped:?}"
            );
        }
    }
}
