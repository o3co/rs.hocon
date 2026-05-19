//! S21.4 — byte-single-letter conformance tests against xx.hocon fixtures.
//!
//! Fixtures: tests/testdata/hocon/byte-single-letter/bsl01-bsl09.conf
//! Each fixture has a raw HOCON value (e.g. `b = "1K"`) plus a `-expected.json`
//! sidecar with the parsed string value.
//!
//! This test file additionally asserts `get_bytes()` output, which is the per-impl
//! accessor-time assertion (byte count is not captured in the sidecar — the sidecar
//! stores the raw parse output as a string value).
//!
//! RED: bsl01-bsl09 assertions fail until `parse_bytes` is updated to use
//! powers-of-two for single-letter units.

use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/byte-single-letter")
}

fn load(stem: &str) -> hocon::Config {
    let path = fixture_dir().join(format!("{}.conf", stem));
    hocon::parse_file(&path).unwrap_or_else(|e| panic!("failed to load {}: {}", stem, e))
}

/// bsl01: `b = "1K"` → get_bytes = 1024.
#[test]
fn bsl01_bare_k_uppercase_is_1024() {
    let cfg = load("bsl01-1K");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1024,
        "bsl01: '1K' must yield 1024 bytes (K=2^10, HOCON.md L1385)"
    );
}

/// bsl02: `b = "1k"` → get_bytes = 1024.
#[test]
fn bsl02_bare_k_lowercase_is_1024() {
    let cfg = load("bsl02-1k");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1024,
        "bsl02: '1k' must yield 1024 bytes (k=2^10)"
    );
}

/// bsl03: `b = "1M"` → get_bytes = 1048576.
#[test]
fn bsl03_bare_m_is_1048576() {
    let cfg = load("bsl03-1M");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_048_576,
        "bsl03: '1M' must yield 1_048_576 bytes (M=2^20)"
    );
}

/// bsl04: `b = "1G"` → get_bytes = 1073741824.
#[test]
fn bsl04_bare_g_is_1073741824() {
    let cfg = load("bsl04-1G");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_073_741_824,
        "bsl04: '1G' must yield 1_073_741_824 bytes (G=2^30)"
    );
}

/// bsl05: `b = "1T"` → get_bytes = 1099511627776.
#[test]
fn bsl05_bare_t_is_1099511627776() {
    let cfg = load("bsl05-1T");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_099_511_627_776,
        "bsl05: '1T' must yield 1_099_511_627_776 bytes (T=2^40)"
    );
}

/// bsl06: `b = "1P"` → get_bytes = 1125899906842624.
#[test]
fn bsl06_bare_p_is_1125899906842624() {
    let cfg = load("bsl06-1P");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_125_899_906_842_624,
        "bsl06: '1P' must yield 1_125_899_906_842_624 bytes (P=2^50)"
    );
}

/// bsl07: `b = "1E"` → get_bytes = 1152921504606846976.
#[test]
fn bsl07_bare_e_is_1152921504606846976() {
    let cfg = load("bsl07-1E");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_152_921_504_606_846_976,
        "bsl07: '1E' must yield 1_152_921_504_606_846_976 bytes (E=2^60)"
    );
}

/// bsl08: `b = "1024K"` → get_bytes = 1048576 (BREAKING: K=1024 not 1000).
#[test]
fn bsl08_multiplier_1024k_is_1048576() {
    let cfg = load("bsl08-1024K");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        1_048_576,
        "bsl08: '1024K' must yield 1_048_576 bytes (K=1024 binary, Lightbend ground truth)"
    );
}

/// bsl09: `b = "0.5K"` → get_bytes = 512 (fractional × binary K).
#[test]
fn bsl09_fractional_k_is_512() {
    let cfg = load("bsl09-05K");
    assert_eq!(
        cfg.get_bytes("b").unwrap(),
        512,
        "bsl09: '0.5K' must yield 512 bytes (0.5 × 1024)"
    );
}
