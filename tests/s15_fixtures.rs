/// S15 cross-impl fixture tests.
///
/// Loads each xx.hocon fixture from testdata/hocon/numeric-obj-array/ and
/// asserts `get_list` returns the spec-canonical o3co result.
///
/// Per-fixture notes:
/// - na04: empty object → getList errors (S15.4 — not converted)
/// - na08: leading-zero key rejected (E2 divergence from Lightbend)
/// - na10a: leading-plus key rejected (E3 divergence)
/// - na10b: leading-minus-zero key rejected (E4 divergence)
/// - na12: no eligible keys → getList errors
/// - na03a/na03b: concat-side conversion (tested after resolver wiring)
/// - na03c/na03d: multi-piece concat (tested after resolver wiring)
use std::collections::HashMap;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/hocon/numeric-obj-array")
}

fn load_fixture(name: &str) -> hocon::Config {
    let path = fixture_dir().join(name);
    hocon::parse_file_with_env(&path, &HashMap::new())
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", name, e))
}

fn scalar_raw(v: &hocon::HoconValue) -> &str {
    match v {
        hocon::HoconValue::Scalar(sv) => &sv.raw,
        other => panic!("expected Scalar, got {:?}", other),
    }
}

// ── na01: basic conversion (S15.1) ────────────────────────────────────────────

#[test]
fn na01_basic_get_list_returns_two_elements() {
    let cfg = load_fixture("na01-basic.conf");
    let items = cfg
        .get_list("items")
        .expect("na01: getList on {\"0\":\"a\",\"1\":\"b\"} must succeed");
    assert_eq!(items.len(), 2, "na01: expected 2 elements");
    assert_eq!(scalar_raw(&items[0]), "a", "na01: items[0] must be \"a\"");
    assert_eq!(scalar_raw(&items[1]), "b", "na01: items[1] must be \"b\"");
}

// ── na02: laziness (S15.2) ────────────────────────────────────────────────────

#[test]
fn na02_lazy_get_config_still_works_and_get_list_also_works() {
    let cfg = load_fixture("na02-lazy-getobject.conf");
    // Object access must NOT be blocked even though the object is numeric-keyed
    assert!(
        cfg.get_config("items").is_ok(),
        "na02: get_config must still succeed (laziness preserved)"
    );
    // List access must trigger conversion
    let items = cfg
        .get_list("items")
        .expect("na02: getList must trigger lazy conversion");
    assert_eq!(items.len(), 2, "na02: expected 2 elements after conversion");
}

// ── na03a: concat-time conversion, left-literal-array (S15.3) ─────────────────

#[test]
fn na03a_concat_left_list_produces_three_elements() {
    let cfg = load_fixture("na03a-concat-left-list.conf");
    let items = cfg
        .get_list("arr")
        .expect("na03a: getList on concat result must succeed");
    // Expected: ["a", "x", "y"]
    assert_eq!(items.len(), 3, "na03a: expected [\"a\",\"x\",\"y\"] (3 elements), got {:?}", items);
    assert_eq!(scalar_raw(&items[0]), "a", "na03a: items[0] must be \"a\"");
    assert_eq!(scalar_raw(&items[1]), "x", "na03a: items[1] must be \"x\"");
    assert_eq!(scalar_raw(&items[2]), "y", "na03a: items[2] must be \"y\"");
}

// ── na03b: concat-time conversion, right-literal-array (S15.3 symmetric) ──────

#[test]
fn na03b_concat_right_list_produces_three_elements() {
    let cfg = load_fixture("na03b-concat-right-list.conf");
    let items = cfg
        .get_list("arr")
        .expect("na03b: getList on concat result must succeed");
    // Expected: ["x", "y", "a"]
    assert_eq!(items.len(), 3, "na03b: expected [\"x\",\"y\",\"a\"] (3 elements), got {:?}", items);
    assert_eq!(scalar_raw(&items[0]), "x", "na03b: items[0] must be \"x\"");
    assert_eq!(scalar_raw(&items[1]), "y", "na03b: items[1] must be \"y\"");
    assert_eq!(scalar_raw(&items[2]), "a", "na03b: items[2] must be \"a\"");
}

// ── na03c: two-object concat NOT converted at concat time, BUT accessible as list ─

#[test]
fn na03c_concat_two_objs_produces_merged_object_or_list() {
    // S10.3: obj+obj → deep-merge object. Accessor-side conversion then fires.
    // Expected: arr as object {"0":"x","1":"y","2":"z","3":"w"}, but since
    // all keys are numeric, get_list must succeed via accessor-time conversion.
    let cfg = load_fixture("na03c-concat-two-objs.conf");
    let items = cfg
        .get_list("arr")
        .expect("na03c: getList on merged numeric object must succeed via accessor conversion");
    // {"0":"x","1":"y","2":"z","3":"w"} → ["x","y","z","w"]
    assert_eq!(items.len(), 4, "na03c: expected 4 elements, got {:?}", items);
    assert_eq!(scalar_raw(&items[0]), "x");
    assert_eq!(scalar_raw(&items[1]), "y");
    assert_eq!(scalar_raw(&items[2]), "z");
    assert_eq!(scalar_raw(&items[3]), "w");
}

// ── na03d: multi-piece concat, left-to-right pairwise (NORMATIVE) ──────────────

#[test]
fn na03d_concat_multi_piece_left_to_right_pairwise() {
    // obj1=${0:x,1:y}, obj2={2:z,3:w}, arr=${obj1} ${obj2} [a]
    // Step 1: join(obj1, obj2) → {0:x,1:y,2:z,3:w} (object merge)
    // Step 2: join(merged, [a]) → numericObjectToArray(merged) → [x,y,z,w] ++ [a]
    // Expected: ["x","y","z","w","a"]
    let cfg = load_fixture("na03d-concat-multi-piece.conf");
    let items = cfg
        .get_list("arr")
        .expect("na03d: getList on multi-piece concat must succeed");
    assert_eq!(
        items.len(), 5,
        "na03d: NORMATIVE multi-piece: expected [\"x\",\"y\",\"z\",\"w\",\"a\"], got {:?}",
        items
    );
    assert_eq!(scalar_raw(&items[0]), "x");
    assert_eq!(scalar_raw(&items[1]), "y");
    assert_eq!(scalar_raw(&items[2]), "z");
    assert_eq!(scalar_raw(&items[3]), "w");
    assert_eq!(scalar_raw(&items[4]), "a");
}

// ── na04: empty object NOT converted (S15.4) ──────────────────────────────────

#[test]
fn na04_empty_object_not_converted() {
    let cfg = load_fixture("na04-empty.conf");
    assert!(
        cfg.get_list("items").is_err(),
        "na04: empty object must NOT convert — getList must return an error"
    );
}

// ── na05: non-integer keys ignored (S15.5) ────────────────────────────────────

#[test]
fn na05_non_int_keys_ignored() {
    let cfg = load_fixture("na05-non-int-keys.conf");
    let items = cfg
        .get_list("items")
        .expect("na05: getList must succeed ignoring non-int key \"foo\"");
    assert_eq!(items.len(), 2, "na05: only keys \"0\" and \"1\" are eligible");
    assert_eq!(scalar_raw(&items[0]), "a");
    assert_eq!(scalar_raw(&items[1]), "c");
}

// ── na06: gaps compacted (S15.6) ──────────────────────────────────────────────

#[test]
fn na06_gaps_compacted() {
    let cfg = load_fixture("na06-gaps.conf");
    let items = cfg
        .get_list("items")
        .expect("na06: getList on gapped keys must succeed");
    assert_eq!(items.len(), 2, "na06: keys 0+2 → 2-element array (gap compacted)");
    assert_eq!(scalar_raw(&items[0]), "a");
    assert_eq!(scalar_raw(&items[1]), "c");
}

// ── na07: sort by integer key (S15.7) ─────────────────────────────────────────

#[test]
fn na07_sorted_by_key() {
    let cfg = load_fixture("na07-sort.conf");
    let items = cfg
        .get_list("items")
        .expect("na07: getList on reversed-key object must succeed");
    assert_eq!(items.len(), 2, "na07: expected 2 elements");
    assert_eq!(scalar_raw(&items[0]), "a", "na07: key 0 comes first");
    assert_eq!(scalar_raw(&items[1]), "b", "na07: key 1 comes second");
}

// ── na08: leading-zero rejected (E2 — o3co divergence from Lightbend) ─────────

#[test]
fn na08_leading_zero_rejected_only_canonical_zero_eligible() {
    // "00" is non-canonical → rejected. Only "0" is eligible → ["b"]
    let cfg = load_fixture("na08-leading-zero.conf");
    let items = cfg
        .get_list("items")
        .expect("na08: getList must succeed with only key \"0\" eligible");
    assert_eq!(items.len(), 1, "na08: only \"0\" eligible → 1 element");
    assert_eq!(scalar_raw(&items[0]), "b");
}

// ── na09: negative keys rejected (Lightbend-aligned) ─────────────────────────

#[test]
fn na09_negative_key_rejected() {
    // "-1" rejected. Only "0" eligible → ["b"]
    let cfg = load_fixture("na09-negative.conf");
    let items = cfg
        .get_list("items")
        .expect("na09: getList must succeed with only key \"0\" eligible");
    assert_eq!(items.len(), 1, "na09: only \"0\" eligible → 1 element");
    assert_eq!(scalar_raw(&items[0]), "b");
}

// ── na10a: leading-plus rejected (E3 — o3co divergence from Lightbend) ────────

#[test]
fn na10a_plus_sign_rejected() {
    // "+1" non-canonical → rejected. Only "0" eligible → ["b"]
    let cfg = load_fixture("na10a-plus-sign.conf");
    let items = cfg
        .get_list("items")
        .expect("na10a: getList must succeed with only key \"0\" eligible");
    assert_eq!(items.len(), 1, "na10a: only \"0\" eligible → 1 element");
    assert_eq!(scalar_raw(&items[0]), "b");
}

// ── na10b: minus-zero rejected (E4 — o3co divergence from Lightbend) ──────────

#[test]
fn na10b_minus_zero_rejected() {
    // "-0" non-canonical → rejected. Only "0" eligible → ["b"]
    let cfg = load_fixture("na10b-minus-zero.conf");
    let items = cfg
        .get_list("items")
        .expect("na10b: getList must succeed with only key \"0\" eligible");
    assert_eq!(items.len(), 1, "na10b: only \"0\" eligible → 1 element");
    assert_eq!(scalar_raw(&items[0]), "b");
}

// ── na11: overflow rejected ───────────────────────────────────────────────────

#[test]
fn na11_overflow_key_rejected() {
    // "99999999999" > i32::MAX → rejected. Only "0" eligible → ["b"]
    let cfg = load_fixture("na11-overflow.conf");
    let items = cfg
        .get_list("items")
        .expect("na11: getList must succeed with only key \"0\" eligible");
    assert_eq!(items.len(), 1, "na11: only \"0\" eligible → 1 element");
    assert_eq!(scalar_raw(&items[0]), "b");
}

// ── na12: no eligible keys → error ───────────────────────────────────────────

#[test]
fn na12_no_eligible_keys_errors() {
    let cfg = load_fixture("na12-no-eligible.conf");
    assert!(
        cfg.get_list("items").is_err(),
        "na12: no integer keys → getList must return an error"
    );
}
