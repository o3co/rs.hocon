//! S21.4 — Single-letter byte abbreviations must map to powers of two (HOCON.md L1385).
//!
//! BREAKING CHANGE: K/M/G/T/P/E are now binary (1024^n) not SI decimal (1000^n).
//!
//! RED tests: these must FAIL until `parse_bytes` in `src/config.rs` is updated.

fn parse_bytes_str(s: &str) -> Result<i64, hocon::ConfigError> {
    let cfg = hocon::parse(&format!(r#"b = "{}""#, s))
        .expect("parse must succeed for byte string fixture");
    cfg.get_bytes("b")
}

/// s21_4_1: `1K` → 1024 (binary, not 1000 SI).
#[test]
fn s21_4_1_k_uppercase_is_1024() {
    assert_eq!(
        parse_bytes_str("1K").unwrap(),
        1024,
        "S21.4: '1K' must be 1024 (powers-of-two, HOCON.md L1385)"
    );
}

/// s21_4_2: `1k` → 1024 (lowercase k, same as uppercase per spec).
#[test]
fn s21_4_2_k_lowercase_is_1024() {
    assert_eq!(
        parse_bytes_str("1k").unwrap(),
        1024,
        "S21.4: '1k' must be 1024 (powers-of-two, HOCON.md L1385)"
    );
}

/// s21_4_3: `1M` → 1048576 (2^20).
#[test]
fn s21_4_3_m_is_2_pow_20() {
    assert_eq!(
        parse_bytes_str("1M").unwrap(),
        1_048_576,
        "S21.4: '1M' must be 1_048_576 (2^20, HOCON.md L1385)"
    );
}

/// s21_4_4: `1G` → 1073741824 (2^30).
#[test]
fn s21_4_4_g_is_2_pow_30() {
    assert_eq!(
        parse_bytes_str("1G").unwrap(),
        1_073_741_824,
        "S21.4: '1G' must be 1_073_741_824 (2^30, HOCON.md L1385)"
    );
}

/// s21_4_5: `1T` → 1099511627776 (2^40).
#[test]
fn s21_4_5_t_is_2_pow_40() {
    assert_eq!(
        parse_bytes_str("1T").unwrap(),
        1_099_511_627_776,
        "S21.4: '1T' must be 1_099_511_627_776 (2^40, HOCON.md L1385)"
    );
}

/// s21_4_6: `1P` → 1125899906842624 (2^50).
#[test]
fn s21_4_6_p_is_2_pow_50() {
    assert_eq!(
        parse_bytes_str("1P").unwrap(),
        1_125_899_906_842_624,
        "S21.4: '1P' must be 1_125_899_906_842_624 (2^50, HOCON.md L1385)"
    );
}

/// s21_4_7: `1E` → 1152921504606846976 (2^60).
#[test]
fn s21_4_7_e_is_2_pow_60() {
    assert_eq!(
        parse_bytes_str("1E").unwrap(),
        1_152_921_504_606_846_976,
        "S21.4: '1E' must be 1_152_921_504_606_846_976 (2^60, HOCON.md L1385)"
    );
}

/// s21_4_8: `1024K` → 1048576 (1024 × 1024 = 2^20). This is the BREAKING load-bearing test.
///
/// Previously: 1024 × 1000 = 1_024_000 (SI decimal K).
/// Now: 1024 × 1024 = 1_048_576 (binary K). Matches Lightbend typesafe-config ground truth.
#[test]
fn s21_4_8_1024k_is_1048576() {
    assert_eq!(
        parse_bytes_str("1024K").unwrap(),
        1_048_576,
        "S21.4 BREAKING: '1024K' must be 1_048_576 (K=1024 binary). Previously was 1_024_000 (K=1000 SI)."
    );
}

/// s21_4_9: `0.5K` → 512 (fractional × binary K).
#[test]
fn s21_4_9_fractional_k_is_512() {
    assert_eq!(
        parse_bytes_str("0.5K").unwrap(),
        512,
        "S21.4: '0.5K' must be 512 (0.5 × 1024, fractional binary K)"
    );
}

/// s21_4_10: `9E` must error (9 × 2^60 = 9 × 1_152_921_504_606_846_976 overflows i64).
///
/// i64::MAX = 9_223_372_036_854_775_807 ≈ 8.07E (2^63-1).
/// 9E = 9 × 2^60 = 10_376_293_541_461_622_784 > i64::MAX → overflow error.
#[test]
fn s21_4_10_9e_overflows_i64() {
    assert!(
        parse_bytes_str("9E").is_err(),
        "S21.4: '9E' must error — 9 × 2^60 overflows i64 (checked_mul overflow guard)"
    );
}

/// s21_4_f1: `8.0E` must error — 8.0 × 2^60 = 2^63 equals i64::MAX as f64 (float64 boundary).
///
/// `i64::MAX as f64` rounds up to exactly 2^63 in IEEE-754, so a naive `> i64::MAX as f64`
/// check lets 8.0E through and the `as i64` cast saturates to i64::MAX silently.
/// The fix uses `>= 2f64.powi(63)` to catch the exact boundary (rs-I1 / convergent with go I1+T1).
#[test]
fn s21_4_f1_8e_boundary_overflows() {
    assert!(
        parse_bytes_str("8.0E").is_err(),
        "S21.4: '8.0E' must error — 8.0 × 2^60 == 2^63 == i64::MAX+1, not a valid i64 (rs-I1)"
    );
}

/// s21_4_f2: `9.0E` must error (9 × 2^60 > i64::MAX, fractional overflow path).
///
/// Tests the fractional path explicitly (integer path uses checked_mul which already worked).
#[test]
fn s21_4_f2_9e_fractional_overflows() {
    assert!(
        parse_bytes_str("9.0E").is_err(),
        "S21.4: '9.0E' must error — 9.0 × 2^60 overflows i64 (fractional path)"
    );
}

/// s21_4_f3: `8.5E` must error (8.5 × 2^60 > 2^63, clearly above boundary).
#[test]
fn s21_4_f3_8_5e_overflows() {
    assert!(
        parse_bytes_str("8.5E").is_err(),
        "S21.4: '8.5E' must error — 8.5 × 2^60 > 2^63"
    );
}

/// s21_4_f4: `7.0E` must succeed — 7 × 2^60 = 8_070_450_532_247_928_832 < i64::MAX.
#[test]
fn s21_4_f4_7e_succeeds() {
    let result = parse_bytes_str("7.0E");
    assert!(
        result.is_ok(),
        "S21.4: '7.0E' must succeed — 7 × 2^60 = 8_070_450_532_247_928_832 < i64::MAX"
    );
    // f64 truncation: 7.0 * 2^60 is exact in f64 (7 * 2^60 < 2^63 and 7 has only 3 bits)
    assert_eq!(
        result.unwrap(),
        7 * 1_152_921_504_606_846_976_i64,
        "S21.4: '7.0E' value must be 7 × 2^60"
    );
}

/// s21_4_11: multi-letter `KB` remains SI decimal 1000 (separate match arm, unchanged).
#[test]
fn s21_4_11_kb_stays_si_decimal() {
    assert_eq!(
        parse_bytes_str("1KB").unwrap(),
        1_000,
        "S21.4 (regression): 'KB' multi-letter must remain SI decimal 1000"
    );
}

/// s21_4_12: `KiB` remains binary 1024 (unchanged).
#[test]
fn s21_4_12_kib_stays_binary() {
    assert_eq!(
        parse_bytes_str("1KiB").unwrap(),
        1_024,
        "S21.4 (regression): 'KiB' must remain 1024"
    );
}
