//! HOCON unit values: duration (S19), period (S20) and byte size (S21).
//!
//! This module owns the unit grammar and the rules a unit value must meet: the
//! default unit of a bare number (ms / days / bytes), non-negative byte sizes,
//! integer-only periods. The `read_*` functions take a scalar's source text
//! and type and are the only place these rules are applied; a caller decides
//! only where the scalar comes from and how a [`UnitError`] is worded.

use crate::lexer::is_hocon_whitespace;
use crate::value::ScalarType;
use decimal::Decimal;
use std::time::Duration;

mod decimal;

/// A calendar period with year, month, and day components.
///
/// Returned by [`Config::get_period`](crate::Config::get_period) and
/// [`Config::get_period_option`](crate::Config::get_period_option).
/// All fields are `i32` to support negative periods (matching Lightbend behaviour).
///
/// The struct is `#[non_exhaustive]` so that new fields (e.g. weeks, hours) can
/// be added in a future minor version without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Period {
    pub years: i32,
    pub months: i32,
    pub days: i32,
}

impl Period {
    /// Construct a `Period` from year, month, and day components.
    pub fn new(years: i32, months: i32, days: i32) -> Self {
        Self {
            years,
            months,
            days,
        }
    }
}

/// Why a scalar could not be read as a unit value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitError {
    InvalidDuration,
    NegativeDuration,
    DurationTooLarge,
    /// A number scalar that is not an integer (`1.5`, `1e3`): a bare byte
    /// count must be a whole number of bytes.
    ExpectedBytes,
    InvalidBytes,
    NegativeBytes,
    InvalidPeriod,
}

/// The problem in words (`invalid duration`); each caller adds the path and
/// the raw text in its own wording.
impl std::fmt::Display for UnitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnitError::InvalidDuration => "invalid duration",
            UnitError::NegativeDuration => "negative duration",
            UnitError::DurationTooLarge => "duration too large",
            UnitError::ExpectedBytes => "expected byte size",
            UnitError::InvalidBytes => "invalid byte size",
            UnitError::NegativeBytes => "negative byte size",
            UnitError::InvalidPeriod => "invalid period",
        })
    }
}

/// Read a scalar as a [`Duration`]. `raw` is the scalar's source text and `ty`
/// its type: a string goes through the duration grammar (`30m`, `100 ms`), a
/// number is milliseconds and may use an exponent (`1e3`). Either way the
/// arithmetic is exact (E20).
pub(crate) fn read_duration(raw: &str, ty: ScalarType) -> Result<Duration, UnitError> {
    if ty == ScalarType::Number {
        // A number value is milliseconds, read from the literal as written,
        // exponent included (S18.1, E20): `1e3` and `1000` are the same value.
        // A number scalar built in code may carry text the parser never
        // produces (`30s`); that still goes through the grammar below.
        if let Some(value) = Decimal::parse(trim_hocon_ws(raw), true) {
            if value.is_negative() {
                return Err(UnitError::NegativeDuration);
            }
            return duration_from(&value, 1_000_000, MAX_DURATION_NANOS);
        }
    }
    parse_duration(raw)
}

/// Read a scalar as a byte count. A number scalar must be a whole number of
/// bytes; a string goes through the byte-size grammar (`512 MB`, `1.5 KiB`).
/// The result is never negative (ub04: Lightbend `getBytesBigInteger` rejects
/// negative sizes on both paths).
pub(crate) fn read_bytes(raw: &str, ty: ScalarType) -> Result<i64, UnitError> {
    let n = if ty == ScalarType::Number {
        raw.parse::<i64>().map_err(|_| UnitError::ExpectedBytes)?
    } else {
        parse_bytes(raw).ok_or(UnitError::InvalidBytes)?
    };
    if n < 0 {
        return Err(UnitError::NegativeBytes);
    }
    Ok(n)
}

/// Read a scalar as a [`Period`]. Integer-only; a bare number is days.
/// Negative periods are allowed (Lightbend behaviour).
pub(crate) fn read_period(raw: &str, ty: ScalarType) -> Result<Period, UnitError> {
    if let Some((y, mo, d)) = parse_period(raw) {
        return Ok(Period::new(y, mo, d));
    }
    if ty == ScalarType::Number {
        if let Ok(n) = raw.parse::<i32>() {
            return Ok(Period::new(0, 0, n));
        }
    }
    Err(UnitError::InvalidPeriod)
}

/// Trim HOCON whitespace (per `is_hocon_whitespace`) from both ends of `s`.
///
/// Unlike `str::trim()` (which is ASCII-only for whitespace), this respects the full
/// HOCON_WS set (U+00A0 NBSP, U+FEFF BOM, various Unicode space separators, etc.).
fn trim_hocon_ws(s: &str) -> &str {
    s.trim_matches(is_hocon_whitespace)
}

/// Returns `true` if `s` matches `[+-]?[0-9]+` (integer pre-classification).
///
/// Mirrors Lightbend `SimpleConfig.isWholeNumber`. Used by `parse_period`, which is
/// integer-only.
fn is_integer_str(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// The longest duration `std::time::Duration` holds, in nanoseconds.
const MAX_DURATION_NANOS: u128 = u64::MAX as u128 * 1_000_000_000 + 999_999_999;

/// Split `[ws] number [ws] [unit] [ws]` into the number and the unit. The
/// number is the leading run of digits, signs and points; the rest, without
/// surrounding HOCON whitespace, is the unit (possibly empty).
fn split_number_unit(s: &str) -> (&str, &str) {
    let s = trim_hocon_ws(s);
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .unwrap_or(s.len());
    (&s[..num_end], trim_hocon_ws(&s[num_end..]))
}

/// Nanoseconds per duration unit (HOCON.md L1304-1313). Unit names are
/// case-sensitive and lowercase (S19.8). No unit means milliseconds
/// (HOCON.md L1301).
fn duration_unit_nanos(unit: &str) -> Option<u128> {
    Some(match unit {
        "" | "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => 1_000_000,
        "ns" | "nano" | "nanos" | "nanosecond" | "nanoseconds" => 1,
        "us" | "micro" | "micros" | "microsecond" | "microseconds" => 1_000,
        "s" | "second" | "seconds" => 1_000_000_000,
        "m" | "minute" | "minutes" => 60_000_000_000,
        "h" | "hour" | "hours" => 3_600_000_000_000,
        "d" | "day" | "days" => 86_400_000_000_000,
        // NOTE: no week arm — HOCON's duration list ends at days, and the
        // Lightbend reference rejects "1w" (probe 2026-08-18). Period (S20)
        // is different: Lightbend's parsePeriod DOES accept weeks, and the
        // period parser below keeps them.
        _ => return None,
    })
}

/// `value × nanos_per_unit` as a [`Duration`]: the exact product, truncated
/// toward zero at the nanosecond (E20), or an error past `max_nanos`.
/// `value` must not be negative.
fn duration_from(
    value: &Decimal,
    nanos_per_unit: u128,
    max_nanos: u128,
) -> Result<Duration, UnitError> {
    let nanos = value
        .mul_trunc(nanos_per_unit)
        .filter(|&n| n <= max_nanos)
        .ok_or(UnitError::DurationTooLarge)?;
    Ok(Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    ))
}

/// Parse a HOCON duration string into a [`Duration`].
///
/// Accepts `[ws] number [ws] [unit] [ws]`; the number is a decimal without an
/// exponent, read exactly (E20: `1.001s` is 1001 ms, not the binary-float
/// 1000.999999 ms). `std::time::Duration` is unsigned, so a negative value is
/// an error (rs-specific; the ud06 conformance test asserts it).
fn parse_duration(s: &str) -> Result<Duration, UnitError> {
    let (num, unit) = split_number_unit(s);
    let nanos_per_unit = duration_unit_nanos(unit).ok_or(UnitError::InvalidDuration)?;
    let value = Decimal::parse(num, false).ok_or(UnitError::InvalidDuration)?;
    if value.is_negative() {
        return Err(UnitError::InvalidDuration);
    }
    // A string stops at u64 nanoseconds.
    duration_from(&value, nanos_per_unit, u64::MAX as u128).map_err(|_| UnitError::InvalidDuration)
}

/// Parse a HOCON period string into a `(years, months, days)` tuple.
///
/// Accepts `[ws] integer [ws] [unit] [ws]` where `unit` is one of the HOCON period
/// units (HOCON.md L1324-1333). When no unit is present the default is **days**
/// (HOCON.md L1321: "bare numbers are taken to be in days").
///
/// Period is integer-only (matches Lightbend `Integer.parseInt`). Fractional strings
/// like `"7.5"` return `None`.
///
/// Returns `(years, months, days)` as `i32` to support negative periods (Lightbend allows
/// negative). A `chrono` dependency is intentionally avoided; callers that need a typed
/// `Period` can decompose the tuple.
fn parse_period(s: &str) -> Option<(i32, i32, i32)> {
    let s = trim_hocon_ws(s);
    if s.is_empty() {
        return None;
    }

    // Scan numeric prefix.
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '-' && c != '+')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    let unit_str = trim_hocon_ws(&s[num_end..]);

    if num_str.is_empty() {
        return None;
    }

    // Period is integer-only (Lightbend `Integer.parseInt`). Reject fractional.
    if !is_integer_str(num_str) {
        return None;
    }

    let n: i32 = num_str.parse().ok()?;

    // Unit match — case-sensitive per HOCON.md L1304 (period shares the same
    // "unit names are case-sensitive" rule as duration; only lowercase accepted).
    match unit_str {
        // Default: days (HOCON.md L1321).
        "" | "d" | "day" | "days" => Some((0, 0, n)),
        "w" | "week" | "weeks" => Some((0, 0, n.checked_mul(7)?)),
        "m" | "mo" | "month" | "months" => Some((0, n, 0)),
        "y" | "year" | "years" => Some((n, 0, 0)),
        _ => None,
    }
}

/// Bytes per byte-size unit. S21.1–S21.4 — the EXACT Lightbend unit set
/// (typesafe-config 1.4.6 probe, 2026-08-18). Multi-letter units are
/// case-sensitive: the kilo-decimal spelling is `kB` (KB/kb are errors),
/// binary prefixes are capital-first (`Ki`, never `ki`), long forms are
/// lowercase only. Only the bare byte unit (B/b) and the single-letter -Xmx
/// forms accept both cases. Single-letter abbreviations map to **powers of
/// two** per HOCON.md L1385 (java -Xmx convention), BREAKING since 1.3.0 for
/// K/M/G/T — see CHANGELOG.md. No unit means bytes (HOCON.md L1341).
fn bytes_unit_multiplier(unit: &str) -> Option<u128> {
    Some(match unit {
        "" | "B" | "b" | "byte" | "bytes" => 1,
        "K" | "k" => 1 << 10,
        "M" | "m" => 1 << 20,
        "G" | "g" => 1 << 30,
        "T" | "t" => 1 << 40,
        "P" | "p" => 1 << 50,
        "E" | "e" => 1 << 60,
        "Z" | "z" => 1 << 70,
        "Y" | "y" => 1 << 80,
        "kB" | "kilobyte" | "kilobytes" => 1_000,
        "Ki" | "KiB" | "kibibyte" | "kibibytes" => 1 << 10,
        "MB" | "megabyte" | "megabytes" => 1_000_000,
        "Mi" | "MiB" | "mebibyte" | "mebibytes" => 1 << 20,
        "GB" | "gigabyte" | "gigabytes" => 1_000_000_000,
        "Gi" | "GiB" | "gibibyte" | "gibibytes" => 1 << 30,
        "TB" | "terabyte" | "terabytes" => 1_000_000_000_000,
        "Ti" | "TiB" | "tebibyte" | "tebibytes" => 1 << 40,
        "PB" | "petabyte" | "petabytes" => 1_000_000_000_000_000,
        "Pi" | "PiB" | "pebibyte" | "pebibytes" => 1 << 50,
        "EB" | "exabyte" | "exabytes" => 1_000_000_000_000_000_000,
        "Ei" | "EiB" | "exbibyte" | "exbibytes" => 1 << 60,
        // Past i64 on their own: only a fractional count can fit (`0.001ZB`).
        "ZB" | "zettabyte" | "zettabytes" => 1_000_000_000_000_000_000_000,
        "Zi" | "ZiB" | "zebibyte" | "zebibytes" => 1 << 70,
        "YB" | "yottabyte" | "yottabytes" => 1_000_000_000_000_000_000_000_000,
        "Yi" | "YiB" | "yobibyte" | "yobibytes" => 1 << 80,
        _ => return None,
    })
}

/// Parse a HOCON byte-size string into a byte count.
///
/// Accepts `[ws] number [ws] [unit] [ws]`; the number is a decimal without an
/// exponent. The product is exact and then truncated toward zero, as
/// Lightbend's `BigDecimal.toBigInteger()` does (E20: `1.001kB` is 1001,
/// `1.0009kB` is 1000). `None` past the `i64` range.
///
/// Note: `read_bytes` rejects negative results (ub04 / Lightbend
/// `getBytesBigInteger` positive-only invariant); this function keeps the
/// sign so it can.
fn parse_bytes(s: &str) -> Option<i64> {
    let (num, unit) = split_number_unit(s);
    let multiplier = bytes_unit_multiplier(unit)?;
    let value = Decimal::parse(num, false)?;
    let magnitude = i128::try_from(value.mul_trunc(multiplier)?).ok()?;
    i64::try_from(if value.is_negative() {
        -magnitude
    } else {
        magnitude
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ──────────────────────────────────────────────────────────────
    // Unit B — parse_duration bare/fractional/ws tests (RED phase)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_duration_bare_integer_uses_ms_default() {
        assert_eq!(parse_duration("500").ok(), Some(Duration::from_millis(500)));
    }
    #[test]
    fn parse_duration_leading_ws_bare() {
        assert_eq!(
            parse_duration(" 500").ok(),
            Some(Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_trailing_ws_bare() {
        assert_eq!(
            parse_duration("500 ").ok(),
            Some(Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_both_ws_bare() {
        assert_eq!(
            parse_duration(" 500 ").ok(),
            Some(Duration::from_millis(500))
        );
    }
    #[test]
    fn parse_duration_fractional_bare_uses_nanos() {
        let d = parse_duration("500.5").ok().unwrap();
        assert_eq!(d.as_nanos(), 500_500_000);
    }
    #[test]
    fn parse_duration_empty_is_none() {
        assert!(parse_duration("").ok().is_none());
    }
    #[test]
    fn parse_duration_ws_only_is_none() {
        assert!(parse_duration("   ").ok().is_none());
    }
    #[test]
    fn parse_duration_unit_only_is_none() {
        assert!(parse_duration("ms").ok().is_none());
    }

    // ──────────────────────────────────────────────────────────────
    // Overflow (issue #95, then E21): a duration past the range is an
    // error, never a saturated or wrapped value.
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_duration_weeks_rejected() {
        // No week unit in HOCON durations: the Lightbend reference rejects
        // "1w" (probe 2026-08-18); weeks exist only in the Period format.
        assert!(parse_duration("1w").ok().is_none());
        assert!(parse_duration("1week").ok().is_none());
        assert!(parse_duration("1 weeks").ok().is_none());
    }

    #[test]
    fn parse_duration_integer_overflow_days_is_none() {
        // i64::MAX days × 86_400_000_000_000 ns/day overflows u64 (2^64 ≈ 1.8e19).
        assert!(parse_duration("9223372036854775807 days").ok().is_none());
    }

    #[test]
    fn parse_duration_integer_max_u64_nanos_succeeds() {
        // u64::MAX nanos is well inside the range.
        let d = parse_duration("18446744073709551615ns").ok().unwrap();
        assert_eq!(d.as_nanos(), u64::MAX as u128);
    }

    #[test]
    fn parse_duration_overflow_is_an_error() {
        // 10^21 days is far past the range, integer or fractional.
        assert!(parse_duration("1000000000000000000000 d").is_err());
        assert!(parse_duration("1000000000000000000000.5 d").is_err());
    }

    #[test]
    fn parse_duration_fractional_above_u64_max_is_none() {
        // 2^64 nanos exactly — just past the string range.
        assert!(parse_duration("18446744073709551616ns").ok().is_none());
    }

    #[test]
    fn exact_arithmetic_e20() {
        assert_eq!(parse_duration("1.001s"), Ok(Duration::from_millis(1001)));
        assert_eq!(parse_duration("2.3h"), Ok(Duration::from_millis(8_280_000)));
        assert_eq!(parse_duration("1.001"), Ok(Duration::from_nanos(1_001_000)));
        assert_eq!(parse_duration("1.5ns"), Ok(Duration::from_nanos(1)));
        assert_eq!(parse_bytes("1.001kB"), Some(1001));
        assert_eq!(parse_bytes("1.0009kB"), Some(1000));
        assert_eq!(parse_bytes("0.001ZB"), Some(1_000_000_000_000_000_000));
        // 2^79 is past i64.
        assert_eq!(parse_bytes("0.5Y"), None);
        // Big units take the exact path too: 2^80 × 10^-6, truncated.
        assert_eq!(parse_bytes("0.000001Yi"), Some(1_208_925_819_614_629_174));
    }

    #[test]
    fn number_scalar_built_in_code_keeps_the_grammar() {
        // `ScalarValue` is public: a number scalar with unit text still reads.
        assert_eq!(
            read_duration("30s", ScalarType::Number),
            Ok(Duration::from_secs(30))
        );
        assert_eq!(
            read_duration("5 ms", ScalarType::Number),
            Ok(Duration::from_millis(5))
        );
    }

    #[test]
    fn bytes_keep_i64_min_as_negative() {
        assert_eq!(parse_bytes("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(parse_bytes("-8E"), Some(i64::MIN));
        assert_eq!(
            read_bytes("-8E", ScalarType::String),
            Err(UnitError::NegativeBytes)
        );
    }

    #[test]
    fn number_value_reads_by_value_not_spelling() {
        // E20 for numbers (S18.1): a value reads the same however it is spelled,
        // so a buffered (re-spelled) number reads as the getter does.
        for (a, b) in [
            ("2.05e0", "2.05"),
            ("0.157e-1", "0.0157"),
            ("5.1e-7", "0.00000051"),
            ("9.99e-5", "0.0000999"),
            ("1e3", "1000"),
        ] {
            assert_eq!(
                read_duration(a, ScalarType::Number),
                read_duration(b, ScalarType::Number),
                "{} vs {}",
                a,
                b
            );
        }
    }

    #[test]
    fn parse_duration_fractional_succeeds_below_boundary() {
        // 10.5 days = 907_200_000_000_000 ns, well within u64.
        let d = parse_duration("10.5d").ok().unwrap();
        assert_eq!(d.as_nanos(), 907_200_000_000_000u128);
    }

    // ──────────────────────────────────────────────────────────────
    // Unit C — parse_bytes ws / truncate
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_bytes_leading_trailing_ws_bare() {
        assert_eq!(parse_bytes(" 1024 "), Some(1024));
    }
    #[test]
    fn parse_bytes_fractional_truncated() {
        assert_eq!(parse_bytes("1024.5"), Some(1024));
    }
    // ──────────────────────────────────────────────────────────────
    // Unit D — parse_period (RED phase)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_period_bare_integer_uses_days_default() {
        assert_eq!(parse_period("7"), Some((0, 0, 7)));
    }
    #[test]
    fn parse_period_leading_trailing_ws() {
        assert_eq!(parse_period(" 7 "), Some((0, 0, 7)));
    }
    #[test]
    fn parse_period_fractional_rejected() {
        assert!(parse_period("7.5").is_none());
    }
    #[test]
    fn parse_period_negative_allowed() {
        assert_eq!(parse_period("-7"), Some((0, 0, -7)));
    }
    #[test]
    fn parse_period_weeks_unit() {
        assert_eq!(parse_period("7w"), Some((0, 0, 49)));
    }
    #[test]
    fn parse_period_months_unit() {
        assert_eq!(parse_period("3m"), Some((0, 3, 0)));
    }
    #[test]
    fn parse_period_years_unit() {
        assert_eq!(parse_period("2y"), Some((2, 0, 0)));
    }
    #[test]
    fn parse_period_days_explicit() {
        assert_eq!(parse_period("5d"), Some((0, 0, 5)));
    }
    #[test]
    fn parse_period_empty_is_none() {
        assert!(parse_period("").is_none());
    }

    // ──────────────────────────────────────────────────────────────
    // Readers — the rules shared by the getters and the serde helpers
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn read_duration_number_exponent_is_millis() {
        // `1e3` is outside the unit grammar; a number scalar still reads as ms.
        assert_eq!(
            read_duration("1e3", ScalarType::Number),
            Ok(Duration::from_secs(1))
        );
        assert_eq!(
            read_duration("1e3", ScalarType::String),
            Err(UnitError::InvalidDuration)
        );
    }

    #[test]
    fn read_duration_negative_number() {
        assert_eq!(
            read_duration("-5", ScalarType::Number),
            Err(UnitError::NegativeDuration)
        );
        assert_eq!(
            read_duration("-5s", ScalarType::String),
            Err(UnitError::InvalidDuration)
        );
    }

    #[test]
    fn read_bytes_number_must_be_integer() {
        assert_eq!(read_bytes("1024", ScalarType::Number), Ok(1024));
        assert_eq!(
            read_bytes("1.5", ScalarType::Number),
            Err(UnitError::ExpectedBytes)
        );
        assert_eq!(
            read_bytes("1e3", ScalarType::Number),
            Err(UnitError::ExpectedBytes)
        );
        // The same text as a string goes through the grammar (truncates).
        assert_eq!(read_bytes("1.5", ScalarType::String), Ok(1));
    }

    #[test]
    fn read_bytes_negative_on_both_paths() {
        assert_eq!(
            read_bytes("-1", ScalarType::Number),
            Err(UnitError::NegativeBytes)
        );
        assert_eq!(
            read_bytes("-1k", ScalarType::String),
            Err(UnitError::NegativeBytes)
        );
    }

    #[test]
    fn read_period_integer_only() {
        assert_eq!(
            read_period("2w", ScalarType::String),
            Ok(Period::new(0, 0, 14))
        );
        assert_eq!(
            read_period("1.5", ScalarType::Number),
            Err(UnitError::InvalidPeriod)
        );
    }
}
