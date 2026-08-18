// S19 / S21.2–S21.4 — unit-table alignment with the Lightbend reference
// (typesafe-config 1.4.6 probes, 2026-08-18), part of the four-impl units
// audit the py.hocon verification wave triggered. rs-specific deltas: the
// duration table carried an extra-spec `w`/`week`/`weeks` unit (Lightbend
// rejects `1w`; weeks exist only in the Period format), the byte table was
// keyed `KB` (the reference's kilo-decimal spelling is `kB`) and stopped at
// TiB.

fn bytes_of(lit: &str) -> Option<i64> {
    let cfg = hocon::parse(&format!("v = \"{lit}\"")).expect("parse should succeed");
    cfg.get_bytes("v").ok()
}

fn duration_ok(lit: &str) -> bool {
    let cfg = hocon::parse(&format!("v = \"{lit}\"")).expect("parse should succeed");
    cfg.get_duration("v").is_ok()
}

#[test]
fn s19_no_week_unit_in_durations() {
    for lit in ["1w", "1week", "1weeks", "1 w"] {
        assert!(!duration_ok(lit), "{lit}: durations have no week unit");
    }
    // Period keeps weeks (Lightbend's parsePeriod accepts them).
    let cfg = hocon::parse("v = \"2w\"").unwrap();
    assert_eq!(cfg.get_period("v").unwrap().days, 14);
}

#[test]
fn s21_2_decimal_units_through_yb() {
    for (lit, want) in [
        ("1kB", 1_000i64),
        ("1PB", 1_000_000_000_000_000),
        ("1petabytes", 1_000_000_000_000_000),
        ("1EB", 1_000_000_000_000_000_000),
        ("0.000001ZB", 1_000_000_000_000_000),
        ("0.000000001YB", 1_000_000_000_000_000),
        ("0.000001zettabytes", 1_000_000_000_000_000),
    ] {
        assert_eq!(bytes_of(lit), Some(want), "{lit}");
    }
    // ZB/YB counts >= 1 exceed i64 — Lightbend range-errors, and so do we.
    assert_eq!(bytes_of("1ZB"), None);
    assert_eq!(bytes_of("1YB"), None);
}

#[test]
fn s21_3_binary_units_through_yi() {
    for (lit, want) in [
        ("1Pi", 1i64 << 50),
        ("1PiB", 1i64 << 50),
        ("1pebibytes", 1i64 << 50),
        ("1Ei", 1i64 << 60),
        ("0.000001Zi", (1e-6 * 2f64.powi(70)) as i64),
        ("0.000000001Yi", (1e-9 * 2f64.powi(80)) as i64),
    ] {
        assert_eq!(bytes_of(lit), Some(want), "{lit}");
    }
}

#[test]
fn s21_4_single_letters_through_y() {
    for lit in ["1Z", "1z", "1Y", "1y"] {
        assert_eq!(bytes_of(lit), None, "{lit}: count 1 overflows i64");
    }
    let want = (1e-6 * 2f64.powi(70)) as i64;
    assert_eq!(bytes_of("0.000001Z"), Some(want));
    assert_eq!(bytes_of("0.000001z"), Some(want));
}

#[test]
fn s21_lightbend_case_sensitivity() {
    // Lightbend's unit table is case-sensitive: kB parses, these do not.
    for lit in [
        "1KB",
        "1kb",
        "1Kb",
        "1mB",
        "1Kilobyte",
        "1MEGABYTES",
        "1kiB",
        "1ki",
        "1Byte",
    ] {
        assert_eq!(bytes_of(lit), None, "{lit}: must be rejected");
    }
    // The two-case exceptions: the bare byte unit and the single letters.
    assert_eq!(bytes_of("1B"), Some(1));
    assert_eq!(bytes_of("1b"), Some(1));
    assert_eq!(bytes_of("1K"), Some(1024));
    assert_eq!(bytes_of("1k"), Some(1024));
}
