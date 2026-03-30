use std::path::PathBuf;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata").join(name)
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
fn include_missing_silently_ignored() {
    let config = hocon::parse(r#"include "nonexistent.conf"
fallback = true"#).unwrap();
    assert!(config.get_bool("fallback").unwrap());
}
