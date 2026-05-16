/// Numerically-indexed object → array conversion (S15).
///
/// Implements the `numericObjectToArray` helper per the design spec:
/// `docs/superpowers/specs/2026-05-16-s15-numeric-obj-array-design.md`
///
/// # Contract
///
/// ```text
/// numeric_object_to_array(value) →
///   if value is not an Object       → None (caller handles type mismatch)
///   if value is an empty Object     → None (S15.4: empty NOT converted)
///   if no key parses as non-neg int → None (caller handles type mismatch)
///   otherwise                       → Array of values, sorted by parsed key
/// ```
///
/// # Integer key parse rule (cross-impl convergence point)
///
/// A key `s` is eligible iff it matches the regex `^(0|[1-9][0-9]*)$` AND
/// the resulting integer fits in i32 (≤ 2_147_483_647).
///
/// Rejected forms (even though Rust's `str::parse::<i32>()` would accept them):
/// - `"+1"` — leading `+` sign
/// - `"-0"` — leading `-` sign
/// - `"00"`, `"007"` — leading zeros
/// - `" 1"`, `"1 "` — whitespace
/// - `""` — empty
use crate::value::HoconValue;

/// Attempt to convert a numerically-indexed HOCON object to an array.
///
/// Returns `Some(Vec<HoconValue>)` if the object has at least one eligible
/// integer key. Returns `None` if the value is not an object, is empty, or
/// has no eligible integer keys.
///
/// This function is invoked from two call-sites:
/// 1. `Config::get_list` — accessor-time conversion (S15.1, S15.5, S15.6, S15.7)
/// 2. Resolver pairwise-join concat — concat-time conversion (S15.3)
///
/// It is deliberately NOT called from `Config::get`, `Config::get_config`, or
/// any untyped accessor — laziness is preserved per S15.2.
pub(crate) fn numeric_object_to_array(value: &HoconValue) -> Option<Vec<HoconValue>> {
    let map = match value {
        HoconValue::Object(m) => m,
        _ => return None,
    };

    // S15.4: empty object → None
    if map.is_empty() {
        return None;
    }

    // Collect eligible (parsed_key, value) pairs
    let mut eligible: Vec<(i32, HoconValue)> = map
        .iter()
        .filter_map(|(k, v)| parse_eligible_key(k).map(|n| (n, v.clone())))
        .collect();

    // No eligible integer keys → None
    if eligible.is_empty() {
        return None;
    }

    // Sort ascending by integer key; compaction is implicit (gaps are discarded)
    eligible.sort_by_key(|(n, _)| *n);

    Some(eligible.into_iter().map(|(_, v)| v).collect())
}

/// Parse a key string as an eligible non-negative integer.
///
/// Returns `Some(n)` iff the key matches `^(0|[1-9][0-9]*)$` and n ≤ i32::MAX.
/// Returns `None` otherwise.
///
/// The pre-filter (character-by-character check equivalent to the regex) MUST
/// precede any native `parse::<i32>()` call — Rust accepts `"+1"`, `"-0"`,
/// and `"00"` which are all rejected by this spec.
fn parse_eligible_key(s: &str) -> Option<i32> {
    if s.is_empty() {
        return None;
    }

    // Pre-filter: must match ^(0|[1-9][0-9]*)$
    // - First char: '0' or '1'–'9'
    // - Remaining chars (if any): all '0'–'9'
    // - If first char is '0', there must be no remaining chars
    let mut chars = s.chars();
    let first = chars.next()?;
    match first {
        '0' => {
            // Valid only if "0" exactly (no trailing digits)
            if chars.next().is_some() {
                return None; // "00", "007", etc.
            }
        }
        '1'..='9' => {
            // Remaining chars must all be digits
            for c in chars {
                if !c.is_ascii_digit() {
                    return None;
                }
            }
        }
        _ => return None, // '-', '+', ' ', letters, etc.
    }

    // Pre-filter passed; now parse with range check (i32::MAX = 2_147_483_647)
    s.parse::<i32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{HoconValue, ScalarValue};
    use indexmap::IndexMap;

    fn sv(s: &str) -> HoconValue {
        HoconValue::Scalar(ScalarValue::string(s.to_string()))
    }

    fn make_obj(pairs: &[(&str, &str)]) -> HoconValue {
        let mut map = IndexMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), sv(v));
        }
        HoconValue::Object(map)
    }

    // ── parse_eligible_key unit tests ──────────────────────────────────────────

    #[test]
    fn eligible_zero() {
        assert_eq!(parse_eligible_key("0"), Some(0));
    }

    #[test]
    fn eligible_positive() {
        assert_eq!(parse_eligible_key("1"), Some(1));
        assert_eq!(parse_eligible_key("42"), Some(42));
        assert_eq!(parse_eligible_key("100"), Some(100));
    }

    #[test]
    fn eligible_i32_max() {
        assert_eq!(parse_eligible_key("2147483647"), Some(2_147_483_647_i32));
    }

    #[test]
    fn rejected_leading_zero() {
        assert_eq!(parse_eligible_key("00"), None);
        assert_eq!(parse_eligible_key("01"), None);
        assert_eq!(parse_eligible_key("007"), None);
    }

    #[test]
    fn rejected_plus_sign() {
        assert_eq!(parse_eligible_key("+1"), None);
        assert_eq!(parse_eligible_key("+0"), None);
    }

    #[test]
    fn rejected_minus_sign() {
        assert_eq!(parse_eligible_key("-1"), None);
        assert_eq!(parse_eligible_key("-0"), None);
    }

    #[test]
    fn rejected_whitespace() {
        assert_eq!(parse_eligible_key(" 1"), None);
        assert_eq!(parse_eligible_key("1 "), None);
    }

    #[test]
    fn rejected_empty() {
        assert_eq!(parse_eligible_key(""), None);
    }

    #[test]
    fn rejected_overflow() {
        // 2^31 = 2_147_483_648 > i32::MAX
        assert_eq!(parse_eligible_key("2147483648"), None);
        assert_eq!(parse_eligible_key("99999999999"), None);
    }

    #[test]
    fn rejected_decimal() {
        assert_eq!(parse_eligible_key("1.0"), None);
        assert_eq!(parse_eligible_key("1e2"), None);
    }

    #[test]
    fn rejected_hex() {
        assert_eq!(parse_eligible_key("0x1"), None);
        assert_eq!(parse_eligible_key("0b10"), None);
    }

    #[test]
    fn rejected_alpha() {
        assert_eq!(parse_eligible_key("foo"), None);
        assert_eq!(parse_eligible_key("bar"), None);
    }

    // ── numeric_object_to_array unit tests ────────────────────────────────────

    #[test]
    fn not_an_object_returns_none() {
        let v = sv("hello");
        assert!(numeric_object_to_array(&v).is_none());
    }

    #[test]
    fn empty_object_returns_none() {
        let v = HoconValue::Object(IndexMap::new());
        assert!(numeric_object_to_array(&v).is_none());
    }

    #[test]
    fn no_eligible_keys_returns_none() {
        // na12: all non-int keys
        let v = make_obj(&[("foo", "a"), ("bar", "b")]);
        assert!(numeric_object_to_array(&v).is_none());
    }

    #[test]
    fn basic_conversion() {
        // na01: {"0":"a","1":"b"} → ["a","b"]
        let v = make_obj(&[("0", "a"), ("1", "b")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 2);
        assert_eq!(extract_raw(&arr[0]), "a");
        assert_eq!(extract_raw(&arr[1]), "b");
    }

    #[test]
    fn non_int_keys_ignored() {
        // na05: {"0":"a","foo":"b","1":"c"} → ["a","c"]
        let v = make_obj(&[("0", "a"), ("foo", "b"), ("1", "c")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 2);
        assert_eq!(extract_raw(&arr[0]), "a");
        assert_eq!(extract_raw(&arr[1]), "c");
    }

    #[test]
    fn gaps_compacted() {
        // na06: {"0":"a","2":"c"} → ["a","c"] (no slot for index 1)
        let v = make_obj(&[("0", "a"), ("2", "c")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 2);
        assert_eq!(extract_raw(&arr[0]), "a");
        assert_eq!(extract_raw(&arr[1]), "c");
    }

    #[test]
    fn sorted_by_key() {
        // na07: {"1":"b","0":"a"} → ["a","b"]
        let v = make_obj(&[("1", "b"), ("0", "a")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 2);
        assert_eq!(extract_raw(&arr[0]), "a");
        assert_eq!(extract_raw(&arr[1]), "b");
    }

    #[test]
    fn leading_zero_rejected_only_canonical_zero_eligible() {
        // na08: {"00":"a","0":"b"} → ["b"] (only "0" eligible)
        let v = make_obj(&[("00", "a"), ("0", "b")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 1);
        assert_eq!(extract_raw(&arr[0]), "b");
    }

    #[test]
    fn negative_key_rejected() {
        // na09: {"-1":"a","0":"b"} → ["b"]
        let v = make_obj(&[("-1", "a"), ("0", "b")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 1);
        assert_eq!(extract_raw(&arr[0]), "b");
    }

    #[test]
    fn plus_sign_rejected() {
        // na10a: {"+1":"a","0":"b"} → ["b"]
        let v = make_obj(&[("+1", "a"), ("0", "b")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 1);
        assert_eq!(extract_raw(&arr[0]), "b");
    }

    #[test]
    fn minus_zero_rejected() {
        // na10b: {"-0":"a","0":"b"} → ["b"]
        let v = make_obj(&[("-0", "a"), ("0", "b")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 1);
        assert_eq!(extract_raw(&arr[0]), "b");
    }

    #[test]
    fn overflow_rejected() {
        // na11: {"99999999999":"a","0":"b"} → ["b"]
        let v = make_obj(&[("99999999999", "a"), ("0", "b")]);
        let arr = numeric_object_to_array(&v).expect("should convert");
        assert_eq!(arr.len(), 1);
        assert_eq!(extract_raw(&arr[0]), "b");
    }

    fn extract_raw(v: &HoconValue) -> &str {
        match v {
            HoconValue::Scalar(sv) => &sv.raw,
            _ => panic!("expected scalar, got {:?}", v),
        }
    }
}
