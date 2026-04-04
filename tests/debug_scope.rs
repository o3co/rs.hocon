use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/hocon")
}

#[test]
fn debug_scope_test() {
    // Simulating what happens when we include test09.conf in bar.nested
    let input = r#"
y = 5
bar {
  nested {
    x={ q : 10 }
    a=1
    a.q.r.s=${b}
    a=${y}
    a=${x}
    a={ c : 3 }
    b=${x}
    b=${y}
    c=${x}
    c={ d : 600, e : ${a}, f : ${b} }
  }
}
"#;
    match hocon::parse(input) {
        Ok(config) => {
            println!("y = {:?}", config.get_i64_option("y"));
            println!("bar.nested.y = {:?}", config.get_i64_option("bar.nested.y"));
            println!("bar.nested.b = {:?}", config.get_i64_option("bar.nested.b"));
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
