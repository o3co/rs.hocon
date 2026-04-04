use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon")
}

#[test]
fn debug_combined() {
    let input = r#"
foo {
  include "test09.conf"
}

bar {
  nested {
    include "test09.conf"
  }
}
"#;
    match hocon::parse(input) {
        Ok(config) => {
            println!("foo.y = {:?}", config.get_i64_option("foo.y"));
            println!("foo.b = {:?}", config.get_i64_option("foo.b"));
            println!("bar.nested.y = {:?}", config.get_i64_option("bar.nested.y"));
            println!("bar.nested.b = {:?}", config.get_i64_option("bar.nested.b"));
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
