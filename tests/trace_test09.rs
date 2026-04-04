use std::path::PathBuf;

fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("testdata")
        .join("hocon")
}

#[test]
fn trace_test09_structure() {
    let path = testdata_dir().join("test09.conf");
    let config = hocon::parse_file(&path)
        .unwrap_or_else(|e| panic!("ParseFile({}) failed: {}", path.display(), e));

    println!("\n=== Test09 Analysis ===");
    println!("\nExpected structure:");
    println!("a = {{c:3, q:10}} (merges a=${{x}} with a={{c:3}})");
    println!("b = 5");
    println!("c = {{d:600, e:{{c:3, q:10}}, f:5, q:10}} (merges c=${{x}} with c={{...}})");
    println!("x = {{q:10}}");
    println!("y = 5");

    println!("\nActual values:");
    println!("a = {:?}", config.get("a"));
    if let Some(a) = config.get("a") {
        println!("  a as debug: {:#?}", a);
    }

    println!("\nb = {:?}", config.get("b"));
    println!("c = {:?}", config.get("c"));
    if let Some(c) = config.get("c") {
        println!("  c as debug: {:#?}", c);
    }

    println!("\nProblem diagnosis:");
    println!("- a should have q:10 from merge of a=${{x}} and a={{c:3}}");
    println!("  Actual a.q: {:?}", config.get_i64("a.q"));

    println!("\n- b should be 5, not object");
    println!("  Actual b: {:?}", config.get_i64("b"));

    println!("\n- c should have q:10 from merge of c=${{x}} and c={{...}}");
    println!("  Actual c.q: {:?}", config.get_i64("c.q"));

    println!("\n- c.e should have q:10 since e=${{a}} and a={{c:3, q:10}}");
    println!("  Actual c.e.q: {:?}", config.get_i64("c.e.q"));

    println!("\n- c.f should be 5 since f=${{b}} and b=5");
    println!("  Actual c.f: {:?}", config.get("c.f"));
}
