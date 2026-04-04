use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("testdata")
        .join("hocon")
}

#[test]
fn debug_test09_full_output() {
    let path = testdata_dir().join("test09.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    println!("\n=== Full config ===");
    println!("{:#?}", config);

    println!("\n=== Specific key checks ===");
    println!("a.c: expected=3, actual={:?}", config.get_i64("a.c"));
    println!("a.q: expected=10, actual={:?}", config.get_i64("a.q"));
    println!("b: expected=5, actual={:?}", config.get_i64("b"));
    println!("c.d: expected=600, actual={:?}", config.get_i64("c.d"));
    println!("c.e.c: expected=3, actual={:?}", config.get_i64("c.e.c"));
    println!("c.e.q: expected=10, actual={:?}", config.get_i64("c.e.q"));
    println!("c.f: expected=5, actual={:?}", config.get_i64("c.f"));
    println!("c.q: expected=10, actual={:?}", config.get_i64("c.q"));
}
