//! S18.4 — string value with no unit → default unit — conformance tests.
//!
//! Fixtures: xx.hocon testdata/hocon/units-default/ (22 fixtures).
//! No expected sidecars — per-impl assertions carry the assertion burden (the xx.hocon
//! generator does not yet support accessor-time output capture; see spec §Test strategy).
//!
//! Coverage:
//!   ud01-ud08  Duration family (bare int, WS variants, fractional, negative, regression)
//!   up01-up05  Period family  (bare int, WS, fractional-rejected, negative, regression)
//!   ub01-ub06  Bytes family   (bare int, WS, fractional-truncated, negative-accessor, unit, empty)
//!   un01-un03  Cross-family negative edge cases (empty, WS-only, unit-only)

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/units-default")
}

fn load(name: &str) -> hocon::Config {
    let path = fixture_dir().join(name);
    hocon::parse_file(&path).unwrap_or_else(|e| panic!("failed to load {}: {}", name, e))
}

// ─────────────────────────────────────────────────────────────────────────────
// Duration family (ud01–ud08)
// ─────────────────────────────────────────────────────────────────────────────

/// ud01: bare integer string → 500 ms (default unit).
#[test]
fn ud01_duration_bare() {
    let cfg = load("ud01-duration-bare.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "ud01: bare '500' must default to 500 ms"
    );
}

/// ud02: leading whitespace before number → still 500 ms.
#[test]
fn ud02_duration_leading_ws() {
    let cfg = load("ud02-duration-leading-ws.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "ud02: leading-WS '\" 500\"' must default to 500 ms"
    );
}

/// ud03: trailing whitespace after number → still 500 ms.
#[test]
fn ud03_duration_trailing_ws() {
    let cfg = load("ud03-duration-trailing-ws.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "ud03: trailing-WS '\"500 \"' must default to 500 ms"
    );
}

/// ud04: leading + trailing whitespace → still 500 ms.
#[test]
fn ud04_duration_both_ws() {
    let cfg = load("ud04-duration-both-ws.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "ud04: both-WS '\" 500 \"' must default to 500 ms"
    );
}

/// ud05: fractional bare string → 500_500_000 nanos (Lightbend Double×nanos_per_unit).
///
/// Lightbend-faithful: duration accepts fractional; `"500.5"` with ms default =
/// 500.5 × 1_000_000 ns = 500_500_000 ns.
#[test]
fn ud05_duration_fractional() {
    let cfg = load("ud05-duration-fractional.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap().as_nanos(),
        500_500_000,
        "ud05: fractional '500.5' must yield 500_500_000 nanos (Lightbend Double path)"
    );
}

/// ud06: negative bare string → Err (rs-specific limitation).
///
/// rs-specific: `std::time::Duration` is unsigned. Lightbend's `java.time.Duration` is
/// signed. For `"-500"` rs.hocon returns `Err` rather than −500 ms. This is a documented
/// rs-specific divergence; see CHANGELOG.
#[test]
fn ud06_duration_negative() {
    let cfg = load("ud06-duration-negative.conf");
    assert!(
        cfg.get_duration("t").is_err(),
        // rs-specific: std::time::Duration cannot represent negative durations.
        // Lightbend java.time.Duration CAN represent negatives. See CHANGELOG.
        "ud06 (rs-specific): get_duration(\"-500\") must Err (std::time::Duration is unsigned)"
    );
}

/// ud07: string with explicit unit `"500ms"` → 500 ms (regression guard for existing path).
#[test]
fn ud07_duration_with_unit() {
    let cfg = load("ud07-duration-with-unit.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "ud07 (regression): explicit unit '500ms' must still parse correctly"
    );
}

/// ud08: WS between number and unit `"500 ms"` → 500 ms (regression guard).
#[test]
fn ud08_duration_ws_between() {
    let cfg = load("ud08-duration-ws-between.conf");
    assert_eq!(
        cfg.get_duration("t").unwrap(),
        std::time::Duration::from_millis(500),
        "ud08 (regression): WS between number and unit must parse correctly"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Period family (up01–up05)
// ─────────────────────────────────────────────────────────────────────────────

/// up01: bare integer string → 7 days (default unit = days, HOCON.md L1321).
#[test]
fn up01_period_bare() {
    let cfg = load("up01-period-bare.conf");
    assert_eq!(
        cfg.get_period("p").unwrap(),
        (0, 0, 7),
        "up01: bare '7' must default to 7 days"
    );
}

/// up02: leading + trailing whitespace → still 7 days.
#[test]
fn up02_period_leading_trailing_ws() {
    let cfg = load("up02-period-leading-trailing-ws.conf");
    assert_eq!(
        cfg.get_period("p").unwrap(),
        (0, 0, 7),
        "up02: whitespace-padded '\" 7 \"' must default to 7 days"
    );
}

/// up03: fractional string `"7.5"` → Err (period is integer-only, Lightbend Integer.parseInt).
#[test]
fn up03_period_fractional_rejected() {
    let cfg = load("up03-period-fractional-rejected.conf");
    assert!(
        cfg.get_period("p").is_err(),
        "up03: fractional '7.5' must Err (period is integer-only per Lightbend Integer.parseInt)"
    );
}

/// up04: negative `"-7"` → (0, 0, -7) — period allows negative at accessor (Lightbend).
#[test]
fn up04_period_negative() {
    let cfg = load("up04-period-negative.conf");
    assert_eq!(
        cfg.get_period("p").unwrap(),
        (0, 0, -7),
        "up04: negative '-7' must yield (0, 0, -7) days (signed i32 tuple)"
    );
}

/// up05: `"7w"` → 49 days (regression guard: explicit weeks unit).
#[test]
fn up05_period_with_unit() {
    let cfg = load("up05-period-with-unit.conf");
    assert_eq!(
        cfg.get_period("p").unwrap(),
        (0, 0, 49),
        "up05 (regression): '7w' must yield (0, 0, 49) days"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Bytes family (ub01–ub06)
// ─────────────────────────────────────────────────────────────────────────────

/// ub01: bare integer string `"1024"` → 1024 bytes (default unit = bytes).
#[test]
fn ub01_bytes_bare() {
    let cfg = load("ub01-bytes-bare.conf");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1024,
        "ub01: bare '1024' must default to 1024 bytes"
    );
}

/// ub02: whitespace-padded `" 1024 "` → 1024 bytes.
#[test]
fn ub02_bytes_leading_trailing_ws() {
    let cfg = load("ub02-bytes-leading-trailing-ws.conf");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1024,
        "ub02: whitespace-padded '\" 1024 \"' must yield 1024 bytes"
    );
}

/// ub03: fractional `"1024.5"` → 1024 bytes (truncated, Lightbend BigDecimal.toBigInteger).
#[test]
fn ub03_bytes_fractional_truncated() {
    let cfg = load("ub03-bytes-fractional-truncated.conf");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1024,
        "ub03: fractional '1024.5' must truncate to 1024 bytes (not round to 1025)"
    );
}

/// ub04: negative `"-1"` → Err at accessor (Lightbend positive-only invariant on byte sizes).
#[test]
fn ub04_bytes_negative_accessor_rejects() {
    let cfg = load("ub04-bytes-negative-accessor-rejects.conf");
    assert!(
        cfg.get_bytes("b").is_err(),
        "ub04: negative byte size '-1' must Err at accessor (positive-only invariant)"
    );
}

/// ub05: `"1024K"` → 1_024_000 bytes (regression guard: SI unit K = 1000).
#[test]
fn ub05_bytes_with_unit() {
    let cfg = load("ub05-bytes-with-unit.conf");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_024_000,
        "ub05 (regression): '1024K' must yield 1_024_000 bytes (K = 1000 SI)"
    );
}

/// ub06: empty string `""` → Err (no number to parse, HOCON.md L1284).
#[test]
fn ub06_bytes_empty_rejected() {
    let cfg = load("ub06-bytes-empty-rejected.conf");
    assert!(
        cfg.get_bytes("b").is_err(),
        "ub06: empty string must Err (no number present)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-family negative edge cases (un01–un03)
// ─────────────────────────────────────────────────────────────────────────────

/// un01: empty string `""` → getDuration Err (no number, HOCON.md L1284).
#[test]
fn un01_empty_duration() {
    let cfg = load("un01-empty-duration.conf");
    assert!(
        cfg.get_duration("t").is_err(),
        "un01: empty string must Err for getDuration"
    );
}

/// un02: whitespace-only string `"   "` → getDuration Err.
#[test]
fn un02_ws_only_duration() {
    let cfg = load("un02-ws-only-duration.conf");
    assert!(
        cfg.get_duration("t").is_err(),
        "un02: whitespace-only string must Err for getDuration"
    );
}

/// un03: unit-only string `"ms"` → getDuration Err (number is required per L1284).
#[test]
fn un03_unit_only_duration() {
    let cfg = load("un03-unit-only-duration.conf");
    assert!(
        cfg.get_duration("t").is_err(),
        "un03: unit-only 'ms' must Err for getDuration (number required)"
    );
}
