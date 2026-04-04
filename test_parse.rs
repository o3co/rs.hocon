fn main() {
    let path = std::path::PathBuf::from("tests/testdata/hocon/test10.conf");
    match hocon::parse_file(&path) {
        Ok(config) => {
            println!("Success: {:?}", config);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
