use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::tempdir;

/// Global lock for tests that change the process-wide CWD.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata")
        .join(name)
}

#[test]
fn parse_file_simple() {
    let config = hocon::parse_file(testdata("base.conf")).unwrap();
    assert_eq!(config.get_string("host").unwrap(), "localhost");
    assert_eq!(config.get_i64("port").unwrap(), 8080);
}

#[test]
fn include_merges_into_current() {
    let config = hocon::parse_file(testdata("with_include.conf")).unwrap();
    assert_eq!(config.get_string("host").unwrap(), "localhost");
    assert_eq!(config.get_i64("port").unwrap(), 8080);
    assert!(config.get_bool("debug").unwrap());
}

#[test]
fn include_override_by_later_key() {
    let config = hocon::parse_file(testdata("override_include.conf")).unwrap();
    assert_eq!(config.get_string("host").unwrap(), "localhost");
    assert_eq!(config.get_i64("port").unwrap(), 9090);
}

#[test]
fn include_nested_directory() {
    let config = hocon::parse_file(testdata("with_nested_include.conf")).unwrap();
    assert_eq!(config.get_string("db_host").unwrap(), "db.local");
    assert_eq!(config.get_string("app").unwrap(), "myapp");
}

#[test]
fn include_circular_detection() {
    let result = hocon::parse_file(testdata("circular_a.conf"));
    assert!(result.is_err());
}

#[test]
fn include_extension_probing() {
    let config = hocon::parse_file(testdata("ext_probe.conf")).unwrap();
    assert_eq!(config.get_string("found").unwrap(), "yes");
    assert!(config.get_bool("extra").unwrap());
}

#[test]
fn include_probe_order_conf_wins() {
    let config = hocon::parse_file(testdata("probe-order-wrapper.conf")).unwrap();
    assert!(config.get_bool("from_json").unwrap());
    assert!(config.get_bool("from_conf").unwrap());
    assert_eq!(config.get_string("shared").unwrap(), "conf");
}

#[test]
fn include_missing_silently_ignored() {
    let config = hocon::parse(
        r#"include "nonexistent.conf"
fallback = true"#,
    )
    .unwrap();
    assert!(config.get_bool("fallback").unwrap());
}

#[test]
fn include_relativize_quoted_key_with_dots() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("child.conf"), "x = 1\ny = ${x}").unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(r#""a.b" {{ include "{}/child.conf" }}"#, dir_str);
    let config = hocon::parse(&input).unwrap();
    assert_eq!(config.get_i64(r#""a.b".x"#).unwrap(), 1);
    assert_eq!(config.get_i64(r#""a.b".y"#).unwrap(), 1);
}

#[test]
fn include_env_fallback_quoted_key_prefix() {
    struct EnvGuard {
        key: &'static str,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            std::env::remove_var(self.key);
        }
    }

    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("child.conf"), "val = ${MY_TEST_VAR_QK}").unwrap();
    std::env::set_var("MY_TEST_VAR_QK", "ok");
    let _guard = EnvGuard {
        key: "MY_TEST_VAR_QK",
    };
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(r#""a.b" {{ include "{}/child.conf" }}"#, dir_str);
    let config = hocon::parse(&input).unwrap();
    assert_eq!(config.get_string(r#""a.b".val"#).unwrap(), "ok");
    // _guard drops here, removing the env var
}

#[test]
fn file_include_resolves_from_cwd_not_including_dir() {
    // Prove that `include file("child.conf")` resolves relative to CWD,
    // NOT relative to the including file's directory.
    //
    // Layout:
    //   tmpdir/child.conf          -> cwd_key = 99   (CWD-level)
    //   tmpdir/sub/parent.conf     -> includes child.conf via bare + file()
    //   tmpdir/sub/child.conf      -> child_key = 1   (including-file-level)
    //
    // CWD is set to tmpdir.  Therefore:
    //   bare include "child.conf"  -> resolves relative to sub/ -> child_key = 1
    //   include file("child.conf") -> resolves relative to CWD  -> cwd_key = 99
    let _lock = CWD_LOCK.lock().unwrap();
    let prev_cwd = std::env::current_dir().unwrap();

    let dir = tempdir().unwrap();
    let subdir = dir.path().join("sub");
    std::fs::create_dir(&subdir).unwrap();

    // child.conf at CWD level (tmpdir/)
    std::fs::write(dir.path().join("child.conf"), "cwd_key = 99").unwrap();
    // child.conf in including file's directory (tmpdir/sub/)
    std::fs::write(subdir.join("child.conf"), "child_key = 1").unwrap();

    let abs_child = subdir
        .join("child.conf")
        .display()
        .to_string()
        .replace('\\', "/");
    let parent_content = format!(
        concat!(
            "bare_ok = true\n",
            "include \"child.conf\"\n",
            "include file(\"child.conf\")\n",
            "include file(\"{}\")\n",
        ),
        abs_child
    );
    std::fs::write(subdir.join("parent.conf"), &parent_content).unwrap();

    // Set CWD to tmpdir so file("child.conf") picks up the CWD-level file
    std::env::set_current_dir(dir.path()).unwrap();
    let config = hocon::parse_file(subdir.join("parent.conf")).unwrap();
    std::env::set_current_dir(&prev_cwd).unwrap();

    // bare include resolved relative to sub/ -> child_key = 1
    assert_eq!(config.get_i64("child_key").unwrap(), 1);
    // file("child.conf") resolved relative to CWD (tmpdir/) -> cwd_key = 99
    assert_eq!(config.get_i64("cwd_key").unwrap(), 99);
    // bare_ok is set
    assert!(config.get_bool("bare_ok").unwrap());
    // file() with absolute path also found the child (child_key still 1)
}

// S14c.2 (rs.hocon#44): config-path fallback for relativized substitutions.
//
// When a substitution inside an included file references an ancestor-scope
// variable that doesn't exist at the relativized path, resolution must fall
// back to the original (non-relativized) path against the merged root —
// matching Lightbend's "resolve against the fully merged tree" behaviour.
//
// Pre-fix: only env var fallback honoured the original path; config-path
// fallback was missing, so `${y}` inside an included file relativized to
// `${bar.y}` would fail when `y` only existed at root.

#[test]
fn s14c_2_ancestor_scope_var_fallback_after_relativization() {
    let dir = tempdir().unwrap();
    // child.conf references `y` which lives at the ROOT scope, not under the
    // include's `bar` prefix. After relativization, `${y}` becomes `${bar.y}`,
    // which doesn't exist — the fallback must try the original `${y}` path.
    std::fs::write(dir.path().join("ref.conf"), "ref = ${y}\n").unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(
        r#"y = "root-y"
bar {{ include "{}/ref.conf" }}
"#,
        dir_str
    );
    let config = hocon::parse(&input).unwrap();
    // bar.ref should resolve to "root-y" via the original-path fallback.
    assert_eq!(config.get_string("bar.ref").unwrap(), "root-y");
}

#[test]
fn s14c_2_relativized_path_still_wins_when_both_exist() {
    let dir = tempdir().unwrap();
    // Both `y` (root) and `bar.y` (relativized) exist. The relativized path
    // takes precedence — this pins that the fallback does NOT shadow the
    // primary lookup.
    std::fs::write(dir.path().join("ref.conf"), "y = \"local-y\"\nref = ${y}\n").unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(
        r#"y = "root-y"
bar {{ include "{}/ref.conf" }}
"#,
        dir_str
    );
    let config = hocon::parse(&input).unwrap();
    // bar.ref should see the relativized bar.y ("local-y"), not the root y.
    assert_eq!(config.get_string("bar.ref").unwrap(), "local-y");
    assert_eq!(config.get_string("y").unwrap(), "root-y");
}

#[test]
fn s14c_2_optional_substitution_falls_back_to_original() {
    let dir = tempdir().unwrap();
    // Optional substitution form: `${?y}` — same fallback rule applies.
    std::fs::write(dir.path().join("ref.conf"), "ref = ${?y}\n").unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(
        r#"y = "from-root"
bar {{ include "{}/ref.conf" }}
"#,
        dir_str
    );
    let config = hocon::parse(&input).unwrap();
    assert_eq!(config.get_string("bar.ref").unwrap(), "from-root");
}

#[test]
fn s14c_2_neither_path_resolves_still_errors() {
    let dir = tempdir().unwrap();
    // Neither relativized (bar.y) nor original (y) exists — must still error
    // (mandatory substitution). This pins that the fallback doesn't mask
    // legitimate "key not found" errors.
    std::fs::write(dir.path().join("ref.conf"), "ref = ${y}\n").unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(r#"bar {{ include "{}/ref.conf" }}"#, dir_str);
    let err = hocon::parse(&input).expect_err("expected resolve error");
    assert!(
        err.to_string().contains("y") || err.to_string().contains("resolve"),
        "error should mention the missing key 'y' or resolution failure, got: {}",
        err
    );
}
