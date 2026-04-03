fn main() {
    let test_cases = vec![
        "{ a = 1 } }",
        "{ a = 1 } garbage",
        "{ a = 1 } invalid_token",
    ];

    for tc in test_cases {
        match hocon::parse(tc) {
            Ok(_) => println!("Input: {:?} -> OK (no error)", tc),
            Err(e) => println!("Input: {:?} -> Error: {}", tc, e),
        }
    }
}
