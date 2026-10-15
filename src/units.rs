//! HOCON unit values: duration (S19), period (S20) and byte size (S21).
//!
//! This module owns the unit grammar and the rules a unit value must meet: the
//! default unit of a bare number (ms / days / bytes), non-negative byte sizes,
//! integer-only periods. The `read_*` functions take a scalar's source text
//! and type and are the only place these rules are applied; a caller decides
//! only where the scalar comes from and how a [`UnitError`] is worded.

use crate::lexer::is_hocon_whitespace;
use crate::value::ScalarType;
use std::time::Duration;

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
/// its type: a number scalar is also read as a bare number of milliseconds,
/// including the forms the unit grammar does not cover (`1e3`).
pub(crate) fn read_duration(raw: &str, ty: ScalarType) -> Result<Duration, UnitError> {
    if let Some(d) = parse_duration(raw) {
        return Ok(d);
    }
    if ty == ScalarType::Number {
        if let Ok(n) = raw.parse::<i64>() {
            if n < 0 {
                return Err(UnitError::NegativeDuration);
            }
            return Ok(Duration::from_millis(n as u64));
        }
        if let Ok(f) = raw.parse::<f64>() {
            if f < 0.0 || !f.is_finite() {
                return Err(UnitError::InvalidDuration);
            }
            let secs = f / 1000.0;
            // `Duration::from_secs_f64` panics at 2^64 seconds and above;
            // `u64::MAX as f64` rounds up to exactly 2^64, so the bound is `>=`.
            if secs >= u64::MAX as f64 {
                return Err(UnitError::DurationTooLarge);
            }
            return Ok(Duration::from_secs_f64(secs));
        }
    }
    Err(UnitError::InvalidDuration)
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
/// Mirrors Lightbend `SimpleConfig.isWholeNumber`. Used to choose the integer fast-path
/// vs fractional fallback in `parse_duration` and `parse_bytes`.
fn is_integer_str(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Parse a HOCON duration string into a [`std::time::Duration`].
///
/// Accepts `[ws] number [ws] [unit] [ws]` where `unit` is one of the HOCON duration
/// units (HOCON.md L1304-1313). When no unit is present the default is **milliseconds**
/// (HOCON.md L1301: "bare numbers are taken to be in milliseconds already").
///
/// # Fractional values (Lightbend-faithful per-family)
///
/// - Integer form `[+-]?[0-9]+`: parsed as `i64` milliseconds → scaled to nanos.
/// - Fractional form: parsed as `f64`, multiplied by `nanos_per_unit`.
///
/// # Negative values (rs-specific limitation)
///
/// `std::time::Duration` is unsigned; this function returns `None` for negative inputs.
/// Callers that need signed duration semantics should inspect the raw string first.
/// `get_duration` documents this constraint; ud06 conformance test asserts `is_err()`.
fn parse_duration(s: &str) -> Option<Duration> {
    let s = trim_hocon_ws(s);
    if s.is_empty() {
        return None;
    }

    // Scan to end of the numeric prefix (digits, optional leading sign, optional decimal).
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    // Trim HOCON whitespace between number and unit. The unit match below is
    // case-sensitive per HOCON.md L1304 (S19.8): only lowercase unit names are valid.
    let unit_str = trim_hocon_ws(&s[num_end..]);

    if num_str.is_empty() {
        return None;
    }

    let nanos_per_unit: f64 = match unit_str {
        // Default: milliseconds (HOCON.md L1301).
        "" | "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => 1_000_000.0,
        "ns" | "nano" | "nanos" | "nanosecond" | "nanoseconds" => 1.0,
        "us" | "micro" | "micros" | "microsecond" | "microseconds" => 1_000.0,
        "s" | "second" | "seconds" => 1_000_000_000.0,
        "m" | "minute" | "minutes" => 60_000_000_000.0,
        "h" | "hour" | "hours" => 3_600_000_000_000.0,
        "d" | "day" | "days" => 86_400_000_000_000.0,
        // NOTE: no week arm — HOCON's duration list ends at days, and the
        // Lightbend reference rejects "1w" (probe 2026-08-18). Period (S20)
        // is different: Lightbend's parsePeriod DOES accept weeks, and the
        // period parser below keeps them.
        _ => return None,
    };

    // Integer fast-path (matches Lightbend `Long.parseLong`).
    if is_integer_str(num_str) {
        // Parse as i128 so we can range-check both negatives (rejected per rs's
        // unsigned-Duration limitation) AND values in [i64::MAX, u64::MAX] before
        // narrowing to u64. The prior i64 parse rejected the entire upper half of
        // the representable nanos range as "parse error" rather than overflow.
        let n_i128: i128 = num_str.parse().ok()?;
        if n_i128 < 0 {
            return None;
        }
        let n_u64: u64 = n_i128.try_into().ok()?;
        // Overflow guard via checked_mul on u64. The table values are exact small
        // integers (max = 604_800_000_000_000 for weeks), so the f64 → u64 cast
        // is lossless. The prior `(n as f64 * nanos_per_unit) as u64` lost precision
        // for large n (~2^53+) and silently saturated on overflow.
        let unit_u64 = nanos_per_unit as u64;
        let nanos = n_u64.checked_mul(unit_u64)?;
        return Some(Duration::from_nanos(nanos));
    }

    // Fractional fallback (matches Lightbend `Double.parseDouble`).
    let f: f64 = num_str.parse().ok()?;
    if f < 0.0 || !f.is_finite() {
        return None;
    }
    // Precision-safe upper bound: u64::MAX = 2^64 - 1 cannot be represented exactly
    // in f64 (rounds up to 2^64). Compare against `2f64.powi(64)` — the exact float64
    // value of 2^64 — so the boundary check fires for any value that would saturate
    // the subsequent `as u64` cast. Same approach as the cluster #3h fractional byte
    // overflow fix (rs `parse_bytes` 2^63, go `math.Exp2(63)`).
    let product = f * nanos_per_unit;
    if !product.is_finite() || product >= 2f64.powi(64) {
        return None;
    }
    Some(Duration::from_nanos(product as u64))
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

/// Parse a HOCON byte-size string into a byte count.
///
/// Accepts `[ws] number [ws] [unit] [ws]`. When no unit is present the default is
/// **bytes** (HOCON.md L1341: "bare numbers are taken to be in bytes").
///
/// Fractional values are accepted and **truncated** (not rounded) per Lightbend's
/// `BigDecimal.toBigInteger()` semantics (e.g. `"1024.5"` → 1024 bytes).
///
/// Note: `get_bytes` rejects negative results at the accessor level (ub04 / Lightbend
/// `getBytesBigInteger` positive-only invariant). `parse_bytes` itself allows negative
/// integer inputs.
fn parse_bytes(s: &str) -> Option<i64> {
    let s = trim_hocon_ws(s);
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .unwrap_or(s.len());
    let num_str = s[..num_end].trim();
    let unit_str = trim_hocon_ws(&s[num_end..]);

    if num_str.is_empty() {
        return None;
    }

    // S21.1–S21.4 — the EXACT Lightbend unit set (typesafe-config 1.4.6
    // probe, 2026-08-18). Multi-letter units are case-sensitive: the
    // kilo-decimal spelling is `kB` (KB/kb are errors), binary prefixes are
    // capital-first (`Ki`, never `ki`), long forms are lowercase only. Only
    // the bare byte unit (B/b) and the single-letter -Xmx forms accept both
    // cases.
    //
    // Single-letter abbreviations map to **powers of two** per HOCON.md
    // L1385 (java -Xmx convention), BREAKING since 1.3.0 for K/M/G/T — see
    // CHANGELOG.md.
    let multiplier: i64 = match unit_str {
        "" | "B" | "b" | "byte" | "bytes" => 1,
        // Single-letter → powers of two (HOCON.md L1385). Z/z/Y/y are in the
        // big-unit arm below (their multipliers exceed i64).
        "K" | "k" => 1_024,
        "M" | "m" => 1_048_576,
        "G" | "g" => 1_073_741_824,
        "T" | "t" => 1_099_511_627_776,
        "P" | "p" => 1_125_899_906_842_624,
        "E" | "e" => 1_152_921_504_606_846_976,
        // Multi-letter forms: SI decimal + IEC binary, through E/Ei.
        "kB" | "kilobyte" | "kilobytes" => 1_000,
        "Ki" | "KiB" | "kibibyte" | "kibibytes" => 1_024,
        "MB" | "megabyte" | "megabytes" => 1_000_000,
        "Mi" | "MiB" | "mebibyte" | "mebibytes" => 1_048_576,
        "GB" | "gigabyte" | "gigabytes" => 1_000_000_000,
        "Gi" | "GiB" | "gibibyte" | "gibibytes" => 1_073_741_824,
        "TB" | "terabyte" | "terabytes" => 1_000_000_000_000,
        "Ti" | "TiB" | "tebibyte" | "tebibytes" => 1_099_511_627_776,
        "PB" | "petabyte" | "petabytes" => 1_000_000_000_000_000,
        "Pi" | "PiB" | "pebibyte" | "pebibytes" => 1_125_899_906_842_624,
        "EB" | "exabyte" | "exabytes" => 1_000_000_000_000_000_000,
        "Ei" | "EiB" | "exbibyte" | "exbibytes" => 1_152_921_504_606_846_976,
        _ => {
            // Units whose multiplier exceeds i64 (ZB=10^21, YB=10^24,
            // Zi=2^70, Yi=2^80). Lightbend recognises them and range-errors
            // on any result past the long ceiling, so an integer count ≥ 1
            // can never survive — every valid use is fractional and goes
            // through the float path with its overflow guard.
            let big: f64 = match unit_str {
                "ZB" | "zettabyte" | "zettabytes" => 1e21,
                "Zi" | "ZiB" | "zebibyte" | "zebibytes" | "Z" | "z" => 2f64.powi(70),
                "YB" | "yottabyte" | "yottabytes" => 1e24,
                "Yi" | "YiB" | "yobibyte" | "yobibytes" | "Y" | "y" => 2f64.powi(80),
                _ => return None,
            };
            let f: f64 = num_str.parse().ok()?;
            if !f.is_finite() || f.abs() * big >= 2f64.powi(63) {
                return None;
            }
            return Some((f * big) as i64);
        }
    };

    // Integer fast-path: lossless, avoids any floating-point rounding.
    if is_integer_str(num_str) {
        let n: i64 = num_str.parse().ok()?;
        return n.checked_mul(multiplier);
    }

    // Fractional fallback: truncate toward zero per Lightbend `BigDecimal.toBigInteger()`.
    let f: f64 = num_str.parse().ok()?;
    // Overflow guard BEFORE the cast: `i64::MAX as f64` rounds up to exactly 2^63 in IEEE-754,
    // so `> i64::MAX as f64` misses the boundary (8.0E == 2^63 passes the `>` test but saturates).
    // Use `>= 2f64.powi(63)` to catch the exact boundary and all values above it.
    if !f.is_finite() || f.abs() * multiplier as f64 >= 2f64.powi(63) {
        return None;
    }
    Some((f * multiplier as f64) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ──────────────────────────────────────────────────────────────
    // Unit B — parse_duration bare/fractional/ws tests (RED phase)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_duration_bare_integer_uses_ms_default() {
        assert_eq!(parse_duration("500"), Some(Duration::from_millis(500)));
    }
    #[test]
    fn parse_duration_leading_ws_bare() {
        assert_eq!(parse_duration(" 500"), Some(Duration::from_millis(500)));
    }
    #[test]
    fn parse_duration_trailing_ws_bare() {
        assert_eq!(parse_duration("500 "), Some(Duration::from_millis(500)));
    }
    #[test]
    fn parse_duration_both_ws_bare() {
        assert_eq!(parse_duration(" 500 "), Some(Duration::from_millis(500)));
    }
    #[test]
    fn parse_duration_fractional_bare_uses_nanos() {
        let d = parse_duration("500.5").unwrap();
        assert_eq!(d.as_nanos(), 500_500_000);
    }
    #[test]
    fn parse_duration_empty_is_none() {
        assert!(parse_duration("").is_none());
    }
    #[test]
    fn parse_duration_ws_only_is_none() {
        assert!(parse_duration("   ").is_none());
    }
    #[test]
    fn parse_duration_unit_only_is_none() {
        assert!(parse_duration("ms").is_none());
    }

    // ──────────────────────────────────────────────────────────────
    // Issue #95 — parse_duration overflow guards (integer + fractional)
    //
    // Pre-fix: `(n as f64 * nanos_per_unit) as u64` silently saturated for
    // large n; `(f * nanos_per_unit) as u64` silently saturated for fractional
    // overflow. Lightbend errors in both cases — we now match.
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_duration_weeks_rejected() {
        // No week unit in HOCON durations: the Lightbend reference rejects
        // "1w" (probe 2026-08-18); weeks exist only in the Period format.
        assert!(parse_duration("1w").is_none());
        assert!(parse_duration("1week").is_none());
        assert!(parse_duration("1 weeks").is_none());
    }

    #[test]
    fn parse_duration_integer_overflow_days_is_none() {
        // i64::MAX days × 86_400_000_000_000 ns/day overflows u64 (2^64 ≈ 1.8e19).
        assert!(parse_duration("9223372036854775807 days").is_none());
    }

    #[test]
    fn parse_duration_integer_max_u64_nanos_succeeds() {
        // u64::MAX nanos should be representable (boundary success).
        let d = parse_duration("18446744073709551615ns").unwrap();
        assert_eq!(d.as_nanos(), u64::MAX as u128);
    }

    #[test]
    fn parse_duration_fractional_overflow_is_none() {
        // 1e30 days is wildly past u64::MAX nanos.
        assert!(parse_duration("1e30 d").is_none());
    }

    #[test]
    fn parse_duration_fractional_above_u64_max_is_none() {
        // 2^64 nanos exactly — boundary just past representable range.
        // f64 rounds u64::MAX up to 2^64, so the strict-< check at 2^64 catches both.
        assert!(parse_duration("18446744073709551616ns").is_none());
    }

    #[test]
    fn parse_duration_fractional_succeeds_below_boundary() {
        // 10.5 days = 907_200_000_000_000 ns, well within u64.
        let d = parse_duration("10.5d").unwrap();
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
