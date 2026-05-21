//! Cross-impl regression tests for go.hocon#106 — include ordering and
//! self-referential append through include. `rs.hocon`'s
//! `deep_merge_res_obj_into` (src wins + prior capture) already implements
//! Lightbend-equivalent semantics; these tests pin that behaviour so a
//! future refactor cannot regress to go.hocon's pre-fix shape.

use tempfile::tempdir;

#[test]
fn issue106_include_scalar_overrides_parent() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("child.conf"), "a = 2\n").unwrap();
    let input = format!("a = 1\ninclude \"{}/child.conf\"\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 2);
}

#[test]
fn issue106_parent_scalar_after_include_wins() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("child.conf"), "a = 2\n").unwrap();
    let input = format!("include \"{}/child.conf\"\na = 5\n", dir_str);
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 5);
}

#[test]
fn issue106_self_ref_append_through_include() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(
        dir.path().join("child.conf"),
        "steps = ${steps} [\n  { name = child }\n]\n",
    )
    .unwrap();
    let input = format!(
        "steps = [\n  {{ name = base }}\n]\n\ninclude \"{}/child.conf\"\n",
        dir_str,
    );
    let cfg = hocon::parse(&input).unwrap();
    let steps = cfg.get_list("steps").unwrap();
    assert_eq!(
        steps.len(),
        2,
        "expected 2 steps (base + child), got {}",
        steps.len()
    );
    // Pin element order + content: a reversed-order or duplicate-element bug
    // would also produce len()==2 but break the spec semantics. Each element
    // must be an object with the expected `name` field.
    let name_at = |i: usize| -> String {
        let item = &steps[i];
        if let hocon::HoconValue::Object(fields) = item {
            if let Some(hocon::HoconValue::Scalar(s)) = fields.get("name") {
                return s.raw.clone();
            }
        }
        panic!(
            "steps[{}] is not an object with a 'name' field: {:?}",
            i, item
        );
    };
    assert_eq!(name_at(0), "base", "steps[0].name");
    assert_eq!(name_at(1), "child", "steps[1].name");
}

#[test]
fn issue106_control_same_file_self_ref_append() {
    let cfg = hocon::parse(
        "steps = [\n  { name = base }\n]\n\nsteps = ${steps} [\n  { name = child }\n]\n",
    )
    .unwrap();
    let steps = cfg.get_list("steps").unwrap();
    assert_eq!(steps.len(), 2);
}

#[test]
fn issue106_object_collision_deep_merge_through_include() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("child.conf"), "server { port = 8080 }\n").unwrap();
    let input = format!(
        "server {{ host = \"localhost\" }}\ninclude \"{}/child.conf\"\n",
        dir_str,
    );
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_string("server.host").unwrap(), "localhost");
    assert_eq!(cfg.get_i64("server.port").unwrap(), 8080);
}

#[test]
fn issue106_nested_include_does_not_leak_to_top_level() {
    // Multi-agent-review regression scenario from go.hocon: nested-include
    // override must NOT leak the prior under the bare leaf key into the
    // resolver-wide scope. An unrelated top-level self-ref with the same
    // leaf must see "no prior" and drop (when optional) — not the nested
    // value.
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("leaf.conf"), "a = innerB\n").unwrap();
    let input = format!(
        "nested {{\n  a = innerA\n  include \"{}/leaf.conf\"\n}}\na = ${{?a}}suffix\n",
        dir_str,
    );
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_string("nested.a").unwrap(), "innerB");
    assert_eq!(cfg.get_string("a").unwrap(), "suffix");
}

#[test]
fn issue106_sequential_includes_chain_priors() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(dir.path().join("c1.conf"), "a = 2\n").unwrap();
    std::fs::write(dir.path().join("c2.conf"), "a = 3\n").unwrap();
    let input = format!(
        "a = 1\ninclude \"{}/c1.conf\"\ninclude \"{}/c2.conf\"\n",
        dir_str, dir_str,
    );
    let cfg = hocon::parse(&input).unwrap();
    assert_eq!(cfg.get_i64("a").unwrap(), 3);
}
