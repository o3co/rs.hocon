use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon")
}

#[test]
fn test10_debug() {
    let path = testdata_dir().join("test10.conf");
    match hocon::parse_file(&path) {
        Ok(config) => {
            println!("Success!");
            println!("foo.y = {:?}", config.get_i64_option("foo.y"));
            println!("bar.nested.y = {:?}", config.get_i64_option("bar.nested.y"));
        }
        Err(e) => {
            println!("Error details:");
            println!("  Type: {:?}", std::mem::discriminant(&e));
            println!("  Message: {}", e);
            match e {
                hocon::HoconError::Resolve(re) => {
                    println!("  Path: {}", re.path);
                    println!("  Line: {}", re.line);
                    println!("  Col: {}", re.col);
                }
                _ => {}
            }
        }
    }
}
