use crate::value::{HoconValue, ScalarValue};
use indexmap::IndexMap;

/// Parse a .properties file into a flat key-value map, the way
/// `java.util.Properties` does — which is what Lightbend uses for
/// `include "x.properties"`.
///
/// Covers S23.5 (backslash continuations) and S23.6 (unicode escapes), both in
/// scope since 2026-07-24, along with the rules that come with them: `=`, `:`
/// or whitespace as the separator, an escaped separator belonging to the key,
/// and a value keeping its trailing whitespace (Java skips whitespace before a
/// value, never after it).
///
/// All values are strings per the .properties/.hocon spec. A repeated key keeps
/// the last value.
///
/// Errors when a `\uXXXX` escape is malformed or names an unpaired surrogate: a
/// Rust `String` is UTF-8 and cannot hold one, where Java's UTF-16 `String` can
/// (see S1.2.6).
pub fn parse_properties(input: &str) -> Result<IndexMap<String, String>, String> {
    let mut result = IndexMap::new();
    for line in logical_lines(input) {
        let (raw_key, raw_value) = split_key_value(&line);
        let key = unescape(&raw_key)?;
        if key.is_empty() {
            continue;
        }
        result.insert(key, unescape(&raw_value)?);
    }
    Ok(result)
}

/// Drop blank and comment lines and join backslash continuations.
///
/// Comment status is decided per natural line before joining, so a continuation
/// line that happens to start with `#` is value text rather than a comment.
fn logical_lines(input: &str) -> Vec<String> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let natural: Vec<&str> = normalized.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < natural.len() {
        let mut text = trim_leading_space(natural[i]).to_string();
        if text.is_empty() || text.starts_with('#') || text.starts_with('!') {
            i += 1;
            continue;
        }
        while ends_with_continuation(&text) {
            text.pop();
            if i + 1 >= natural.len() {
                break;
            }
            i += 1;
            text.push_str(trim_leading_space(natural[i]));
        }
        out.push(text);
        i += 1;
    }
    out
}

fn trim_leading_space(s: &str) -> &str {
    s.trim_start_matches([' ', '\t', '\u{0C}'])
}

/// An odd number of trailing backslashes means the last one is an escape, so
/// the line continues; an even number means they escape each other.
fn ends_with_continuation(line: &str) -> bool {
    line.chars().rev().take_while(|&c| c == '\\').count() % 2 == 1
}

/// Split at the first unescaped `=`, `:` or whitespace run, then skip
/// whitespace around that separator. Whatever remains is the value, trailing
/// whitespace included.
fn split_key_value(line: &str) -> (String, String) {
    let chars: Vec<char> = line.chars().collect();
    let mut key = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            key.push(c);
            key.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c == '=' || c == ':' || is_props_space(c) {
            break;
        }
        key.push(c);
        i += 1;
    }
    while i < chars.len() && is_props_space(chars[i]) {
        i += 1;
    }
    if i < chars.len() && (chars[i] == '=' || chars[i] == ':') {
        i += 1;
        while i < chars.len() && is_props_space(chars[i]) {
            i += 1;
        }
    }
    (key, chars[i..].iter().collect())
}

fn is_props_space(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\u{0C}'
}

/// Apply the `java.util.Properties` escape rules. An unknown escape drops the
/// backslash and a trailing lone backslash is dropped, both as Java does.
fn unescape(s: &str) -> Result<String, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= chars.len() {
            break;
        }
        match chars[i] {
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'f' => out.push('\u{0C}'),
            'u' => {
                let (cp, consumed) = unicode_escape(&chars, i)?;
                out.push(cp);
                i += consumed;
            }
            other => out.push(other),
        }
        i += 1;
    }
    Ok(out)
}

/// Decode the `\uXXXX` at `chars[i] == 'u'`, combining a surrogate pair when one
/// follows. Returns the codepoint and how many chars past `'u'` were consumed.
///
/// Java strings are UTF-16 and can hold an unpaired surrogate; Rust strings
/// cannot, so one is an error rather than a silent replacement character.
fn unicode_escape(chars: &[char], i: usize) -> Result<(char, usize), String> {
    let hi = hex4(chars, i + 1)?;
    if !(0xD800..=0xDFFF).contains(&hi) {
        let c =
            char::from_u32(hi).ok_or_else(|| format!("\\u{hi:04X} is not a valid codepoint"))?;
        return Ok((c, 4));
    }
    if hi > 0xDBFF {
        return Err(format!("\\u{hi:04X} is an unpaired low surrogate"));
    }
    if i + 6 < chars.len() && chars[i + 5] == '\\' && chars[i + 6] == 'u' {
        if let Ok(lo) = hex4(chars, i + 7) {
            if (0xDC00..=0xDFFF).contains(&lo) {
                let cp = 0x1_0000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                let c = char::from_u32(cp)
                    .ok_or_else(|| format!("surrogate pair \\u{hi:04X}\\u{lo:04X} is invalid"))?;
                return Ok((c, 10));
            }
        }
    }
    Err(format!("\\u{hi:04X} is an unpaired high surrogate"))
}

fn hex4(chars: &[char], start: usize) -> Result<u32, String> {
    if start + 4 > chars.len() {
        return Err("truncated \\u escape".to_string());
    }
    let digits: String = chars[start..start + 4].iter().collect();
    u32::from_str_radix(&digits, 16).map_err(|_| format!("invalid \\u escape \"{digits}\""))
}

/// Convert parsed .properties into a HoconValue::Object, expanding dotted keys
/// into nested objects. All values remain strings (per HOCON spec for .properties).
///
/// Keys are processed in **sorted order** so that conflict resolution is
/// deterministic regardless of the input file line order. This mirrors the
/// sort discipline required by HOCON.md L1476-1479, which notes that Java
/// properties files do not preserve order.
///
/// Conflict rule (HOCON.md L1485): object wins over scalar. When a dotted key
/// expands to an object subtree and a plain key also exists at the same path,
/// the object is kept and the scalar is discarded.
pub fn properties_to_hocon(input: &str) -> Result<HoconValue, String> {
    let props = parse_properties(input)?;
    let mut root = IndexMap::new();

    // Collect and sort keys for deterministic conflict resolution (HOCON.md L1476-1479).
    let mut keys: Vec<&String> = props.keys().collect();
    keys.sort();

    for key in keys {
        let value = &props[key];
        let segments: Vec<&str> = key.split('.').collect();
        set_nested(
            &mut root,
            &segments,
            HoconValue::Scalar(ScalarValue::string(value.clone())),
        );
    }

    Ok(HoconValue::Object(root))
}

/// Recursively set a value at a dotted-key path, applying the object-wins rule
/// (HOCON.md L1485): "the object must always win."
///
/// - **Last segment + existing object**: SKIP — the scalar is discarded (object wins).
/// - **Last segment + no existing / existing scalar**: write the scalar.
/// - **Non-last segment + existing scalar**: REPLACE the scalar with a new object,
///   then descend. The scalar is discarded per L1487 ("the 'object wins' rule
///   throws out at most one value, the string").
/// - **Non-last segment + existing object**: descend (already correct).
fn set_nested(map: &mut IndexMap<String, HoconValue>, segments: &[&str], value: HoconValue) {
    if segments.is_empty() {
        return;
    }
    if segments.len() == 1 {
        // Last segment: object wins — only write the scalar if no existing object.
        match map.get(segments[0]) {
            Some(HoconValue::Object(_)) => {} // object wins — discard incoming scalar
            _ => {
                map.insert(segments[0].to_string(), value);
            }
        }
        return;
    }
    // Non-last segment: ensure an object node exists at `head`.
    let head = segments[0].to_string();
    let tail = &segments[1..];
    let entry = map
        .entry(head)
        .or_insert_with(|| HoconValue::Object(IndexMap::new()));
    // If a scalar was sitting here (e.g. `a=hello` before `a.b=world`), replace it
    // with an empty object so the dotted subtree can be built (object wins, L1487).
    if !matches!(entry, HoconValue::Object(_)) {
        *entry = HoconValue::Object(IndexMap::new());
    }
    if let HoconValue::Object(inner) = entry {
        set_nested(inner, tail, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> IndexMap<String, String> {
        parse_properties(input).expect("parse_properties")
    }

    fn get(input: &str, key: &str) -> String {
        parse(input)
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("key {key:?} missing"))
    }

    #[test]
    fn parses_simple_key_value() {
        assert_eq!(get("key=value", "key"), "value");
    }

    #[test]
    fn parses_multiple_lines() {
        let result = parse("a=1\nb=2\nc=3");
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("a"), Some(&"1".to_string()));
    }

    #[test]
    fn skips_comments() {
        let result = parse("# comment\nkey=value\n! another comment");
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn skips_empty_lines() {
        assert_eq!(parse("\n\nkey=value\n\n").len(), 1);
    }

    #[test]
    fn handles_dotted_keys() {
        assert_eq!(get("a.b.c=hello", "a.b.c"), "hello");
    }

    #[test]
    fn handles_colon_separator() {
        assert_eq!(get("key:value", "key"), "value");
    }

    #[test]
    fn handles_whitespace_around_separator() {
        assert_eq!(get("key = value", "key"), "value");
    }

    /// Java skips whitespace before a value but never after it, so the trailing
    /// run survives. Pinned by the ps04 fixture; this asserted the opposite
    /// until S23.5/S23.6 came in scope on 2026-07-24.
    #[test]
    fn value_keeps_trailing_whitespace() {
        assert_eq!(get("  key  =  value  ", "key"), "value  ");
    }

    /// S23.5 — whitespace alone separates a key from its value.
    #[test]
    fn whitespace_is_a_separator() {
        assert_eq!(get("host localhost", "host"), "localhost");
        assert_eq!(get("f value = 3", "f"), "value = 3");
    }

    /// S23.5 — a trailing backslash continues the line.
    #[test]
    fn joins_continuations() {
        assert_eq!(get("a = one\\\ntwo", "a"), "onetwo");
        assert_eq!(get("a = one\\\n      two", "a"), "onetwo");
    }

    /// An even run of backslashes escapes itself and does not continue.
    #[test]
    fn even_backslash_run_is_not_a_continuation() {
        let result = parse("a = end\\\\\nb = 2");
        assert_eq!(result.get("a"), Some(&"end\\".to_string()));
        assert_eq!(result.get("b"), Some(&"2".to_string()));
    }

    /// Comment status is decided before continuations are joined.
    #[test]
    fn continuation_into_hash_is_value_text() {
        assert_eq!(get("a = one\\\n#two", "a"), "one#two");
    }

    /// S23.6 — the escape set, with an unknown escape dropping its backslash.
    #[test]
    fn applies_escape_set() {
        assert_eq!(get("a = x\\ty", "a"), "x\ty");
        assert_eq!(get("a = \\u00e9", "a"), "é");
        assert_eq!(get("a = q\\zr", "a"), "qzr");
    }

    /// S23.6 — an escaped separator belongs to the key.
    #[test]
    fn escaped_separator_belongs_to_key() {
        assert_eq!(get("b\\:c = 2", "b:c"), "2");
        assert_eq!(get("a\\ b = 1", "a b"), "1");
    }

    /// S23.6 — a surrogate pair becomes its astral character.
    #[test]
    fn combines_surrogate_pair() {
        assert_eq!(get("a = \\ud83d\\ude00", "a"), "\u{1F600}");
    }

    /// A Rust String is UTF-8 and cannot hold a lone surrogate, so it is an
    /// error rather than a silent replacement character (S1.2.6). ts.hocon
    /// accepts one, its strings being UTF-16 like Java's.
    #[test]
    fn rejects_unpaired_surrogate_and_malformed_escapes() {
        for src in ["a = \\ud83d", "a = \\ude00", "a = \\u12", "a = \\uZZZZ"] {
            assert!(parse_properties(src).is_err(), "expected error for {src:?}");
        }
    }

    #[test]
    fn values_are_always_strings() {
        assert_eq!(get("num=42\nbool=true", "num"), "42");
        assert_eq!(get("num=42\nbool=true", "bool"), "true");
    }

    #[test]
    fn converts_to_hocon_value() {
        let hv = properties_to_hocon("a.b=1\nc=hello").expect("properties_to_hocon");
        if let HoconValue::Object(map) = &hv {
            if let Some(HoconValue::Object(a)) = map.get("a") {
                assert_eq!(
                    a.get("b"),
                    Some(&HoconValue::Scalar(ScalarValue::string("1".into())))
                );
            } else {
                panic!("expected nested object for 'a'");
            }
            assert_eq!(
                map.get("c"),
                Some(&HoconValue::Scalar(ScalarValue::string("hello".into())))
            );
        } else {
            panic!("expected object");
        }
    }
}
