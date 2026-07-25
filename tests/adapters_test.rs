//! Format adapters — the tree-level rules this crate owns (spec F0/F1/F3/F4/F5).
#![cfg(feature = "adapters")]

use std::collections::HashMap;

use hocon::adapters::{env, jsonc, properties, toml, yaml};

fn env_opts(prefix: &str) -> env::Options {
    env::Options {
        prefix: prefix.to_string(),
        ..Default::default()
    }
}

#[test]
fn properties_nests_and_shares_the_include_syntax_layer() {
    let cfg = properties::parse("db.host = db.internal\na = one\\\ntwo\n", None).unwrap();
    assert_eq!(cfg.get_string("db.host").unwrap(), "db.internal");
    assert_eq!(cfg.get_string("a").unwrap(), "onetwo");
}

/// F0.2 — the file belongs to another program, so `${...}` is data.
#[test]
fn properties_leaves_substitution_syntax_literal() {
    let cfg = properties::parse("a = ${foo.bar}", None).unwrap();
    assert_eq!(cfg.get_string("a").unwrap(), "${foo.bar}");
}

/// F1.2/F1.3 — `__` separates, a single `_` does not, segments lowercase.
#[test]
fn env_mounts_a_prefixed_namespace() {
    let vars: HashMap<String, String> = [
        ("APP_DB__HOST", "db.internal"),
        ("APP_DB__MAX_CONN", "10"),
        ("APP_NAME", "svc"),
        ("PATH", "/usr/bin"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();

    let cfg = env::load_from(&vars, env_opts("APP_")).unwrap();
    assert_eq!(cfg.get_string("db.host").unwrap(), "db.internal");
    assert_eq!(cfg.get_string("db.max_conn").unwrap(), "10");
    assert_eq!(cfg.get_string("name").unwrap(), "svc");
    assert!(cfg.get("path").is_none());
}

/// F1.1 — mounting everything would pull in unrelated secrets.
#[test]
fn env_requires_a_prefix() {
    let err = env::load_from(&HashMap::new(), env::Options::default()).unwrap_err();
    assert!(err.message.contains("F1.1"), "{}", err.message);
}

/// F1.6 — the environment has no order to break a tie with.
#[test]
fn env_refuses_a_collision() {
    let vars: HashMap<String, String> = [("APP_A__B", "1"), ("APP_a__b", "2")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let err = env::load_from(&vars, env_opts("APP_")).unwrap_err();
    assert_eq!(err.message, "env: APP_A__B and APP_a__b both map to a.b");
}

/// The collision message renders the path as a HOCON path expression: a
/// segment that is not safely bare is quoted, and quoted exactly once. The
/// earlier version wrapped `display_path`'s own output in a second pair of
/// quotes, producing the unparseable `a."foo.bar""`.
#[test]
fn env_collision_message_quotes_a_dotted_segment_exactly_once() {
    let vars: HashMap<String, String> = [("APP_A__FOO.BAR", "1"), ("APP_a__FOO.BAR", "2")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let err = env::load_from(&vars, env_opts("APP_")).unwrap_err();
    assert_eq!(
        err.message,
        "env: APP_A__FOO.BAR and APP_a__FOO.BAR both map to a.\"foo.bar\""
    );
}

/// Two distinct paths must never render identically, or a collision report
/// cannot be acted on. A segment holding a literal `"` is escaped.
#[test]
fn env_collision_message_escapes_quotes_in_a_segment() {
    let vars: HashMap<String, String> = [("APP_A\"B", "1"), ("APP_a\"b", "2")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let err = env::load_from(&vars, env_opts("APP_")).unwrap_err();
    assert!(
        err.message.ends_with(r#"both map to "a\"b""#),
        "{}",
        err.message
    );
}

/// F1.3 — ASCII-only folding. Rust's full Unicode `to_lowercase` maps `İ`
/// (U+0130) to `i` + U+0307, which would make `APP_İ` collide with `APP_I`;
/// Go's simple mapping would not. Pinning to ASCII keeps the four
/// implementations in agreement.
#[test]
fn env_lowercases_ascii_only() {
    let vars: HashMap<String, String> = [("APP_\u{130}", "dotted"), ("APP_I", "plain")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let cfg = env::load_from(&vars, env_opts("APP_")).expect("must not collide");
    assert_eq!(cfg.get_string("\"\u{130}\"").unwrap(), "dotted");
    assert_eq!(cfg.get_string("i").unwrap(), "plain");
}

/// A deep path must be refused, not built. `set_nested` recurses per segment
/// and the resulting tree drops recursively, so an unbounded depth overflows
/// the stack — an abort, which no `catch_unwind` can contain.
#[test]
fn env_refuses_an_absurdly_deep_path() {
    let name = format!("APP_{}", vec!["a"; 5000].join("__"));
    let vars: HashMap<String, String> = [(name, "v".to_string())].into_iter().collect();
    let err = env::load_from(&vars, env_opts("APP_")).unwrap_err();
    assert!(err.message.contains("segments deep"), "{}", err.message);
}

/// The same cap protects `parse_dotenv`, which takes arbitrary file text.
#[test]
fn dotenv_refuses_an_absurdly_deep_path() {
    let src = format!("{}=v\n", vec!["a"; 5000].join("__"));
    let err = env::parse_dotenv(&src, env::Options::default()).unwrap_err();
    assert!(err.message.contains("segments deep"), "{}", err.message);
}

/// F1.2 — a literal `.` in the variable name is key text, not a boundary;
/// only `__` creates hierarchy.
#[test]
fn env_keeps_a_literal_dot_as_key_text() {
    let vars: HashMap<String, String> = [("APP_FOO.BAR", "flat")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let cfg = env::load_from(&vars, env_opts("APP_")).unwrap();
    assert_eq!(cfg.get_string("\"foo.bar\"").unwrap(), "flat");
    assert!(cfg.get("foo").is_none(), "the dot must not nest");
}

/// F1.2/F1.6 — `APP_FOO.BAR` and `APP_FOO__BAR` are distinct paths, so they
/// coexist rather than colliding.
#[test]
fn env_literal_dot_does_not_collide_with_the_separator() {
    let vars: HashMap<String, String> = [("APP_FOO.BAR", "flat"), ("APP_FOO__BAR", "nested")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let cfg = env::load_from(&vars, env_opts("APP_")).unwrap();
    assert_eq!(cfg.get_string("\"foo.bar\"").unwrap(), "flat");
    assert_eq!(cfg.get_string("foo.bar").unwrap(), "nested");
}

/// F1.7 — a deliberately small dialect.
#[test]
fn dotenv_reads_the_small_dialect() {
    let src = "# comment\nexport FOO=bar\nDB__HOST=db.internal\nQUOTED=\"a\\nb\"\nSINGLE='raw ${x} #hash'\nHASH=#fff\n";
    let cfg = env::parse_dotenv(src, env::Options::default()).unwrap();
    assert_eq!(cfg.get_string("foo").unwrap(), "bar");
    assert_eq!(cfg.get_string("db.host").unwrap(), "db.internal");
    assert_eq!(cfg.get_string("quoted").unwrap(), "a\nb");
    assert_eq!(cfg.get_string("single").unwrap(), "raw ${x} #hash");
    assert_eq!(cfg.get_string("hash").unwrap(), "#fff");
}

/// F1.2 applies to `.env` files the same way: the dot stays in the key.
#[test]
fn dotenv_keeps_a_literal_dot_as_key_text() {
    let cfg = env::parse_dotenv("A.B=v\n", env::Options::default()).unwrap();
    assert_eq!(cfg.get_string("\"a.b\"").unwrap(), "v");
    assert!(cfg.get("a").is_none(), "the dot must not nest");
}

#[test]
fn dotenv_refuses_an_ambiguous_trailing_hash() {
    let err = env::parse_dotenv("FOO=bar # comment", env::Options::default()).unwrap_err();
    assert!(err.message.contains("quote the value"), "{}", err.message);
}

#[test]
fn jsonc_accepts_comments_and_trailing_commas() {
    let cfg = jsonc::parse(
        r#"{
          // line
          "a": 1, /* block */
          "b": [1, 2,],
          "c": { "d": true, },
        }"#,
        None,
    )
    .unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert!(cfg.get_bool("c.d").unwrap());
}

/// F3.2 — a comment is replaced by whitespace, never the empty string, so it
/// can never splice its neighbors into one token.
#[test]
fn jsonc_block_comment_between_tokens_does_not_join_them() {
    let err = jsonc::parse(r#"{"a": 1/*x*/2}"#, None).unwrap_err();
    assert!(err.message.contains("jsonc"), "{}", err.message);
    let err = jsonc::parse(r#"{"a": tr/*x*/ue}"#, None).unwrap_err();
    assert!(err.message.contains("jsonc"), "{}", err.message);
}

/// The replacement whitespace must stay invisible to JSON itself: a comment
/// in every legal position still parses, including tight against a comma.
#[test]
fn jsonc_block_comment_in_normal_positions_still_parses() {
    let cfg = jsonc::parse("{\"a\":/*x*/1/*y*/,\"b\":/* multi\nline */true}", None).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert!(cfg.get_bool("b").unwrap());
}

/// F3.2 — a lone CR terminates a `//` comment. Otherwise the first `//` in a
/// CR-delimited file swallows the whole rest of the document.
#[test]
fn jsonc_line_comment_ends_at_a_lone_cr() {
    let cfg = jsonc::parse("{\"a\":1 //c\r,\"b\":2}", None).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);
    assert_eq!(cfg.get_i64("b").unwrap(), 2);
}

#[test]
fn jsonc_leaves_comment_markers_inside_strings() {
    let cfg = jsonc::parse(r#"{"url": "https://example.com/a//b"}"#, None).unwrap();
    assert_eq!(cfg.get_string("url").unwrap(), "https://example.com/a//b");
}

/// F0.3 — a config root has to be an object.
#[test]
fn jsonc_refuses_a_non_object_root() {
    let err = jsonc::parse("[1, 2]", None).unwrap_err();
    assert!(err.message.contains("F0.3"), "{}", err.message);
}

/// F0.9 — a leading BOM must never end up inside the first key. Windows
/// editors emit one, and `"\u{feff}a"` makes a lookup of `a` miss while the
/// document still parses: plausible-but-wrong, the worst outcome available.
#[test]
fn adapters_strip_a_leading_bom() {
    const BOM: &str = "\u{feff}";

    let cfg = jsonc::parse(&format!("{BOM}{{\"a\": 1}}"), None).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);

    let cfg = toml::parse(&format!("{BOM}a = 1"), None).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);

    let cfg = yaml::parse(&format!("{BOM}a: 1"), None).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 1);

    let cfg = properties::parse(&format!("{BOM}a = 1"), None).unwrap();
    assert_eq!(cfg.get_string("a").unwrap(), "1");

    let cfg = env::parse_dotenv(&format!("{BOM}A=1"), env::Options::default()).unwrap();
    assert_eq!(cfg.get_string("a").unwrap(), "1");
}

#[test]
fn toml_maps_tables_and_arrays_of_tables() {
    let cfg = toml::parse(
        "name = \"svc\"\nport = 8080\n[db]\nhost = \"localhost\"\n[[db.replicas]]\nid = 1\n[[db.replicas]]\nid = 2\n",
        None,
    )
    .unwrap();
    assert_eq!(cfg.get_string("name").unwrap(), "svc");
    assert_eq!(cfg.get_i64("port").unwrap(), 8080);
    assert_eq!(cfg.get_string("db.host").unwrap(), "localhost");
}

/// F4.2 — HOCON has no datetime, so dates are their RFC 3339 text.
#[test]
fn toml_renders_datetimes_as_strings() {
    let cfg = toml::parse(
        "a = 1979-05-27T07:32:00Z\nb = 1979-05-27\nc = 07:32:00\n",
        None,
    )
    .unwrap();
    assert_eq!(cfg.get_string("a").unwrap(), "1979-05-27T07:32:00Z");
    assert_eq!(cfg.get_string("b").unwrap(), "1979-05-27");
    assert_eq!(cfg.get_string("c").unwrap(), "07:32:00");
}

#[test]
fn toml_refuses_infinity() {
    let err = toml::parse("a = inf", None).unwrap_err();
    assert!(err.message.contains("F0.6"), "{}", err.message);
}

#[test]
fn yaml_maps_scalars_mappings_and_sequences() {
    let cfg = yaml::parse("name: svc\nport: 8080\ndb:\n  host: localhost\n", None).unwrap();
    assert_eq!(cfg.get_string("name").unwrap(), "svc");
    assert_eq!(cfg.get_i64("port").unwrap(), 8080);
    assert_eq!(cfg.get_string("db.host").unwrap(), "localhost");
}

/// F5.2 — `yaml-rust2` leaves `<<` as an ordinary key, so merging is ours to
/// do; a `<<` leaking through as a field would be a structural difference.
#[test]
fn yaml_resolves_merge_keys_and_aliases() {
    let cfg = yaml::parse(
        "d: &d\n  host: h\n  port: 1\np:\n  <<: *d\n  port: 2\ncopy: *d\n",
        None,
    )
    .unwrap();
    assert_eq!(cfg.get_string("p.host").unwrap(), "h");
    assert_eq!(cfg.get_i64("p.port").unwrap(), 2, "explicit key must win");
    assert_eq!(cfg.get_i64("copy.port").unwrap(), 1);
    assert!(cfg.get("p.<<").is_none(), "merge key must not leak through");
}

/// F5.3 — non-string scalar keys map to their string forms.
#[test]
fn yaml_stringifies_non_string_keys() {
    let cfg = yaml::parse("1: one\ntrue: t\n", None).unwrap();
    assert_eq!(cfg.get_string("\"1\"").unwrap(), "one");
    assert_eq!(cfg.get_string("\"true\"").unwrap(), "t");
}

/// F5.7 — decoding one document and dropping the rest would be silent loss.
#[test]
fn yaml_refuses_a_multi_document_stream() {
    let err = yaml::parse("a: 1\n---\nb: 2\n", None).unwrap_err();
    assert!(err.message.contains("F5.7"), "{}", err.message);
}

/// F5.9 — an empty document is the empty object, as in HOCON itself (S3.1).
#[test]
fn yaml_empty_document_is_the_empty_object() {
    let cfg = yaml::parse("", None).unwrap();
    assert!(cfg.keys().is_empty());
}

#[test]
fn yaml_refuses_nan_and_a_sequence_root() {
    assert!(yaml::parse("a: .nan", None)
        .unwrap_err()
        .message
        .contains("F0.6"));
    assert!(yaml::parse("- 1\n- 2", None)
        .unwrap_err()
        .message
        .contains("F0.3"));
}

/// The tree-level entry point: the caller decodes with whatever library and
/// settings they chose, and the same rules apply to what they hand over.
#[test]
fn yaml_from_value_accepts_an_externally_decoded_tree() {
    let docs = yaml_rust2::YamlLoader::load_from_str("db:\n  host: h\n").unwrap();
    let cfg = yaml::from_value(&docs[0], Some("via-caller")).unwrap();
    assert_eq!(cfg.get_string("db.host").unwrap(), "h");
}

/// The reason the adapters exist: a HOCON document reading values out of a
/// file some other tool owns.
#[test]
fn used_as_a_substitution_source_under_hocon() {
    let base = yaml::parse("services:\n  db:\n    image: postgres:16\n", None).unwrap();
    // Substitutions must stay unresolved until the fallback is attached.
    let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
    let cfg = hocon::parse_string_with_options("image = ${services.db.image}", opts).unwrap();
    let merged = cfg
        .with_fallback(&base)
        .resolve(hocon::ResolveOptions::defaults())
        .unwrap();
    assert_eq!(merged.get_string("image").unwrap(), "postgres:16");
}
