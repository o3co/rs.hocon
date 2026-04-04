#[test]
fn simple_scope() {
    let input = r#"
y = 5
obj {
  a = ${y}
}
"#;
    match hocon::parse(input) {
        Ok(config) => {
            println!("y = {:?}", config.get_i64_option("y"));
            println!("obj.a = {:?}", config.get_i64_option("obj.a"));
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
