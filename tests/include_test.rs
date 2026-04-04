use std::path::PathBuf;
use tempfile::tempdir;

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
    assert_eq!(config.get_bool("from_json").unwrap(), true);
    assert_eq!(config.get_bool("from_conf").unwrap(), true);
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
    let input = format!(
        r#""a.b" {{ include "{}/child.conf" }}"#,
        dir_str
    );
    let config = hocon::parse(&input).unwrap();
    assert_eq!(config.get_i64(r#""a.b".x"#).unwrap(), 1);
    assert_eq!(config.get_i64(r#""a.b".y"#).unwrap(), 1);
}

#[test]
fn include_env_fallback_quoted_key_prefix() {
    struct EnvGuard { key: &'static str }
    impl Drop for EnvGuard {
        fn drop(&mut self) { std::env::remove_var(self.key); }
    }

    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("child.conf"), "val = ${MY_TEST_VAR_QK}").unwrap();
    std::env::set_var("MY_TEST_VAR_QK", "ok");
    let _guard = EnvGuard { key: "MY_TEST_VAR_QK" };
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    let input = format!(
        r#""a.b" {{ include "{}/child.conf" }}"#,
        dir_str
    );
    let config = hocon::parse(&input).unwrap();
    assert_eq!(config.get_string(r#""a.b".val"#).unwrap(), "ok");
    // _guard drops here, removing the env var
}
