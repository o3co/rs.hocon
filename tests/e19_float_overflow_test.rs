// E19 — overflowing float literal is a parse error (xx.hocon#97, posture B).
//
// A numeric literal whose magnitude overflows the f64 range is a parse error
// in all four sibling implementations. This is a documented divergence from
// Lightbend: the reference admits the literal as Infinity, but HOCON has no
// Infinity literal, so the value cannot be rendered or re-parsed as a number
// (Lightbend's own render -> re-parse silently turns it into the STRING
// "Infinity"). go.hocon has always errored here via strconv.ParseFloat;
// ts/py/rs align with it. Port of ts.hocon's e19-float-overflow.test.ts.
//
// Underflow is NOT an error: `1e-400` reads as 0 in every implementation and
// in Lightbend, so only the infinite case is rejected.

use hocon::HoconError;

fn parse_err(src: &str) -> HoconError {
    hocon::parse(src).expect_err("expected a parse error")
}

#[test]
fn rejects_overflow() {
    match parse_err("a = 1e999") {
        HoconError::Parse(e) => {
            assert_eq!(e.message, "invalid float \"1e999\"");
            assert_eq!((e.line, e.col), (1, 5));
        }
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn rejects_negative_overflow() {
    match parse_err("a = -1e999") {
        HoconError::Parse(e) => assert_eq!(e.message, "invalid float \"-1e999\""),
        other => panic!("expected Parse error, got {other:?}"),
    }
}

#[test]
fn rejects_fractional_overflow() {
    assert!(matches!(parse_err("a = 2.5e999"), HoconError::Parse(_)));
}

#[test]
fn rejects_overflow_in_array_element() {
    assert!(matches!(parse_err("a = [1, 1e999]"), HoconError::Parse(_)));
}

#[test]
fn accepts_underflow_as_zero() {
    let cfg = hocon::parse("a = 1e-400").expect("underflow parses");
    assert_eq!(cfg.get_f64("a").expect("get_f64"), 0.0);
}

#[test]
fn accepts_finite_extremes() {
    let cfg = hocon::parse("a = 1e308\nb = 5e-324").expect("finite extremes parse");
    assert_eq!(cfg.get_f64("a").expect("get_f64"), 1e308);
    assert_eq!(cfg.get_f64("b").expect("get_f64"), 5e-324);
}

#[test]
fn quoted_overflow_stays_string() {
    let cfg = hocon::parse("a = \"1e999\"").expect("quoted literal parses");
    assert_eq!(cfg.get_string("a").expect("get_string"), "1e999");
}

#[test]
fn unquoted_infinity_stays_string() {
    // No Infinity literal exists in HOCON; the word is an unquoted string.
    let cfg = hocon::parse("a = Infinity").expect("unquoted word parses");
    assert_eq!(cfg.get_string("a").expect("get_string"), "Infinity");
}

// E19 hardening: the parser now rejects overflowing literals, but a
// non-finite number scalar can still be constructed programmatically.
// The serde path must refuse it loudly instead of letting serde_json
// silently degrade it to `null`.
#[cfg(feature = "serde")]
#[test]
fn serde_refuses_programmatically_constructed_non_finite_number() {
    use hocon::{from_value, HoconValue, ScalarValue};
    let v = HoconValue::Scalar(ScalarValue::number("1e999".to_string()));
    let res: Result<serde_json::Value, _> = from_value(&v);
    let err = res.expect_err("non-finite must not silently degrade to null");
    assert!(err.to_string().contains("non-finite"), "got: {err}");
}
