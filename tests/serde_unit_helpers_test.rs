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
    // A number scalar's source text reaches the reader, exponent included.
    let d: D = hocon::from_str("v = 1e3").unwrap();
    assert_eq!(d.v, Duration::from_secs(1));
    let b: B = hocon::from_str("v = 1e3").unwrap();
    assert_eq!(b.v, 1000);
    // The digits as written, not the nearest double: 0.99999999999999999 ms
    // is 999999 ns exactly, while the double rounds to 1.0 (1 ms).
    let d: D = hocon::from_str("v = 0.99999999999999999").unwrap();
    assert_eq!(d.v, Duration::from_nanos(999_999));
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
        hocon::from_str::<Items>(r#"items { "0" { v = 1k }, "1" { v = -1 } }"#).unwrap_err(),
    );
    assert_eq!(err.path, "items[1].v");
    assert_eq!(err.message, "negative byte size: \"-1\"");
    #[derive(Debug, Deserialize)]
    struct Durations {
        items: Vec<D>,
    }
    let ds: Durations = hocon::from_str(r#"items { "0" { v = 0.99999999999999999 } }"#).unwrap();
    assert_eq!(ds.items[0].v, Duration::from_nanos(999_999));
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

    // Another format hands over a number's value, read exactly and truncated
    // like the getter's (E20).
    let b: B = serde_json::from_str(r#"{"v": 1000}"#).unwrap();
    assert_eq!(b.v, 1000);
    let b: B = serde_json::from_str(r#"{"v": 1e3}"#).unwrap();
    assert_eq!(b.v, 1000);
    let b: B = serde_json::from_str(r#"{"v": 1.5}"#).unwrap();
    assert_eq!(b.v, 1);
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

#[test]
fn buffered_negative_underflow_reads_as_zero() {
    // Pinned exception: `-1e-400` underflows the double to `-0.0`, which
    // cannot be told from a literal `-0` (zero, accepted by the getter), so a
    // buffered duration reads zero where the getter reports a negative value.
    let src = "v = -1e-400";
    let err = parse(src).unwrap().get_duration("v").unwrap_err();
    assert!(
        err.message.starts_with("negative duration"),
        "{}",
        err.message
    );
    let buffered = hocon::from_str::<Flat<D>>(src).unwrap().inner.v;
    assert_eq!(buffered, Duration::ZERO);
    // The literal `-0` reads as zero both ways.
    assert_eq!(
        parse("v = -0").unwrap().get_duration("v").unwrap(),
        Duration::ZERO
    );
    assert_eq!(
        hocon::from_str::<Flat<D>>("v = -0").unwrap().inner.v,
        Duration::ZERO
    );
    // Bytes truncate first, so the two agree: zero.
    assert_eq!(parse(src).unwrap().get_bytes("v").unwrap(), 0);
    assert_eq!(hocon::from_str::<Flat<B>>(src).unwrap().inner.v, 0);
}

// ── require_unit (ts.hocon#233) ─────────────────────────────────────────
// `hocon::serde::{duration, bytes, period}::require_unit` read the same
// values as their siblings but refuse one that does not name its unit.

#[derive(Debug, Deserialize)]
struct RD {
    #[serde(with = "hocon::serde::duration::require_unit")]
    v: Duration,
}

#[derive(Debug, Deserialize)]
struct RB {
    #[serde(with = "hocon::serde::bytes::require_unit")]
    v: i64,
}

#[derive(Debug, Deserialize)]
struct RP {
    #[serde(with = "hocon::serde::period::require_unit")]
    v: Period,
}

/// The decode error of `v = <literal>`, at the key `v`.
fn require_unit_error<W: serde::de::DeserializeOwned + std::fmt::Debug>(literal: &str) -> String {
    let err = config_error(hocon::from_str::<W>(&format!("v = {}", literal)).unwrap_err());
    assert_eq!(err.path, "v", "literal {}", literal);
    err.message
}

#[test]
fn require_unit_refuses_a_value_without_its_unit() {
    // A number, a string of the number alone (an environment variable), and
    // an exponent form, which is a number too (`1e3` is not "1 e3").
    for lit in ["30", "\"30\"", "1e3", "\"1e3\"", "\" 30 \"", "0", "1.5"] {
        assert_eq!(
            require_unit_error::<RD>(lit),
            "duration must name its unit",
            "literal {}",
            lit
        );
    }
    for lit in ["4096", "\"4096\"", "1e3", "\"1e3\"", "0"] {
        assert_eq!(
            require_unit_error::<RB>(lit),
            "byte size must name its unit",
            "literal {}",
            lit
        );
    }
    for lit in ["14", "\"14\"", "-7", "\"+7\"", "0"] {
        assert_eq!(
            require_unit_error::<RP>(lit),
            "period must name its unit",
            "literal {}",
            lit
        );
    }
    // A period is integer-only: `"1e3"` is not a period at all, and keeps
    // that error.
    assert_eq!(
        require_unit_error::<RP>("\"1e3\""),
        "invalid period: \"1e3\""
    );
    assert_eq!(require_unit_error::<RP>("1e3"), "invalid period: \"1e3\"");
}

#[test]
fn require_unit_message_has_no_value() {
    // A value can be a secret set under the wrong key.
    let secret = "\"8675309\"";
    for message in [
        require_unit_error::<RD>(secret),
        require_unit_error::<RB>(secret),
        require_unit_error::<RP>(secret),
        require_unit_error::<RD>("8675309"),
    ] {
        assert!(!message.contains("8675309"), "{}", message);
    }
}

#[test]
fn require_unit_reads_a_value_with_its_unit() {
    for (lit, want) in [
        ("30s", Duration::from_secs(30)),
        ("\"30s\"", Duration::from_secs(30)),
        ("30 s", Duration::from_secs(30)),
        ("\"30 s\"", Duration::from_secs(30)),
        ("\"1e3ms\"", Duration::from_secs(1)),
        ("1e3ms", Duration::from_secs(1)),
        ("100 ms", Duration::from_millis(100)),
        ("2 hours", Duration::from_secs(7200)),
    ] {
        let d: RD = hocon::from_str(&format!("v = {}", lit)).unwrap();
        assert_eq!(d.v, want, "literal {}", lit);
    }
    for (lit, want) in [
        ("64KiB", 65_536),
        ("\"64 KiB\"", 65_536),
        ("4096B", 4096),
        ("1e3kB", 1_000_000),
        ("0.5G", 1 << 29),
        ("1E", 1 << 60),
    ] {
        let b: RB = hocon::from_str(&format!("v = {}", lit)).unwrap();
        assert_eq!(b.v, want, "literal {}", lit);
    }
    for (lit, want) in [
        ("2w", Period::new(0, 0, 14)),
        ("\"2 w\"", Period::new(0, 0, 14)),
        ("7d", Period::new(0, 0, 7)),
        ("-7d", Period::new(0, 0, -7)),
        ("3 mo", Period::new(0, 3, 0)),
        ("1y", Period::new(1, 0, 0)),
    ] {
        let p: RP = hocon::from_str(&format!("v = {}", lit)).unwrap();
        assert_eq!(p.v, want, "literal {}", lit);
    }
}

#[test]
fn require_unit_keeps_the_readers_errors() {
    // The reader runs first: a value that does not read reports why, with
    // the value, as the sibling helper does.
    assert_eq!(
        require_unit_error::<RD>("5 parsecs"),
        "invalid duration: \"5 parsecs\""
    );
    assert_eq!(require_unit_error::<RD>("-5"), "negative duration: \"-5\"");
    assert_eq!(
        require_unit_error::<RD>("\"-5s\""),
        "invalid duration: \"-5s\""
    );
    assert_eq!(
        require_unit_error::<RD>("18446744073709551616000"),
        "duration too large: \"18446744073709551616000\""
    );
    assert_eq!(
        require_unit_error::<RD>("\"1e400s\""),
        "duration too large: \"1e400s\""
    );
    assert_eq!(require_unit_error::<RB>("-1"), "negative byte size: \"-1\"");
    assert_eq!(
        require_unit_error::<RB>("\"-1k\""),
        "negative byte size: \"-1k\""
    );
    assert_eq!(
        require_unit_error::<RB>("9223372036854775808"),
        "invalid byte size: \"9223372036854775808\""
    );
    assert_eq!(
        require_unit_error::<RB>("10 KB"),
        "invalid byte size: \"10 KB\""
    );
    assert_eq!(require_unit_error::<RP>("7.5"), "invalid period: \"7.5\"");
    assert_eq!(require_unit_error::<RP>("7D"), "invalid period: \"7D\"");
    assert_eq!(
        require_unit_error::<RD>("true"),
        "invalid duration: \"true\""
    );

    // The field type's range is checked by the reader too.
    #[derive(Debug, Deserialize)]
    struct Small {
        #[serde(with = "hocon::serde::bytes::require_unit")]
        v: u8,
    }
    assert_eq!(
        require_unit_error::<Small>("1k"),
        "byte size \"1k\" (1024 bytes) is out of range for u8"
    );
    assert_eq!(
        require_unit_error::<Small>("300"),
        "byte size \"300\" (300 bytes) is out of range for u8"
    );
    assert_eq!(hocon::from_str::<Small>("v = 255B").unwrap().v, 255);

    // Not a scalar: the same type error as the sibling helper.
    assert_eq!(
        require_unit_error::<RD>("{ a = 1 }"),
        "invalid type: map, expected a HOCON duration"
    );
}

#[derive(Debug, Deserialize, PartialEq)]
struct RequiredOptional {
    #[serde(default, with = "hocon::serde::duration::require_unit::option")]
    d: Option<Duration>,
    #[serde(default, with = "hocon::serde::bytes::require_unit::option")]
    b: Option<u64>,
    #[serde(default, with = "hocon::serde::period::require_unit::option")]
    p: Option<Period>,
}

#[test]
fn require_unit_option_helpers() {
    let all: RequiredOptional = hocon::from_str("d = 30s, b = 1k, p = 1y").unwrap();
    assert_eq!(
        all,
        RequiredOptional {
            d: Some(Duration::from_secs(30)),
            b: Some(1024),
            p: Some(Period::new(1, 0, 0)),
        }
    );
    let none = RequiredOptional {
        d: None,
        b: None,
        p: None,
    };
    let nulls: RequiredOptional = hocon::from_str("d = null, b = null, p = null").unwrap();
    assert_eq!(nulls, none);
    let missing: RequiredOptional = hocon::from_str("").unwrap();
    assert_eq!(missing, none);

    for (src, path, message) in [
        ("d = 30", "d", "duration must name its unit"),
        ("d = \"30\"", "d", "duration must name its unit"),
        ("b = 1024", "b", "byte size must name its unit"),
        ("p = 7", "p", "period must name its unit"),
        ("b = -1k", "b", "negative byte size: \"-1k\""),
    ] {
        let err = config_error(hocon::from_str::<RequiredOptional>(src).unwrap_err());
        assert_eq!((err.path.as_str(), err.message.as_str()), (path, message));
    }
}

#[test]
fn require_unit_error_carries_the_key_path() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct Token {
        #[serde(with = "hocon::serde::duration::require_unit")]
        ttl: Duration,
    }
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct App {
        tokens: Vec<Token>,
    }
    let err =
        config_error(hocon::from_str::<App>("tokens = [{ ttl = 30s }, { ttl = 30 }]").unwrap_err());
    assert_eq!(err.path, "tokens[1].ttl");
    assert_eq!(err.message, "duration must name its unit");

    // A substitution of a string, as an environment variable gives one, is
    // a string of the number alone; through `get_as` the path is the full one.
    let cfg = parse("raw = \"30\"\napp.token.ttl = ${raw}").unwrap();
    let err = cfg.get_as::<Token>("app.token").unwrap_err();
    assert_eq!(err.path, "app.token.ttl");
    assert_eq!(err.message, "duration must name its unit");
    let cfg = parse("raw = \"30 s\"\napp.token.ttl = ${raw}").unwrap();
    let t = cfg.get_as::<Token>("app.token").unwrap();
    assert_eq!(t.ttl, Duration::from_secs(30));
}

#[test]
fn require_unit_inside_flatten() {
    // serde buffers the input first: a number arrives as a number and a
    // string as a string, so the check is the same as without flatten.
    for lit in ["30", "\"30\"", "1e3", "\"1e3\"", "1.5"] {
        let err = hocon::from_str::<Flat<RD>>(&format!("v = {}", lit)).unwrap_err();
        let err = config_error(err);
        assert_eq!(
            err.message, "duration must name its unit",
            "literal {}",
            lit
        );
        let err = config_error(hocon::from_str::<Flat<RB>>(&format!("v = {}", lit)).unwrap_err());
        assert_eq!(
            err.message, "byte size must name its unit",
            "literal {}",
            lit
        );
    }
    for lit in ["14", "\"14\""] {
        let err = config_error(hocon::from_str::<Flat<RP>>(&format!("v = {}", lit)).unwrap_err());
        assert_eq!(err.message, "period must name its unit", "literal {}", lit);
    }
    let d = hocon::from_str::<Flat<RD>>("v = 30s").unwrap();
    assert_eq!(d.inner.v, Duration::from_secs(30));
    let d = hocon::from_str::<Flat<RD>>("v = \"1e3ms\"").unwrap();
    assert_eq!(d.inner.v, Duration::from_secs(1));
    let b = hocon::from_str::<Flat<RB>>("v = 64KiB").unwrap();
    assert_eq!(b.inner.v, 65_536);
    let p = hocon::from_str::<Flat<RP>>("v = 2w").unwrap();
    assert_eq!(p.inner.v, Period::new(0, 0, 14));

    // The reader's own error still comes first there.
    let err = config_error(hocon::from_str::<Flat<RD>>("v = -5").unwrap_err());
    assert_eq!(err.message, "negative duration: \"-5\"");

    // Untagged and internally tagged enums buffer the same way.
    assert!(hocon::from_str::<Untagged<RD>>("v = 30").is_err());
    assert!(hocon::from_str::<Untagged<RD>>("v = 30s").is_ok());
    assert!(hocon::from_str::<Tagged<RD>>("t = X, v = \"30\"").is_err());
    assert!(hocon::from_str::<Tagged<RD>>("t = X, v = 30s").is_ok());
}

#[test]
fn require_unit_with_other_deserializers() {
    let err = serde_json::from_str::<RD>(r#"{"v": 30}"#).unwrap_err();
    assert!(
        err.to_string().contains("duration must name its unit"),
        "{}",
        err
    );
    let err = serde_json::from_str::<RB>(r#"{"v": "1024"}"#).unwrap_err();
    assert!(
        err.to_string().contains("byte size must name its unit"),
        "{}",
        err
    );
    let d: RD = serde_json::from_str(r#"{"v": "30s"}"#).unwrap();
    assert_eq!(d.v, Duration::from_secs(30));
}

#[test]
fn sibling_helpers_still_read_a_bare_number_in_the_default_unit() {
    // The default is off: the existing modules are unchanged.
    for lit in ["30", "\"30\""] {
        let src = format!("v = {}", lit);
        assert_eq!(
            hocon::from_str::<D>(&src).unwrap().v,
            Duration::from_millis(30)
        );
        assert_eq!(hocon::from_str::<B>(&src).unwrap().v, 30);
        assert_eq!(hocon::from_str::<P>(&src).unwrap().v, Period::new(0, 0, 30));
        assert_eq!(
            hocon::from_str::<Flat<D>>(&src).unwrap().inner.v,
            Duration::from_millis(30)
        );
    }
    let o: Optional = hocon::from_str("d = 30, b = 30, p = 30").unwrap();
    assert_eq!(
        o,
        Optional {
            d: Some(Duration::from_millis(30)),
            b: Some(30),
            p: Some(Period::new(0, 0, 30)),
        }
    );
    // The getters too.
    let c = parse("v = 30").unwrap();
    assert_eq!(c.get_duration("v").unwrap(), Duration::from_millis(30));
    assert_eq!(c.get_bytes("v").unwrap(), 30);
    assert_eq!(c.get_period("v").unwrap(), Period::new(0, 0, 30));
}
