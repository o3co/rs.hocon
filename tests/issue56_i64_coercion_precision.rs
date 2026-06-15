//! xx.hocon#56: integer coercion of a float-like scalar must derive wholeness
//! from the raw decimal text, not from an intermediate `f64`. Above 2^52 an
//! `f64` cannot represent fractional parts, so the previous `f.fract() == 0.0`
//! check both (a) false-accepted non-whole values and (b) off-by-one'd would-be
//! whole values. Pinned across all three i64 coercion surfaces (`Config::get_i64`,
//! `HoconValue::as_i64`, serde `get_as::<i64>`).

use hocon::parse;

#[test]
fn rejects_non_whole_float_at_2_pow_53() {
    // 2^53 + 0.5: f64 rounds to 9007199254740992.0, so fract()==0.0 false-accepts.
    let c = parse("n = 9007199254740992.5").unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), None);
    assert!(c.get_i64("n").is_err());
}

#[test]
fn rejects_non_whole_half_integer_in_2_pow_52_window() {
    // 2^52 + 0.5: ulp is already 1 here, so the .5 is lost to rounding — a
    // magnitude threshold of 2^53 would miss this; raw-string wholeness catches it.
    let c = parse("n = 4503599627370496.5").unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), None);
    assert!(c.get_i64("n").is_err());
}

#[test]
fn accepts_exact_whole_float_above_2_pow_53_without_off_by_one() {
    // 2^53 + 1 written as a whole float: via f64 this rounds to 2^53 (off by one).
    // Raw-string parsing yields the exact integer.
    let c = parse("n = 9007199254740993.0").unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), Some(9007199254740993));
    assert_eq!(c.get_i64("n").unwrap(), 9007199254740993);
}

#[test]
fn accepts_large_whole_exponent_form() {
    // 1e16 (> 2^53) is a whole number; it must still coerce (no regression).
    let c = parse("n = 1e16").unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), Some(10000000000000000));
    assert_eq!(c.get_i64("n").unwrap(), 10000000000000000);
}

#[test]
fn small_whole_floats_still_coerce_and_non_whole_reject() {
    let c = parse(
        r#"a = 1.0
           b = 1e3
           c = 1.5e1
           d = 1.5
           e = 1.234e2"#,
    )
    .unwrap();
    assert_eq!(c.get("a").unwrap().as_i64(), Some(1));
    assert_eq!(c.get("b").unwrap().as_i64(), Some(1000));
    assert_eq!(c.get("c").unwrap().as_i64(), Some(15)); // 1.5e1 = 15 (whole)
    assert_eq!(c.get("d").unwrap().as_i64(), None);
    assert_eq!(c.get("e").unwrap().as_i64(), None); // 123.4 (non-whole)
}

#[test]
fn negative_exponent_wholeness_is_text_based() {
    let c = parse(
        r#"a = 1e-3
           b = 1000e-3
           c = 1500e-3"#,
    )
    .unwrap();
    assert_eq!(c.get("a").unwrap().as_i64(), None); // 0.001
    assert_eq!(c.get("b").unwrap().as_i64(), Some(1)); // 1.0
    assert_eq!(c.get("c").unwrap().as_i64(), None); // 1.5
}

#[test]
fn overflow_float_form_rejects() {
    let c = parse("n = 1e30").unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), None);
    assert!(c.get_i64("n").is_err());
}

#[test]
fn i64_min_and_max_float_form_preserved() {
    // i64::MIN's magnitude is 2^63, which does not fit i64 — the sign must be
    // applied with a range check, not via `int.parse::<i64>()` on the magnitude.
    let c = parse(
        r#"min = -9223372036854775808.0
           max = 9223372036854775807.0"#,
    )
    .unwrap();
    assert_eq!(c.get("min").unwrap().as_i64(), Some(i64::MIN));
    assert_eq!(c.get_i64("min").unwrap(), i64::MIN);
    assert_eq!(c.get("max").unwrap().as_i64(), Some(i64::MAX));
    // one past i64::MAX as a whole float must reject, not wrap
    let c2 = parse("n = 9223372036854775808.0").unwrap();
    assert_eq!(c2.get("n").unwrap().as_i64(), None);
}

#[test]
fn huge_exponent_rejects_without_huge_allocation() {
    // Must return None quickly, not attempt a multi-GB zero-padded string.
    let c = parse("n = 1e2147483647").unwrap();
    assert_eq!(c.get("n").unwrap().as_i64(), None);
    assert!(c.get_i64("n").is_err());
    let c2 = parse("n = 1e-2147483648").unwrap();
    assert_eq!(c2.get("n").unwrap().as_i64(), None); // ~0, non-whole
}

#[test]
fn negative_exponent_consistent_across_surfaces() {
    let c = parse("a = 1000e-3").unwrap();
    assert_eq!(c.get("a").unwrap().as_i64(), Some(1));
    assert_eq!(c.get_i64("a").unwrap(), 1);
    #[cfg(feature = "serde")]
    assert_eq!(c.get_as::<i64>("a").unwrap(), 1);
}

#[test]
fn leading_zero_float_forms_coerce() {
    let c = parse(
        r#"a = 0100.0
           b = 0001e3"#,
    )
    .unwrap();
    assert_eq!(c.get("a").unwrap().as_i64(), Some(100));
    assert_eq!(c.get("b").unwrap().as_i64(), Some(1000));
}

#[cfg(feature = "serde")]
#[test]
fn serde_get_as_i64_matches() {
    let c = parse(
        r#"bad = 9007199254740992.5
           good = 9007199254740993.0"#,
    )
    .unwrap();
    assert!(c.get_as::<i64>("bad").is_err());
    assert_eq!(c.get_as::<i64>("good").unwrap(), 9007199254740993);
}
