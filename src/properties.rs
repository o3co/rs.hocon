use crate::value::{HoconValue, ScalarValue};
use indexmap::IndexMap;

/// Parse a .properties file into a flat key-value map.
/// All values are strings per the .properties/.hocon spec.
pub fn parse_properties(input: &str) -> IndexMap<String, String> {
    let mut result = IndexMap::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let sep_pos = trimmed.find(['=', ':']);
        if let Some(pos) = sep_pos {
            let key = trimmed[..pos].trim().to_string();
            let value = trimmed[pos + 1..].trim().to_string();
            if !key.is_empty() {
                result.insert(key, value);
            }
        }
    }
    result
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
pub fn properties_to_hocon(input: &str) -> HoconValue {
    let props = parse_properties(input);
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

    HoconValue::Object(root)
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

    #[test]
    fn parses_simple_key_value() {
        let result = parse_properties("key=value");
        assert_eq!(result.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn parses_multiple_lines() {
        let result = parse_properties("a=1\nb=2\nc=3");
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("a"), Some(&"1".to_string()));
    }

    #[test]
    fn skips_comments() {
        let result = parse_properties("# comment\nkey=value\n! another comment");
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn skips_empty_lines() {
        let result = parse_properties("\n\nkey=value\n\n");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn handles_dotted_keys() {
        let result = parse_properties("a.b.c=hello");
        assert_eq!(result.get("a.b.c"), Some(&"hello".to_string()));
    }

    #[test]
    fn handles_colon_separator() {
        let result = parse_properties("key:value");
        assert_eq!(result.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn handles_whitespace_around_separator() {
        let result = parse_properties("key = value");
        assert_eq!(result.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn values_are_always_strings() {
        let result = parse_properties("num=42\nbool=true");
        assert_eq!(result.get("num"), Some(&"42".to_string()));
        assert_eq!(result.get("bool"), Some(&"true".to_string()));
    }

    #[test]
    fn converts_to_hocon_value() {
        let hv = properties_to_hocon("a.b=1\nc=hello");
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

    // ─── S23.4 object-wins tests (mis-classification fix) ─────────────────────

    fn obj_wins_check(hv: &HoconValue) {
        // Result must be `{a: {b: "world"}}` — object wins over scalar at `a`.
        if let HoconValue::Object(map) = hv {
            if let Some(HoconValue::Object(a_obj)) = map.get("a") {
                assert_eq!(
                    a_obj.get("b"),
                    Some(&HoconValue::Scalar(ScalarValue::string("world".into()))),
                    "S23.4: a.b must be 'world'"
                );
                assert_eq!(a_obj.len(), 1, "S23.4: no extra keys under a");
            } else {
                panic!("S23.4: a must be an Object, got: {:?}", map.get("a"));
            }
            assert_eq!(map.len(), 1, "S23.4: root must have exactly 1 key");
        } else {
            panic!("S23.4: root must be Object");
        }
    }

    /// S23.4 forward order: `a=hello\na.b=world` → `{a: {b: "world"}}`.
    /// The scalar `a=hello` is discarded (object wins per HOCON.md L1485).
    #[test]
    fn s23_4_forward_object_wins() {
        let hv = properties_to_hocon("a=hello\na.b=world");
        obj_wins_check(&hv);
    }

    /// S23.4 reverse order: `a.b=world\na=hello` → same `{a: {b: "world"}}`.
    /// Sort discipline ensures identical result regardless of input line order.
    #[test]
    fn s23_4_reverse_object_wins() {
        let hv = properties_to_hocon("a.b=world\na=hello");
        obj_wins_check(&hv);
    }

    /// S23.4 deep forward (pc03 shape): `a.b.c=v1\na.b=v2` → `{a: {b: {c: "v1"}}}`.
    /// The scalar `a.b=v2` is discarded (object at a.b wins).
    #[test]
    fn s23_4_deep_forward_object_wins() {
        let hv = properties_to_hocon("a.b.c=v1\na.b=v2");
        if let HoconValue::Object(map) = &hv {
            if let Some(HoconValue::Object(a_obj)) = map.get("a") {
                if let Some(HoconValue::Object(b_obj)) = a_obj.get("b") {
                    assert_eq!(
                        b_obj.get("c"),
                        Some(&HoconValue::Scalar(ScalarValue::string("v1".into()))),
                        "S23.4 deep: a.b.c must be 'v1'"
                    );
                } else {
                    panic!("S23.4 deep: a.b must be Object, got: {:?}", a_obj.get("b"));
                }
            } else {
                panic!("S23.4 deep: a must be Object");
            }
        } else {
            panic!("S23.4 deep: root must be Object");
        }
    }

    /// S23.4 deep reverse (pc04 shape): `a.b=v1\na.b.c=v2` → `{a: {b: {c: "v2"}}}`.
    /// The scalar at `a.b=v1` is replaced by an object when `a.b.c` is processed.
    #[test]
    fn s23_4_deep_reverse_object_wins() {
        let hv = properties_to_hocon("a.b=v1\na.b.c=v2");
        if let HoconValue::Object(map) = &hv {
            if let Some(HoconValue::Object(a_obj)) = map.get("a") {
                if let Some(HoconValue::Object(b_obj)) = a_obj.get("b") {
                    assert_eq!(
                        b_obj.get("c"),
                        Some(&HoconValue::Scalar(ScalarValue::string("v2".into()))),
                        "S23.4 deep rev: a.b.c must be 'v2'"
                    );
                } else {
                    panic!(
                        "S23.4 deep rev: a.b must be Object, got: {:?}",
                        a_obj.get("b")
                    );
                }
            } else {
                panic!("S23.4 deep rev: a must be Object");
            }
        } else {
            panic!("S23.4 deep rev: root must be Object");
        }
    }
}
