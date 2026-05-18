//! S13c — env-var list expansion `${X[]}` / `${?X[]}` conformance tests.
//!
//! Fixtures loaded from `tests/testdata/hocon/env-var-list/` (synced from
//! xx.hocon via `make testdata`). Expected JSON from
//! `tests/testdata/expected/env-var-list/` (also synced via `make testdata`).
//! Both directories are populated by the Makefile's tarball fetch from xx.hocon
//! main and persisted under the crate root so CI (which only checks out
//! rs.hocon) can resolve them without depending on a sibling xx.hocon worktree.
//!
//! Env injection uses `parse_file_with_env` (hermetic — no `std::env::set_var`).

use hocon::HoconValue;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Paths ─────────────────────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon/env-var-list")
}

fn expected_dir() -> PathBuf {
    // Synced by `make testdata` from xx.hocon main into a crate-local path so CI
    // (which only checks out rs.hocon, no sibling xx.hocon worktree) can find it.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/expected/env-var-list")
}

fn fixture_path(name: &str) -> PathBuf {
    fixture_dir().join(format!("{}.conf", name))
}

fn env_path(name: &str) -> PathBuf {
    fixture_dir().join(format!("{}.env", name))
}

fn expected_path(name: &str) -> PathBuf {
    expected_dir().join(format!("{}-expected.json", name))
}

// ── Sidecar parser ────────────────────────────────────────────────────────────

/// Parse a KEY=VALUE sidecar file into a `HashMap<String, String>`.
///
/// Rules:
/// - Lines beginning with `#` (after optional leading ASCII space/tab) are comments.
/// - Blank lines are ignored.
/// - `KEY=VALUE` — value is everything after the first `=`; value may be empty.
/// - Keys and values are NOT unquoted / unescaped (fixtures use plain ASCII).
fn parse_env_sidecar(path: &std::path::Path) -> HashMap<String, String> {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read env sidecar {}: {}", path.display(), e));
    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].to_string();
            let val = trimmed[eq + 1..].to_string();
            map.insert(key, val);
        }
    }
    map
}

// ── JSON helpers (mirrors lightbend_test.rs) ─────────────────────────────────

fn normalize(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut m = serde_json::Map::new();
            for (k, val) in map {
                m.insert(k.clone(), normalize(val));
            }
            serde_json::Value::Object(m)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize).collect())
        }
        serde_json::Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            serde_json::json!(f)
        }
        other => other.clone(),
    }
}

fn hocon_to_json(v: &hocon::HoconValue) -> serde_json::Value {
    match v {
        hocon::HoconValue::Object(map) => {
            let mut m = serde_json::Map::new();
            for (k, val) in map {
                m.insert(k.clone(), hocon_to_json(val));
            }
            serde_json::Value::Object(m)
        }
        hocon::HoconValue::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(hocon_to_json).collect())
        }
        hocon::HoconValue::Scalar(sv) => match sv.value_type {
            hocon::ScalarType::Null => serde_json::Value::Null,
            hocon::ScalarType::Boolean => serde_json::Value::Bool(sv.raw == "true"),
            hocon::ScalarType::Number => {
                if !sv.raw.contains('.') && !sv.raw.contains('e') && !sv.raw.contains('E') {
                    if let Ok(n) = sv.raw.parse::<i64>() {
                        return serde_json::json!(n as f64);
                    }
                }
                if let Ok(f) = sv.raw.parse::<f64>() {
                    return serde_json::json!(f);
                }
                serde_json::Value::String(sv.raw.clone())
            }
            hocon::ScalarType::String => serde_json::Value::String(sv.raw.clone()),
            _ => serde_json::Value::String(sv.raw.clone()),
        },
        _ => unreachable!("unknown HoconValue variant"),
    }
}

fn key_to_lookup_path(key: &str) -> String {
    if key.is_empty()
        || key.contains('.')
        || key.contains('"')
        || key.contains('\\')
        || key.contains(' ')
        || key.contains('\t')
    {
        let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    } else {
        key.to_string()
    }
}

fn config_to_json(config: &hocon::Config) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for key in config.keys() {
        let path = key_to_lookup_path(key);
        if let Some(val) = config.get(&path) {
            m.insert(key.to_string(), hocon_to_json(val));
        }
    }
    normalize(&serde_json::Value::Object(m))
}

// ── Fixture helpers ───────────────────────────────────────────────────────────

fn parse_fixture_with_env(name: &str) -> Result<hocon::Config, hocon::HoconError> {
    let env = parse_env_sidecar(&env_path(name));
    hocon::parse_file_with_env(fixture_path(name), &env)
}

fn load_expected_json(name: &str) -> serde_json::Value {
    let path = expected_path(name);
    let s = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read expected {}: {}", path.display(), e));
    let v: serde_json::Value = serde_json::from_str(&s)
        .unwrap_or_else(|e| panic!("invalid JSON in {}: {}", path.display(), e));
    normalize(&v)
}

fn assert_fixture_matches(name: &str) {
    let cfg = parse_fixture_with_env(name)
        .unwrap_or_else(|e| panic!("s13c {}: unexpected parse/resolve error: {:?}", name, e));
    let got = config_to_json(&cfg);
    let expected = load_expected_json(name);
    assert_eq!(
        got,
        expected,
        "s13c {}: output mismatch\n  got:      {}\n  expected: {}",
        name,
        serde_json::to_string_pretty(&got).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap(),
    );
}

// ── ev01: basic env-var list expansion ───────────────────────────────────────

/// ev01: `x = ${S13C_EV01_MY_LIST[]}` with _0=a, _1=b → `{"x": ["a","b"]}`.
#[test]
fn s13c_ev01_basic() {
    assert_fixture_matches("ev01-basic");
}

// ── Remaining success fixtures (ev02, ev04–ev11) ─────────────────────────────

/// ev02: stops at gap (_0 present, _1 absent, _2 present → only ["a"]).
#[test]
fn s13c_ev02_stops_at_gap() {
    assert_fixture_matches("ev02-stops-at-gap");
}

/// ev04: optional with no _0 → key absent (`{}`).
#[test]
fn s13c_ev04_optional_no_elements() {
    assert_fixture_matches("ev04-optional-no-elements");
}

/// ev05: config-defined wins (E6). `${X[]}` returns config value, not env list.
#[test]
fn s13c_ev05_config_defined_wins() {
    assert_fixture_matches("ev05-config-defined-wins");
}

/// ev06: concat prepend — `["x","y"] ${?S13C_EV06_MY_LIST[]}` → `["x","y","a"]`.
#[test]
fn s13c_ev06_concat_prepend() {
    assert_fixture_matches("ev06-concat-prepend");
}

/// ev07: concat append — `${?S13C_EV07_MY_LIST[]} ["x","y"]` → `["a","x","y"]`.
#[test]
fn s13c_ev07_concat_append() {
    assert_fixture_matches("ev07-concat-append");
}

/// ev09: E7 — whitespace before `[]` in fixture: `${S13C_EV09_MY_LIST []}`.
#[test]
fn s13c_ev09_whitespace_before_suffix() {
    assert_fixture_matches("ev09-whitespace-before-suffix");
}

/// ev10: empty-string element is preserved (stop on absent key, not empty value).
#[test]
fn s13c_ev10_empty_string_element() {
    assert_fixture_matches("ev10-empty-string-element");
}

/// ev11: include context — relativized fallback strips include prefix.
#[test]
fn s13c_ev11_include_context() {
    assert_fixture_matches("ev11-include-context");
}

// ── Unit 7: error fixture ─────────────────────────────────────────────────────

/// ev03: required `${S13C_EV03_MY_LIST[]}` with no _0 → ResolveError.
#[test]
fn s13c_ev03_required_no_elements_errors() {
    let result = parse_fixture_with_env("ev03-required-no-elements");
    assert!(
        matches!(result, Err(hocon::HoconError::Resolve(_))),
        "ev03: required list with no _0 must raise ResolveError, got: {:?}",
        result
    );
}

// ── Unit 8: S13c.5 — scalar env fallback suppressed when list_suffix=true ────

/// When `list_suffix=true` and config lookup misses AND no `_0` element exists,
/// the resolver must NOT fall through to the scalar env fallback.
/// Required form must raise ResolveError.
#[test]
fn s13c_s5_required_no_scalar_fallback() {
    let mut env = HashMap::new();
    env.insert("S13C_BARE".into(), "scalar".into());
    // No S13C_BARE_0 → list lookup finds nothing → must error (not "scalar").
    let result = hocon::parse_with_env("x = ${S13C_BARE[]}", &env);
    assert!(
        matches!(result, Err(hocon::HoconError::Resolve(_))),
        "S13c.5: required list with no _0 must raise ResolveError even when bare key exists, got: {:?}",
        result
    );
}

/// Optional form with `list_suffix=true` and no `_0` → key absent, not scalar fallback.
#[test]
fn s13c_s5_optional_no_scalar_fallback() {
    let mut env = HashMap::new();
    env.insert("S13C_BARE_OPT".into(), "scalar".into());
    // No S13C_BARE_OPT_0 → optional list lookup finds nothing → key removed.
    let cfg = hocon::parse_with_env("x = ${?S13C_BARE_OPT[]}", &env)
        .expect("s13c.5: optional list with no _0 must parse OK (key dropped)");
    // x must be absent (not "scalar")
    assert!(
        cfg.get("x").is_none(),
        "S13c.5: optional list with no _0 must drop key (got {:?})",
        cfg.get("x")
    );
}

// ── Unit 9: ev08 — self-ref concat with list suffix ──────────────────────────

/// ev08: `x = ["x"]; x = ${?x} ${?LIST[]}` → `["x","a"]`.
///
/// The plan proposed a `#[should_panic]` tripwire (S13a.13 gap), but rs.hocon's
/// existing prior_values / self-ref-lookback logic already handles this case
/// correctly via the concat resolver and `join_pair`. Verified 2026-05-18:
/// ev08 resolves to `["x","a"]` without the S13a.13 fix. This fixture therefore
/// ships as a regular ✅ success test, not a tripwire.
#[test]
fn s13c_ev08_self_ref_concat() {
    assert_fixture_matches("ev08-self-append");
}

// ── Multi-agent-review regressions ───────────────────────────────────────────
//
// The following tests pin three bugs caught during /multi-agent-review of this
// branch (Codex Critical C1 + Important I1/I2; convergent with ts.hocon).

/// C1: `${X}` and `${X[]}` must NOT collide in the substitution cache.
///
/// Before the fix, the cache key was `segments_to_key(s.segments)` only, so
/// whichever form resolved first poisoned the cache for the other. Verified
/// in both directions to ensure neither path wins by accident.
#[test]
fn s13c_cache_disambiguation_scalar_then_list() {
    let mut env = HashMap::new();
    env.insert("S13C_CACHE_X".to_string(), "scalar-val".to_string());
    env.insert("S13C_CACHE_X_0".to_string(), "a".to_string());
    env.insert("S13C_CACHE_X_1".to_string(), "b".to_string());
    let cfg = hocon::parse_with_env("a = ${S13C_CACHE_X}\nb = ${S13C_CACHE_X[]}", &env)
        .expect("parse_with_env");
    assert_eq!(cfg.get_string("a").unwrap(), "scalar-val");
    let b = cfg.get_list("b").expect("b should be list");
    let texts: Vec<String> = b
        .iter()
        .map(|v| match v {
            HoconValue::Scalar(sv) => sv.raw.clone(),
            _ => panic!("expected scalar element in b"),
        })
        .collect();
    assert_eq!(texts, vec!["a", "b"]);
}

#[test]
fn s13c_cache_disambiguation_list_then_scalar() {
    let mut env = HashMap::new();
    env.insert("S13C_CACHE2_X".to_string(), "scalar-val".to_string());
    env.insert("S13C_CACHE2_X_0".to_string(), "a".to_string());
    env.insert("S13C_CACHE2_X_1".to_string(), "b".to_string());
    let cfg = hocon::parse_with_env("a = ${S13C_CACHE2_X[]}\nb = ${S13C_CACHE2_X}", &env)
        .expect("parse_with_env");
    let a = cfg.get_list("a").expect("a should be list");
    let texts: Vec<String> = a
        .iter()
        .map(|v| match v {
            HoconValue::Scalar(sv) => sv.raw.clone(),
            _ => panic!("expected scalar element in a"),
        })
        .collect();
    assert_eq!(texts, vec!["a", "b"]);
    assert_eq!(cfg.get_string("b").unwrap(), "scalar-val");
}

/// `${X.[]}` must be rejected as empty-segment-before-suffix at parse time.
///
/// The empty-segment guard at the `'[' =>` arm fires when `!cur_started` (no
/// segment text is being accumulated), uniformly handling both `${[]}` (no
/// segments at all) and `${X.[]}` (trailing dot just reset cur_started but
/// segments already has entries from prior parsing).
///
/// Tests assert specifically `Err(HoconError::Parse(_))` rather than any error
/// because an empty env map could otherwise let resolution fail with
/// HoconError::Resolve and mask a lexer regression (Copilot review on rs#88).
#[test]
fn s13c_lex_trailing_dot_before_suffix_errors() {
    let env = HashMap::new();
    let err = hocon::parse_with_env("x = ${A.[]}", &env);
    assert!(
        matches!(err, Err(hocon::HoconError::Parse(_))),
        "expected HoconError::Parse for ${{A.[]}}, got {:?}",
        err
    );
}

#[test]
fn s13c_lex_trailing_dot_space_before_suffix_errors() {
    let env = HashMap::new();
    let err = hocon::parse_with_env("x = ${A . []}", &env);
    assert!(
        matches!(err, Err(hocon::HoconError::Parse(_))),
        "expected HoconError::Parse for ${{A . []}}, got {:?}",
        err
    );
}

/// E7 narrow-allow-list — only ASCII SPACE / TAB allowed before `[]`.
///
/// The `[` arm validates pending_ws and rejects any non-{space,tab} char.
/// General subst-body inter-segment whitespace is broader (S6 set: NBSP, CR,
/// Zs, BOM, …) but at the `[` arm we are strict per extra-spec E7.
///
/// Tests assert specifically `Err(HoconError::Parse(_))` for the same reason as
/// the trailing-dot tests above (Copilot review on rs#88).
#[test]
fn s13c_lex_nbsp_before_suffix_errors() {
    let env = HashMap::new();
    let err = hocon::parse_with_env("x = ${A\u{00A0}[]}", &env);
    assert!(
        matches!(err, Err(hocon::HoconError::Parse(_))),
        "expected HoconError::Parse for ${{A\\u00A0[]}} (NBSP), got {:?}",
        err
    );
}

#[test]
fn s13c_lex_cr_before_suffix_errors() {
    let env = HashMap::new();
    let err = hocon::parse_with_env("x = ${A\r[]}", &env);
    assert!(
        matches!(err, Err(hocon::HoconError::Parse(_))),
        "expected HoconError::Parse for ${{A\\r[]}} (CR), got {:?}",
        err
    );
}

#[test]
fn s13c_lex_zs_em_space_before_suffix_errors() {
    let env = HashMap::new();
    let err = hocon::parse_with_env("x = ${A\u{2003}[]}", &env);
    assert!(
        matches!(err, Err(hocon::HoconError::Parse(_))),
        "expected HoconError::Parse for ${{A\\u2003[]}} (em-space), got {:?}",
        err
    );
}

/// E7 positive sanity: ASCII space + tab should still pass.
#[test]
fn s13c_lex_e7_ascii_space_tab_before_suffix_ok() {
    let mut env = HashMap::new();
    env.insert("S13C_E7_OK_0".to_string(), "v".to_string());
    let cfg = hocon::parse_with_env("x = ${S13C_E7_OK []}", &env)
        .expect("space before [] should be accepted");
    assert!(cfg.has("x"));
    let cfg2 = hocon::parse_with_env("x = ${S13C_E7_OK\t[]}", &env)
        .expect("tab before [] should be accepted");
    assert!(cfg2.has("x"));
}

/// `${"a"[]}` — quoted segment followed by suffix. Sanity-pin positive.
#[test]
fn s13c_lex_quoted_segment_with_suffix_ok() {
    let mut env = HashMap::new();
    env.insert("a_0".to_string(), "v".to_string());
    let cfg = hocon::parse_with_env(r#"x = ${"a"[]}"#, &env)
        .expect("quoted segment + suffix should be accepted");
    let list = cfg.get_list("x").expect("x should be list");
    assert_eq!(list.len(), 1);
    match &list[0] {
        HoconValue::Scalar(sv) => assert_eq!(sv.raw, "v"),
        _ => panic!("expected scalar element"),
    }
}
