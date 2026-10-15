#![cfg(feature = "serde")]
//! `hocon::serde::{duration, bytes, period}` read unit values with the
//! getters' rules (xx.hocon#102). The parity tables run each literal through
//! the getter and the helper and require the same outcome.

use hocon::{parse, HoconError, Period};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct D {
    #[serde(with = "hocon::serde::duration")]
    v: Duration,
}

#[derive(Debug, Deserialize)]
struct B {
    #[serde(with = "hocon::serde::bytes")]
    v: i64,
}

#[derive(Debug, Deserialize)]
struct P {
    #[serde(with = "hocon::serde::period")]
    v: Period,
}

fn config_error(e: HoconError) -> hocon::ConfigError {
    match e {
        HoconError::Config(e) => e,
        other => panic!("expected HoconError::Config, got {:?}", other),
    }
}

/// Run `literal` through the getter and the helper; both must succeed with
/// the same value, or both fail.
fn assert_parity<T, W>(
    literal: &str,
    getter: impl Fn(&hocon::Config) -> Result<T, hocon::ConfigError>,
    field: impl Fn(W) -> T,
) where
    T: PartialEq + std::fmt::Debug,
    W: serde::de::DeserializeOwned,
{
    let src = format!("v = {}", literal);
    let via_getter = getter(&parse(&src).unwrap());
    let via_serde = hocon::from_str::<W>(&src).map(field);
    match (&via_getter, &via_serde) {
        (Ok(g), Ok(s)) => assert_eq!(g, s, "literal {}", literal),
        (Err(_), Err(_)) => {}
        _ => panic!(
            "literal {}: getter {:?} but helper {:?}",
            literal, via_getter, via_serde
        ),
    }
}

#[test]
fn duration_parity_with_get_duration() {
    for lit in [
        "30m",
        "\"30m\"",
        "100 ms",
        "2 hours",
        "500",
        "\"500\"",
        "1.5",
        "1.5s",
        "1e3",
        "\"1e3\"",
        "0.0000001",
        "-5",
        "\"-5s\"",
        "100 MS",
        "1w",
        "18446744073709551615ns",
        "18446744073709551616000",
        "null",
        "true",
        "\"\"",
    ] {
        assert_parity(lit, |c| c.get_duration("v"), |w: D| w.v);
    }
}

#[test]
fn bytes_parity_with_get_bytes() {
    for lit in [
        "64KiB", "512 MB", "10 kB", "10 KB", "1.5G", "1024", "\"1024\"", "1.5", "\"1.5\"", "1e3",
        "\"1e3\"", "-1", "\"-1k\"", "8E", "null",
    ] {
        assert_parity(lit, |c| c.get_bytes("v"), |w: B| w.v);
    }
}

#[test]
fn period_parity_with_get_period() {
    for lit in [
        "7d", "2w", "3m", "3 mo", "1y", "7", "\"7\"", "-7", "7.5", "1e1", "7D", "null",
    ] {
        assert_parity(lit, |c| c.get_period("v"), |w: P| w.v);
    }
}

#[test]
fn exponent_form_follows_the_getter() {
    // A number scalar's source text reaches the reader: `1e3` is 1000 ms as a
    // duration, and not a byte count (a bare byte count is an integer).
    let d: D = hocon::from_str("v = 1e3").unwrap();
    assert_eq!(d.v, Duration::from_secs(1));
    let err = config_error(hocon::from_str::<B>("v = 1e3").unwrap_err());
    assert_eq!(err.message, "expected byte size: \"1e3\"");
    assert_eq!(err.path, "v");
}

#[test]
fn error_carries_the_key_path() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct Session {
        #[serde(with = "hocon::serde::duration")]
        lifetime: Duration,
    }
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct App {
        session: Session,
    }
    let err = config_error(hocon::from_str::<App>("session { lifetime = 5 parsecs }").unwrap_err());
    assert_eq!(err.path, "session.lifetime");
    assert_eq!(err.message, "invalid duration: \"5 parsecs\"");

    let cfg = parse("app.session.lifetime = -5").unwrap();
    let err = cfg.get_as::<Session>("app.session").unwrap_err();
    assert_eq!(err.path, "app.session.lifetime");
    assert_eq!(err.message, "negative duration: \"-5\"");
}

#[test]
fn bytes_range_is_the_field_types() {
    #[derive(Debug, Deserialize)]
    struct Small {
        #[serde(with = "hocon::serde::bytes")]
        v: u8,
    }
    let ok: Small = hocon::from_str("v = 255B").unwrap();
    assert_eq!(ok.v, 255);
    let err = config_error(hocon::from_str::<Small>("v = 1k").unwrap_err());
    assert_eq!(err.path, "v");
    assert_eq!(
        err.message,
        "byte size \"1k\" (1024 bytes) is out of range for u8"
    );

    #[derive(Debug, Deserialize)]
    struct Unsigned {
        #[serde(with = "hocon::serde::bytes")]
        v: u64,
    }
    let u: Unsigned = hocon::from_str("v = 64KiB").unwrap();
    assert_eq!(u.v, 65_536);
}

#[derive(Debug, Deserialize, PartialEq)]
struct Optional {
    #[serde(default, with = "hocon::serde::duration::option")]
    d: Option<Duration>,
    #[serde(default, with = "hocon::serde::bytes::option")]
    b: Option<u64>,
    #[serde(default, with = "hocon::serde::period::option")]
    p: Option<Period>,
}

#[test]
fn option_helpers() {
    let all: Optional = hocon::from_str("d = 30s, b = 1k, p = 1y").unwrap();
    assert_eq!(
        all,
        Optional {
            d: Some(Duration::from_secs(30)),
            b: Some(1024),
            p: Some(Period::new(1, 0, 0)),
        }
    );
    let nulls: Optional = hocon::from_str("d = null, b = null, p = null").unwrap();
    assert_eq!(
        nulls,
        Optional {
            d: None,
            b: None,
            p: None
        }
    );
    let missing: Optional = hocon::from_str("").unwrap();
    assert_eq!(missing, nulls);

    let err = config_error(hocon::from_str::<Optional>("b = -1").unwrap_err());
    assert_eq!(err.path, "b");
    assert_eq!(err.message, "negative byte size: \"-1\"");
}

#[test]
fn non_scalar_is_a_type_error() {
    let err = config_error(hocon::from_str::<D>("v { a = 1 }").unwrap_err());
    assert_eq!(err.path, "v");
    assert_eq!(err.message, "invalid type: map, expected a HOCON duration");
    let err = config_error(hocon::from_str::<P>("v = [1]").unwrap_err());
    assert_eq!(
        err.message,
        "invalid type: sequence, expected a HOCON period"
    );
    // An array shaped like the private raw-scalar answer is still an array.
    let err = config_error(hocon::from_str::<B>(r#"v = ["number", "5"]"#).unwrap_err());
    assert_eq!(
        err.message,
        "invalid type: sequence, expected a HOCON byte size"
    );
    let err = serde_json::from_str::<B>(r#"{"v": ["number", "5"]}"#).unwrap_err();
    assert!(
        err.to_string().contains("invalid type: sequence"),
        "{}",
        err
    );
}

#[test]
fn array_elements_including_numeric_keyed_objects() {
    #[derive(Debug, Deserialize)]
    struct Items {
        items: Vec<B>,
    }
    let ok: Items = hocon::from_str("items = [{ v = 1k }, { v = 2 }]").unwrap();
    assert_eq!(ok.items.iter().map(|b| b.v).collect::<Vec<_>>(), [1024, 2]);

    // S15 numeric-keyed object → array: elements go through the owned
    // deserializer, which must hand over the source text too.
    let err = config_error(
        hocon::from_str::<Items>(r#"items { "0" { v = 1k }, "1" { v = 1e3 } }"#).unwrap_err(),
    );
    assert_eq!(err.path, "items[1].v");
    assert_eq!(err.message, "expected byte size: \"1e3\"");
}

#[test]
fn other_deserializers_read_the_value() {
    #[derive(Debug, Deserialize)]
    struct All {
        #[serde(with = "hocon::serde::duration")]
        d: Duration,
        #[serde(with = "hocon::serde::bytes")]
        b: u64,
        #[serde(with = "hocon::serde::period")]
        p: Period,
    }
    let all: All = serde_json::from_str(r#"{"d": "30m", "b": 1024, "p": "2w"}"#).unwrap();
    assert_eq!(all.d, Duration::from_secs(1800));
    assert_eq!(all.b, 1024);
    assert_eq!(all.p, Period::new(0, 0, 14));

    let err = serde_json::from_str::<B>(r#"{"v": "5 parsecs"}"#).unwrap_err();
    assert!(err.to_string().contains("invalid byte size: \"5 parsecs\""));

    // Another format hands over a number's value: an integer reads as an
    // integer, a float as a float — so bytes rejects `1e3` / `1.0` here just
    // as the getter rejects the HOCON literal `1e3`.
    let b: B = serde_json::from_str(r#"{"v": 1000}"#).unwrap();
    assert_eq!(b.v, 1000);
    let err = serde_json::from_str::<B>(r#"{"v": 1e3}"#).unwrap_err();
    assert!(
        err.to_string().contains("expected byte size: \"1000.0\""),
        "{}",
        err
    );
    assert!(serde_json::from_str::<B>(r#"{"v": 1.0}"#).is_err());
    let d: D = serde_json::from_str(r#"{"v": 1e3}"#).unwrap();
    assert_eq!(d.v, Duration::from_secs(1));

    let none: Optional = serde_json::from_str(r#"{"d": null, "b": null}"#).unwrap();
    assert_eq!(none.d, None);
    assert_eq!(none.b, None);
}

/// The parity tables again, with serde buffering the input first
/// (`#[serde(flatten)]` and an untagged enum): the helper then gets the value
/// instead of the source text, and must still agree with the getter.
#[derive(Debug, Deserialize)]
struct Flat<W> {
    #[serde(flatten)]
    inner: W,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Untagged<W> {
    Only(W),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
enum Tagged<W> {
    X(W),
}

fn assert_buffered_parity<T, W>(
    literal: &str,
    getter: impl Fn(&hocon::Config) -> Result<T, hocon::ConfigError>,
    field: impl Fn(W) -> T + Copy,
) where
    T: PartialEq + std::fmt::Debug,
    W: serde::de::DeserializeOwned + std::fmt::Debug,
{
    assert_parity(literal, &getter, move |w: Flat<W>| field(w.inner));
    assert_parity(literal, &getter, move |w: Untagged<W>| match w {
        Untagged::Only(w) => field(w),
    });
    // Internally tagged: the tag sits beside `v`.
    let src = format!("t = X, v = {}", literal);
    let via_getter = getter(&parse(&src).unwrap());
    let via_serde = hocon::from_str::<Tagged<W>>(&src).map(|w| match w {
        Tagged::X(w) => field(w),
    });
    match (&via_getter, &via_serde) {
        (Ok(g), Ok(s)) => assert_eq!(g, s, "tagged literal {}", literal),
        (Err(_), Err(_)) => {}
        _ => panic!(
            "tagged literal {}: getter {:?} but helper {:?}",
            literal, via_getter, via_serde
        ),
    }
}

const NUMBER_LITERALS: &[&str] = &[
    "0",
    "-0",
    "7",
    "-7",
    "1024",
    "1.0",
    "7.0",
    "1.5",
    "-1.5",
    "1e3",
    "1E3",
    "1e1",
    "1.5e3",
    "0.0000001",
    "9223372036854775808",
    "12345678901234567890",
    "18446744073709551616000",
    // Spelled differently from their buffered form (exponent vs decimal);
    // E20 makes a number read by value, so they read alike.
    "2.05e0",
    "0.157e-1",
    "0.00000051",
    "0.0000999",
];

/// Number literals whose nearest double differs from the digits written
/// (more than 15 significant digits, not an i64 integer). A buffered helper
/// sees the double, so a duration (whose arithmetic is exact, E20) reads the
/// double's value. Bytes and period reject these either way.
const BEYOND_DOUBLE_PRECISION: &[&str] = &["9223372036854775808", "12345678901234567890"];

#[test]
fn buffered_parity_with_the_getters() {
    for lit in NUMBER_LITERALS
        .iter()
        .chain(&["30m", "\"64KiB\"", "2w", "null", "true"])
    {
        if !BEYOND_DOUBLE_PRECISION.contains(lit) {
            assert_buffered_parity(lit, |c| c.get_duration("v"), |w: D| w.v);
        }
        assert_buffered_parity(lit, |c| c.get_bytes("v"), |w: B| w.v);
        assert_buffered_parity(lit, |c| c.get_period("v"), |w: P| w.v);
    }
}

#[test]
fn number_literal_parity_with_the_getters() {
    for lit in NUMBER_LITERALS {
        assert_parity(lit, |c| c.get_duration("v"), |w: D| w.v);
        assert_parity(lit, |c| c.get_bytes("v"), |w: B| w.v);
        assert_parity(lit, |c| c.get_period("v"), |w: P| w.v);
    }
}

#[test]
fn unresolved_substitution_is_reported() {
    use hocon::{parse_string_with_options, ParseOptions};
    let c = parse_string_with_options(
        "v = ${missing}",
        ParseOptions::defaults().with_resolve_substitutions(false),
    )
    .unwrap();
    let err = c.deserialize::<D>().unwrap_err();
    assert_eq!(err.path, "v");
    assert!(
        err.message.contains("unresolved substitution"),
        "{}",
        err.message
    );
}

#[test]
fn buffered_number_beyond_double_precision_reads_the_double() {
    // Pinned: the only buffered difference left after E20. The getter reads
    // the 19–20 digits written; the buffered helper reads the nearest double.
    for lit in BEYOND_DOUBLE_PRECISION {
        let src = format!("v = {}", lit);
        let getter = parse(&src).unwrap().get_duration("v").unwrap();
        let buffered = hocon::from_str::<Flat<D>>(&src).unwrap().inner.v;
        let nearest_double: f64 = lit.parse().unwrap();
        let expected = parse(&format!("v = {:?}", nearest_double))
            .unwrap()
            .get_duration("v")
            .unwrap();
        assert_eq!(buffered, expected, "literal {}", lit);
        assert_ne!(getter, buffered, "literal {}", lit);
    }
}
