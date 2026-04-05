mod include_loader;
mod structure_builder;
mod substitution_resolver;
mod types;
mod utils;

use crate::error::ResolveError;
use crate::parser::AstNode;
use crate::value::HoconValue;

pub use types::ResolveOptions;

use structure_builder::StructureBuilder;
use substitution_resolver::SubstitutionResolver;

// ---- Public entry point ----

pub fn resolve(ast: AstNode, opts: &ResolveOptions) -> Result<HoconValue, ResolveError> {
    let root = StructureBuilder::new(opts).build(ast, &[])?;
    SubstitutionResolver::new(&root, &opts.env).resolve()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse_tokens;
    use crate::value::{HoconValue, ScalarValue};
    use indexmap::IndexMap;
    use std::collections::HashMap;

    fn resolve_str(input: &str) -> HoconValue {
        resolve_str_with_env(input, &HashMap::new())
    }

    fn resolve_str_with_env(input: &str, env: &HashMap<String, String>) -> HoconValue {
        let tokens = tokenize(input).unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        resolve(ast, &ResolveOptions::new(env.clone())).unwrap()
    }

    fn obj(v: &HoconValue) -> &IndexMap<String, HoconValue> {
        match v {
            HoconValue::Object(m) => m,
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn resolves_simple_string() {
        let v = resolve_str("host = \"localhost\"");
        assert_eq!(
            obj(&v).get("host"),
            Some(&HoconValue::Scalar(ScalarValue::string("localhost".into())))
        );
    }

    #[test]
    fn resolves_number() {
        let v = resolve_str("port = 8080");
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::number("8080".into())))
        );
    }

    #[test]
    fn resolves_nested_objects() {
        let v = resolve_str("server { host = \"localhost\" }");
        assert!(matches!(obj(&v).get("server"), Some(HoconValue::Object(_))));
    }

    #[test]
    fn merges_duplicate_object_keys() {
        let v = resolve_str("server { host = \"a\" }\nserver { port = 8080 }");
        if let Some(HoconValue::Object(server)) = obj(&v).get("server") {
            assert!(server.contains_key("host"));
            assert!(server.contains_key("port"));
        } else {
            panic!("expected server object");
        }
    }

    #[test]
    fn last_value_wins_for_scalars() {
        let v = resolve_str("x = 1\nx = 2");
        assert_eq!(
            obj(&v).get("x"),
            Some(&HoconValue::Scalar(ScalarValue::number("2".into())))
        );
    }

    #[test]
    fn resolves_arrays() {
        let v = resolve_str("list = [1, 2, 3]");
        if let Some(HoconValue::Array(items)) = obj(&v).get("list") {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn handles_plus_equals_on_existing_array() {
        let v = resolve_str("list = [1, 2]\nlist += 3");
        if let Some(HoconValue::Array(items)) = obj(&v).get("list") {
            assert_eq!(items.len(), 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn handles_plus_equals_on_missing_key() {
        let v = resolve_str("list += 1");
        if let Some(HoconValue::Array(items)) = obj(&v).get("list") {
            assert_eq!(items.len(), 1);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn preserves_key_order() {
        let v = resolve_str("c = 3\na = 1\nb = 2");
        let keys: Vec<&String> = obj(&v).keys().collect();
        assert_eq!(keys, vec!["c", "a", "b"]);
    }

    #[test]
    fn resolves_substitution() {
        let v = resolve_str("host = \"localhost\"\nurl = ${host}");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::string("localhost".into())))
        );
    }

    #[test]
    fn resolves_nested_path_substitution() {
        let v = resolve_str("server { host = \"x\" }\nhost = ${server.host}");
        assert_eq!(
            obj(&v).get("host"),
            Some(&HoconValue::Scalar(ScalarValue::string("x".into())))
        );
    }

    #[test]
    fn resolves_optional_substitution_exists() {
        let v = resolve_str("a = 1\nb = ${?a}");
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::number("1".into())))
        );
    }

    #[test]
    fn drops_field_for_optional_missing() {
        let v = resolve_str("b = ${?missing}");
        assert_eq!(obj(&v).get("b"), None);
    }

    #[test]
    fn falls_back_to_prior_value() {
        let v = resolve_str("port = 50051\nport = ${?GRPC_PORT}");
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::number("50051".into())))
        );
    }

    #[test]
    fn uses_env_var_when_present() {
        let mut env = HashMap::new();
        env.insert("GRPC_PORT".into(), "9090".into());
        let v = resolve_str_with_env("port = 50051\nport = ${?GRPC_PORT}", &env);
        assert_eq!(
            obj(&v).get("port"),
            Some(&HoconValue::Scalar(ScalarValue::string("9090".into())))
        );
    }

    #[test]
    fn throws_on_unresolved_mandatory() {
        let tokens = tokenize("b = ${missing}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &ResolveOptions::new(HashMap::new())).is_err());
    }

    #[test]
    fn resolves_env_var_fallback() {
        let mut env = HashMap::new();
        env.insert("MY_VAR".into(), "hello".into());
        let v = resolve_str_with_env("b = ${MY_VAR}", &env);
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::string("hello".into())))
        );
    }

    #[test]
    fn resolves_self_referential_substitution() {
        let v = resolve_str("path = \"/usr\"\npath = ${path}:/extra");
        if let Some(HoconValue::Scalar(sv)) = obj(&v).get("path") {
            assert!(sv.raw.contains("/usr"));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn resolves_last_assignment_wins_for_substitution() {
        // b=${x} then b=${y} — ${b} should resolve to y's value (5), not x's ({q:10})
        let v = resolve_str("x={q:10}\ny=5\nb=${x}\nb=${y}");
        assert_eq!(
            obj(&v).get("b"),
            Some(&HoconValue::Scalar(ScalarValue::number("5".into())))
        );
    }

    #[test]
    fn resolves_string_concat_with_substitution() {
        let v = resolve_str("host = \"localhost\"\nurl = \"http://\"${host}");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::string(
                "http://localhost".into()
            )))
        );
    }

    #[test]
    fn throws_on_circular_substitution() {
        let tokens = tokenize("a = ${b}\nb = ${a}").unwrap();
        let ast = parse_tokens(&tokens).unwrap();
        assert!(resolve(ast, &ResolveOptions::new(HashMap::new())).is_err());
    }

    #[test]
    fn resolves_forward_reference() {
        let v = resolve_str("url = ${host}\nhost = \"localhost\"");
        assert_eq!(
            obj(&v).get("url"),
            Some(&HoconValue::Scalar(ScalarValue::string("localhost".into())))
        );
    }

    #[test]
    fn delayed_merge_object_with_substitution() {
        // a=${x} then a={c:3} should deep merge: {q:10, c:3}
        let v = resolve_str("x={q:10}\na=${x}\na={c:3}");
        let a = obj(&v).get("a").cloned().unwrap();
        match a {
            HoconValue::Object(map) => {
                assert_eq!(
                    map.get("c"),
                    Some(&HoconValue::Scalar(ScalarValue::number("3".into())))
                );
                assert_eq!(
                    map.get("q"),
                    Some(&HoconValue::Scalar(ScalarValue::number("10".into())))
                );
            }
            other => panic!("expected object, got {:?}", other),
        }
    }
}
