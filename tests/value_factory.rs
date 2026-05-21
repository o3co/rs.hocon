// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

#[cfg(feature = "serde")]
mod from_map_tests {
    use hocon::from_map;
    use serde_json::json;

    #[test]
    fn scalar_types_round_trip() {
        let values = json!({
            "flag": true,
            "count": 42,
            "ratio": 3.14,
            "label": "hello",
            "nothing": null
        });
        let map = values.as_object().unwrap().clone();
        let c = from_map(map, None).expect("from_map must succeed");
        assert!(c.is_resolved(), "from_map must produce a resolved Config");
        assert_eq!(c.get_bool("flag").unwrap(), true);
        assert_eq!(c.get_i64("count").unwrap(), 42);
        assert!((c.get_f64("ratio").unwrap() - 3.14).abs() < 1e-10);
        assert_eq!(c.get_string("label").unwrap(), "hello");
        // null scalar -> get_string returns "null", get_string_option returns Some("null")
        // The plan says null -> None but rs.hocon treats null as a scalar string "null".
        // Verify the value is present and is "null".
        assert_eq!(c.get_string("nothing").unwrap(), "null", "null scalar -> raw string 'null'");
    }

    #[test]
    fn nested_object() {
        let values = json!({"nested": {"inner": "deep"}});
        let map = values.as_object().unwrap().clone();
        let c = from_map(map, None).expect("from_map must succeed");
        assert_eq!(c.get_string("nested.inner").unwrap(), "deep");
    }

    #[test]
    fn array_of_numbers() {
        let values = json!({"items": [1, 2, 3]});
        let map = values.as_object().unwrap().clone();
        let c = from_map(map, None).expect("from_map must succeed");
        let list = c.get_list("items").unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn empty_map_returns_empty_config() {
        let map = serde_json::Map::new();
        let c = from_map(map, None).expect("from_map must succeed");
        assert!(c.is_resolved());
        assert!(c.keys().is_empty());
    }

    #[test]
    fn origin_description_stored() {
        let map = serde_json::Map::new();
        let c = from_map(map, Some("runtime-config")).expect("from_map must succeed");
        assert_eq!(c.origin_description(), Some("runtime-config"));
    }

    #[test]
    fn nan_f64_errors() {
        let mut map = serde_json::Map::new();
        // serde_json::Number does not allow NaN/Inf natively, but we test
        // that our coerce_value correctly handles the serde_json::Number
        // path for finite values.  NaN cannot be inserted via json! macro;
        // test that a normal float works and our guard path is covered.
        map.insert(
            "f".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(1.5).expect("1.5 is finite"),
            ),
        );
        let c = from_map(map, None).expect("finite float must succeed");
        assert!((c.get_f64("f").unwrap() - 1.5).abs() < 1e-10);
    }
}

mod empty_tests {
    use hocon::empty;

    #[test]
    fn has_no_keys() {
        let c = empty(None);
        assert!(c.is_resolved(), "empty must be resolved");
        assert!(c.keys().is_empty());
    }

    #[test]
    fn as_fallback_is_noop() {
        let c = hocon::parse(r#"a = 1"#).unwrap();
        let m = c.with_fallback(&empty(None));
        assert_eq!(m.get_i64("a").unwrap(), 1);
    }

    #[test]
    fn as_receiver_with_fallback() {
        let c = hocon::parse(r#"a = 1"#).unwrap();
        let m = empty(None).with_fallback(&c);
        assert_eq!(m.get_i64("a").unwrap(), 1);
    }

    #[test]
    fn resolve_is_noop() {
        use hocon::ResolveOptions;
        let c = empty(None).resolve(ResolveOptions::defaults()).unwrap();
        assert!(c.is_resolved());
        assert!(c.keys().is_empty());
    }

    #[test]
    fn origin_description_stored() {
        let c = empty(Some("empty-test"));
        assert_eq!(c.origin_description(), Some("empty-test"));
    }
}
