/// Phase 5 spec-compliance tests — per-impl mop-up (17 items).
///
/// Items covered:
///   S10.2  ✅, S10.15  ✅ (fixed by Phase 6 #3b S10.13 tightening), S10.17  ✅
///   S13.15, S13a.9, S13a.10, S13a.14
///   S14a.7, S14a.10, S14a.11
///   S18.3, S18.4
///   S19.8
///   S22.2, S22.3
///   S23.2
///   S26.2
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// S10.2  All arrays → array concatenation  (spec L312)
// Status: ✅  Fixed as a side effect of fix/s15-numeric-obj-array:
// the is_sep separator-skip in the array-concat branch discards the whitespace
// artefacts that used to leak between adjacent literal arrays.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s10_2_spec_array_concat() {
    let cfg = hocon::parse_with_env(r#"a = [1,2] [3,4]"#, &HashMap::new()).unwrap();
    let list = cfg.get_list("a").unwrap();
    assert_eq!(
        list.len(),
        4,
        "S10.2: [1,2] [3,4] must concat to a 4-element array"
    );
    // Elements should be 1, 2, 3, 4 in order
    if let hocon::HoconValue::Scalar(sv) = &list[0] {
        assert_eq!(sv.raw, "1");
    }
    if let hocon::HoconValue::Scalar(sv) = &list[3] {
        assert_eq!(sv.raw, "4");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// S10.15  Quoted whitespace between obj/array substitutions is an error (L442)
// Status: ✅ (incidentally fixed by Phase 6 #3b S10.13 tightening)
//
// The S10.13 fix raises an error whenever a scalar (including the quoted " ")
// appears between structured values. `${a} " " ${b}` with objects/arrays for
// a and b now fails at join_pair(object/array, " ") with a type-mismatch error.
// The S10.15 spec behavior (error on quoted whitespace between structured substs)
// is therefore satisfied as a side effect, even though the error fires for the
// S10.13 reason rather than a dedicated S10.15 whitespace check.
// ─────────────────────────────────────────────────────────────────────────────

/// S10.15 (objects): quoted whitespace between object substitutions now errors.
#[test]
fn s10_15_quoted_ws_between_obj_substs_is_error() {
    let r = hocon::parse_with_env(
        r#"
        a = {x:1}
        b = {y:2}
        c = ${a} " " ${b}
    "#,
        &HashMap::new(),
    );
    assert!(
        matches!(r, Err(hocon::HoconError::Resolve(_))),
        "S10.15: quoted whitespace between object substitutions must produce an error (spec L442)"
    );
}

/// S10.15 (arrays): quoted whitespace between array substitutions now errors.
#[test]
fn s10_15_quoted_ws_between_arr_substs_is_error() {
    let r = hocon::parse_with_env(
        r#"
        a = [1]
        b = [2]
        c = ${a} " " ${b}
    "#,
        &HashMap::new(),
    );
    assert!(
        matches!(r, Err(hocon::HoconError::Resolve(_))),
        "S10.15: quoted whitespace between array substitutions must produce an error (spec L442)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S10.17  Substitution resolving to array participates in array concat (L387)
// Status: ✅  Fixed as a side effect of fix/s15-numeric-obj-array (same root
// as S10.2 fix: is_sep separator-skip in the array-concat branch).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s10_17_spec_subst_array_concat() {
    let cfg = hocon::parse_with_env(
        r#"
        base = [1,2]
        combined = ${base} [3,4]
    "#,
        &HashMap::new(),
    )
    .unwrap();
    let list = cfg.get_list("combined").unwrap();
    assert_eq!(
        list.len(),
        4,
        "S10.17: ${{base}} [3,4] must produce a 4-element array"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S13.15  foo:${?bar}${?baz} skipped only when BOTH undefined (L640)
// Status: ❌
// ─────────────────────────────────────────────────────────────────────────────

/// Pin: when both bar and baz are undefined, foo is still created (null value).
#[test]
fn s13_15_pin_both_optional_undefined_field_exists() {
    let cfg = hocon::parse_with_env(r#"foo = ${?bar}${?baz}"#, &HashMap::new()).unwrap();
    // impl creates the field with a null value instead of dropping it
    let opt = cfg.get_string_option("foo");
    assert!(
        opt.is_some(),
        "[pin] S13.15: impl currently creates 'foo' even when both substs are undefined"
    );
    // The value is null (returned as "null" by get_string)
    assert_eq!(
        cfg.get_string("foo").unwrap(),
        "null",
        "[pin] S13.15: foo should be the null sentinel when both optional substs are undefined"
    );
}

#[test]
#[ignore = "spec violation per S13.15 (L640): foo:${?bar}${?baz} must not create field 'foo' when both bar and baz are undefined; impl creates null-valued field instead"]
fn s13_15_spec_both_optional_undefined_field_absent() {
    let cfg = hocon::parse_with_env(r#"foo = ${?bar}${?baz}"#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_string_option("foo").is_none(),
        "S13.15: 'foo' must not exist when both ${{?bar}} and ${{?baz}} are undefined (spec L640)"
    );
}

/// Positive: when only one is undefined, field IS created (with the defined one's value).
#[test]
fn s13_15_one_defined_field_is_created() {
    let cfg = hocon::parse_with_env(
        r#"bar = hello
foo = ${?bar}${?baz}"#,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(
        cfg.get_string("foo").unwrap(),
        "hello",
        "S13.15: when bar is defined, foo must be created with bar's value"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S13a.9  Multi-step cycle a→b→c→a → error  (L862)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s13a_9_multi_step_cycle_is_error() {
    let r = hocon::parse_with_env(
        r#"
        a = ${b}
        b = ${c}
        c = ${a}
    "#,
        &HashMap::new(),
    );
    assert!(
        r.is_err(),
        "S13a.9: three-step cycle a→b→c→a must produce an error (spec L862)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S13a.10  Substitution memoized by instance, not by path  (L885)
// Status: ✅  (observable outcome: a and b end up equal)
// ─────────────────────────────────────────────────────────────────────────────

/// The spec (L883) says for the undefined-order case `a=1,b=2,a=${b},b=${a}`,
/// both a and b must end up with the SAME value (memoization guarantee).
/// The implementation resolves this correctly (both become 2).
#[test]
fn s13a_10_memoization_same_value() {
    let cfg = hocon::parse_with_env(
        r#"
        a = 1
        b = 2
        a = ${b}
        b = ${a}
    "#,
        &HashMap::new(),
    )
    .unwrap();
    let a = cfg.get_i64("a").unwrap();
    let b = cfg.get_i64("b").unwrap();
    assert_eq!(
        a, b,
        "S13a.10: a and b must resolve to the same value (memoization); got a={}, b={}",
        a, b
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S13a.14  Mutually-referring object fields resolve lazily without false cycle
//          (L825-834)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s13a_14_mutual_refs_no_false_cycle() {
    // From spec example L830-835:
    //   bar.a should be 4  (resolves ${foo.d} → 4 after foo.d override)
    //   foo.c should be 3  (resolves ${bar.b} → 3 after bar.b override)
    let cfg = hocon::parse_with_env(
        r#"
        bar : { a : ${foo.d}, b : 1 }
        bar.b = 3
        foo : { c : ${bar.b}, d : 2 }
        foo.d = 4
    "#,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(cfg.get_i64("bar.a").unwrap(), 4, "S13a.14: bar.a must be 4");
    assert_eq!(cfg.get_i64("foo.c").unwrap(), 3, "S13a.14: foo.c must be 3");
}

// ─────────────────────────────────────────────────────────────────────────────
// S14a.7  Whitespace (including newlines) between `include` and resource (L952)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s14a_7_whitespace_before_include_arg() {
    let dir = std::env::temp_dir().join("hocon_s14a7_ws");
    std::fs::create_dir_all(&dir).unwrap();
    let inc = dir.join("inc.conf");
    std::fs::write(&inc, r#"x = 42"#).unwrap();
    let inc_path = inc.to_str().unwrap().replace('\\', "/");

    // Extra spaces between include keyword and quoted arg
    let input = format!("include   \"{}\"\n", inc_path);
    let cfg = hocon::parse_with_env(&input, &HashMap::new()).unwrap();
    assert_eq!(
        cfg.get_i64("x").unwrap(),
        42,
        "S14a.7: extra spaces allowed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s14a_7_newline_before_include_arg() {
    let dir = std::env::temp_dir().join("hocon_s14a7_nl");
    std::fs::create_dir_all(&dir).unwrap();
    let inc = dir.join("inc.conf");
    std::fs::write(&inc, r#"x = 99"#).unwrap();
    let inc_path = inc.to_str().unwrap().replace('\\', "/");

    // Newline between include keyword and quoted arg (spec L952 says newlines allowed)
    let input = format!("include\n\"{}\"\n", inc_path);
    let cfg = hocon::parse_with_env(&input, &HashMap::new()).unwrap();
    assert_eq!(
        cfg.get_i64("x").unwrap(),
        99,
        "S14a.7: newline between include and arg allowed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ─────────────────────────────────────────────────────────────────────────────
// S14a.10  Include argument must be a quoted string  (L958)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s14a_10_unquoted_include_arg_rejected() {
    let r = hocon::parse_with_env(r#"include some_file.conf"#, &HashMap::new());
    assert!(
        r.is_err(),
        "S14a.10: unquoted include argument must be rejected (spec L958)"
    );
    let err = r.unwrap_err().to_string();
    assert!(
        err.contains("expected include path") || err.contains("Unquoted"),
        "S14a.10: error message should mention unquoted; got: {}",
        err
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S14a.11  `"include"` (quoted) is just a normal key  (L977)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s14a_11_quoted_include_is_normal_key() {
    let cfg = hocon::parse_with_env(r#""include" = 42"#, &HashMap::new()).unwrap();
    assert_eq!(
        cfg.get_i64("include").unwrap(),
        42,
        "S14a.11: quoted 'include' must be treated as a normal key (spec L977)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S18.3  Unit name letters-only (Unicode L* / isLetter)  (L1287)
// Status: ✅  (impl rejects units containing digits or hyphens)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s18_3_unit_with_digit_rejected() {
    let cfg = hocon::parse_with_env(r#"t = "100 ms2""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_err(),
        "S18.3: unit 'ms2' (contains digit) must be rejected per spec L1287"
    );
}

#[test]
fn s18_3_unit_with_hyphen_rejected() {
    let cfg = hocon::parse_with_env(r#"t = "100 milli-seconds""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_err(),
        "S18.3: unit 'milli-seconds' (contains hyphen) must be rejected per spec L1287"
    );
}

#[test]
fn s18_3_valid_letter_only_unit_accepted() {
    let cfg = hocon::parse_with_env(r#"t = "100 ms""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_ok(),
        "S18.3: unit 'ms' (letters only) must be accepted"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S18.4  String with no unit → interpreted with default unit  (L1290)
// Status: ⚠️  — bytes ✅, duration ❌
// ─────────────────────────────────────────────────────────────────────────────

/// Bytes: string "1024" with no unit → 1024 bytes (default unit = bytes). ✅
#[test]
fn s18_4_bytes_string_no_unit_uses_default() {
    let cfg = hocon::parse_with_env(r#"s = "1024""#, &HashMap::new()).unwrap();
    assert_eq!(
        cfg.get_bytes("s").unwrap(),
        1024,
        "S18.4: bytes string with no unit must use default (bytes)"
    );
}

/// Duration pin: string "500" with no unit currently errors (does not use default ms).
#[test]
fn s18_4_pin_duration_string_no_unit_errors() {
    let cfg = hocon::parse_with_env(r#"t = "500""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_err(),
        "[pin] S18.4: impl currently errors on duration string without unit (should use default ms per spec L1290)"
    );
    let err = cfg.get_duration("t").unwrap_err().to_string();
    assert!(
        err.contains("invalid duration"),
        "[pin] S18.4: error message should contain 'invalid duration'; got: {}",
        err
    );
}

#[test]
#[ignore = "spec violation per S18.4 (L1290): duration string '500' with no unit must be interpreted as milliseconds (default unit); impl errors instead"]
fn s18_4_spec_duration_string_no_unit_uses_default() {
    let cfg = hocon::parse_with_env(r#"t = "500""#, &HashMap::new()).unwrap();
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "S18.4: string '500' without unit must default to milliseconds"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S19.8  Duration unit names are case sensitive (lowercase only)  (L1304)
// Status: ❌  — impl lowercases the unit before matching, so uppercase passes
// ─────────────────────────────────────────────────────────────────────────────

/// Pin: impl currently accepts uppercase "MS" as milliseconds.
#[test]
fn s19_8_pin_uppercase_ms_accepted() {
    let cfg = hocon::parse_with_env(r#"t = "100 MS""#, &HashMap::new()).unwrap();
    // impl succeeds — pin that it does succeed (wrong behavior)
    assert!(
        cfg.get_duration("t").is_ok(),
        "[pin] S19.8: impl currently accepts uppercase 'MS' (should reject per spec L1304)"
    );
}

#[test]
#[ignore = "spec violation per S19.8 (L1304): duration unit names are case sensitive and must be lowercase; impl lowercases unit before matching, so 'MS', 'Seconds', 'NS' etc. are wrongly accepted"]
fn s19_8_spec_uppercase_ms_rejected() {
    let cfg = hocon::parse_with_env(r#"t = "100 MS""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_err(),
        "S19.8: uppercase 'MS' must be rejected (only lowercase duration units allowed)"
    );
}

#[test]
fn s19_8_pin_mixed_case_seconds_accepted() {
    let cfg = hocon::parse_with_env(r#"t = "100 Seconds""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_ok(),
        "[pin] S19.8: impl currently accepts 'Seconds' (should reject per spec L1304)"
    );
}

#[test]
#[ignore = "spec violation per S19.8 (L1304): 'Seconds' (mixed case) must be rejected; impl accepts it due to .to_lowercase() in parse_duration"]
fn s19_8_spec_mixed_case_seconds_rejected() {
    let cfg = hocon::parse_with_env(r#"t = "100 Seconds""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_err(),
        "S19.8: 'Seconds' must be rejected (spec requires lowercase only)"
    );
}

/// Correct lowercase units still work.
#[test]
fn s19_8_lowercase_units_accepted() {
    let cfg = hocon::parse_with_env(r#"t = "100 ms""#, &HashMap::new()).unwrap();
    assert!(
        cfg.get_duration("t").is_ok(),
        "S19.8: lowercase 'ms' must be accepted"
    );
    let cfg2 = hocon::parse_with_env(r#"t = "100 ns""#, &HashMap::new()).unwrap();
    assert!(
        cfg2.get_duration("t").is_ok(),
        "S19.8: lowercase 'ns' must be accepted"
    );
    let cfg3 = hocon::parse_with_env(r#"t = "30 seconds""#, &HashMap::new()).unwrap();
    assert!(
        cfg3.get_duration("t").is_ok(),
        "S19.8: lowercase 'seconds' must be accepted"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S22.2  Intermediate non-object hides earlier object across files  (L1406)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

/// From spec L1410-1417: merge A={a:{x:1}}, B={a:42}, C={a:{y:2}} in priority order
/// (A highest). Because B=42 is paired with C={y:2} and 42 simply wins, the two
/// objects are not adjacent and their merge is lost.
/// Result: {a:{x:1}}.
#[test]
fn s22_2_non_object_hides_earlier_object_across_merge() {
    let cfg_a = hocon::parse_with_env(r#"a { x = 1 }"#, &HashMap::new()).unwrap();
    let cfg_b = hocon::parse_with_env(r#"a = 42"#, &HashMap::new()).unwrap();
    let cfg_c = hocon::parse_with_env(r#"a { y = 2 }"#, &HashMap::new()).unwrap();

    // Merge: cfg_a highest priority, cfg_b middle, cfg_c lowest
    let mid = cfg_b.with_fallback(&cfg_c);
    let merged = cfg_a.with_fallback(&mid);

    // a.x from cfg_a is accessible
    assert_eq!(
        merged.get_i64("a.x").unwrap(),
        1,
        "S22.2: a.x from the highest-priority object must be present"
    );
    // a.y from cfg_c must NOT be accessible (hidden by cfg_b's non-object 42)
    assert!(
        merged.get_i64("a.y").is_err(),
        "S22.2: a.y must not be accessible — cfg_b's a=42 hides cfg_c's a.y (spec L1406)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S22.3  Setting key to null clears earlier object value  (L1436)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

/// Spec L1436: setting a key to null clears earlier object values when merging.
/// Merge A={a:null} over B={a:{x:1}} → a is null, a.x is not accessible.
#[test]
fn s22_3_null_clears_earlier_object_in_merge() {
    let cfg_a = hocon::parse_with_env(r#"a = null"#, &HashMap::new()).unwrap();
    let cfg_b = hocon::parse_with_env(r#"a { x = 1 }"#, &HashMap::new()).unwrap();

    let merged = cfg_a.with_fallback(&cfg_b);

    // a.x must not be accessible — null cleared the object
    assert!(
        merged.get_i64("a.x").is_err(),
        "S22.3: a.x must not be accessible after a=null clears the earlier object (spec L1436)"
    );
    // a itself is null (get_string returns "null" due to S17.6 bug, which is separate)
    assert!(
        merged.get_string("a").is_ok(),
        "S22.3: a is still present as null value"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S23.2  Empty path elements (leading/trailing) preserved in properties  (L1456)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

/// Spec L1456: "a." splits to ["a", ""] — the trailing empty segment is preserved.
/// Verified via include of a .properties file.
#[test]
fn s23_2_trailing_dot_creates_empty_key_segment() {
    let dir = std::env::temp_dir().join("hocon_s23_2_trail");
    std::fs::create_dir_all(&dir).unwrap();
    // "a. = v" → key "a." splits to ["a", ""] → a."" = v
    let props = dir.join("trail.properties");
    std::fs::write(&props, "a. = trailing_dot\n").unwrap();

    let props_str = props.to_str().unwrap().replace('\\', "/");
    let hocon_input = format!("include \"{}\"\n", props_str);
    let r = hocon::parse_with_env(&hocon_input, &HashMap::new());
    // impl succeeds and creates a nested object under "a"
    assert!(
        r.is_ok(),
        "S23.2: properties with trailing dot key must parse"
    );
    let cfg = r.unwrap();
    // "a" is an object (not a scalar), the empty-string key lives inside it.
    // get_config("a") should succeed AND the value must be retrievable via the
    // trailing-dot accessor "a." (which split_config_path treats as path
    // ["a", ""] — the empty trailing segment maps to the empty-string key).
    assert!(
        cfg.get_config("a").is_ok(),
        "S23.2: 'a' must be an object (trailing dot creates nested obj with empty key)"
    );
    assert_eq!(
        cfg.get_string("a.").unwrap(),
        "trailing_dot",
        "S23.2: 'a.' in properties must create path [\"a\", \"\"] accessible as 'a.'"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s23_2_leading_dot_creates_empty_key_segment() {
    let dir = std::env::temp_dir().join("hocon_s23_2_lead");
    std::fs::create_dir_all(&dir).unwrap();
    // ".a = v" → key ".a" splits to ["", "a"] → ""."a" = v
    let props = dir.join("lead.properties");
    std::fs::write(&props, ".a = leading_dot\n").unwrap();

    let props_str = props.to_str().unwrap().replace('\\', "/");
    let hocon_input = format!("include \"{}\"\n", props_str);
    let r = hocon::parse_with_env(&hocon_input, &HashMap::new());
    assert!(
        r.is_ok(),
        "S23.2: properties with leading dot key must parse"
    );
    let cfg = r.unwrap();
    // The empty-string root key is present in the internal map.
    // The accessor path ".a" (dot-prefixed) reaches the value because split_config_path
    // treats the leading dot as a path separator producing ["", "a"].
    assert_eq!(
        cfg.get_string(".a").unwrap(),
        "leading_dot",
        "S23.2: '.a' in properties must create path [\"\", \"a\"] accessible as '.a'"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ─────────────────────────────────────────────────────────────────────────────
// S26.2  Empty env var preserved as empty string (not undefined)  (L1558)
// Status: ✅
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn s26_2_empty_env_var_preserved_as_empty_string() {
    let mut env = HashMap::new();
    env.insert("MY_VAR".to_string(), "".to_string());

    let cfg = hocon::parse_with_env(r#"v = ${MY_VAR}"#, &env).unwrap();
    let val = cfg.get_string("v").unwrap();
    assert_eq!(
        val, "",
        "S26.2: env var set to empty string must remain as empty string (not undefined) — spec L1558"
    );
}
