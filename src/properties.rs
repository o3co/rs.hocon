use indexmap::IndexMap;
use crate::value::{HoconValue, ScalarValue};

/// Parse a .properties file into a flat key-value map.
/// All values are strings per the .properties/.hocon spec.
pub fn parse_properties(input: &str) -> IndexMap<String, String> {
    let mut result = IndexMap::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let sep_pos = trimmed.find(|c: char| c == '=' || c == ':');
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
pub fn properties_to_hocon(input: &str) -> HoconValue {
    let props = parse_properties(input);
    let mut root = IndexMap::new();

    for (key, value) in props {
        let segments: Vec<&str> = key.split('.').collect();
        set_nested(&mut root, &segments, HoconValue::Scalar(ScalarValue::String(value)));
    }

    HoconValue::Object(root)
}

fn set_nested(map: &mut IndexMap<String, HoconValue>, segments: &[&str], value: HoconValue) {
    if segments.is_empty() {
        return;
    }
    if segments.len() == 1 {
        map.insert(segments[0].to_string(), value);
        return;
    }
    let head = segments[0].to_string();
    let tail = &segments[1..];
    let entry = map.entry(head).or_insert_with(|| HoconValue::Object(IndexMap::new()));
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
                assert_eq!(a.get("b"), Some(&HoconValue::Scalar(ScalarValue::String("1".into()))));
            } else {
                panic!("expected nested object for 'a'");
            }
            assert_eq!(map.get("c"), Some(&HoconValue::Scalar(ScalarValue::String("hello".into()))));
        } else {
            panic!("expected object");
        }
    }
}
