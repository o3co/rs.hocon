//! Configuration that stays inside its module (README section of that name).
//!
//! The `auth` module ships its defaults as a package, reads only its own
//! subtree through its own types, and never names the application's keys;
//! the application includes the defaults, overrides what differs and wires
//! `auth.db` to a shared value.
//!
//! Run: `cargo run --example module_config --features serde,include-package`

/// The auth module. Nothing outside it knows where its defaults live or
/// what shape its section has.
mod auth {
    use serde::Deserialize;
    use std::time::Duration;

    /// Publish the module's defaults for `include package(...)` (E11).
    pub fn register(parser: hocon::Parser) -> hocon::Parser {
        parser.register_package(
            "github.com/acme/auth",
            "reference.conf",
            include_str!("auth/reference.conf"),
        )
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub struct Session {
        #[serde(with = "hocon::serde::duration")]
        pub lifetime: Duration,
        #[serde(with = "hocon::serde::bytes")]
        pub max_body: u64,
    }

    #[derive(Debug, Deserialize)]
    pub struct Db {
        pub host: String,
        pub port: u16,
    }

    #[derive(Debug, Deserialize)]
    pub struct Auth {
        pub session: Session,
        pub db: Db,
    }

    /// The module reads only its own subtree, through its own schema.
    pub fn load(config: &hocon::Config) -> Result<Auth, hocon::ConfigError> {
        config.get_as::<Auth>("auth")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The application: register each module's defaults, then parse its own file.
    let config = auth::register(hocon::Parser::new()).parse(include_str!("application.conf"))?;

    let auth = auth::load(&config)?;
    assert_eq!(
        auth.session.lifetime,
        std::time::Duration::from_secs(2 * 3600)
    ); // overridden
    assert_eq!(auth.session.max_body, 64 * 1024); // the module's default
    assert_eq!((auth.db.host.as_str(), auth.db.port), ("db.internal", 5432)); // wired
    println!("{:#?}", auth);
    Ok(())
}
