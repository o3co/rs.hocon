use hocon::HoconError;
use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon")
}

#[test]
fn debug_test10() {
    let path = testdata_dir().join("test10.conf");
    let result = hocon::parse_file(&path);
    match result {
        Ok(config) => {
            println!("Success: {:?}", config);
        }
        Err(e) => {
            println!("Error type: {:?}", e);
            println!("Error message: {}", e);
        }
    }
}
