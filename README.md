# o3co-hocon

[![Crates.io](https://img.shields.io/crates/v/o3co-hocon.svg)](https://crates.io/crates/o3co-hocon)
[![docs.rs](https://docs.rs/o3co-hocon/badge.svg)](https://docs.rs/o3co-hocon)
[![CI](https://github.com/o3co/rs.hocon/actions/workflows/test.yml/badge.svg)](https://github.com/o3co/rs.hocon/actions/workflows/test.yml)
[![License](https://img.shields.io/crates/l/o3co-hocon.svg)](LICENSE)

Full [Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md)-compliant
parser for Rust. Zero-copy lexer, recursive-descent parser, and a typed `Config` API
with optional Serde integration.

## Features

- Complete HOCON syntax: objects, arrays, comments, multi-line strings, unquoted strings
- Substitutions (`${foo}`, `${?foo}`) with cycle detection
- `include` directives (file, classpath, URL) with relative path resolution
- Object merging and array concatenation per spec
- String, array, and object value concatenation
- Duration and byte-size parsing (`10 seconds`, `512 MB`)
- Environment variable substitution (`${HOME}`)
- Dot-separated path expressions (`server.host`)
- Fallback configuration merging (`with_fallback`)
- Optional Serde deserialization support
- Passes Lightbend equivalence tests (equiv01 through equiv05)

## Installation

```sh
cargo add o3co-hocon
```

To enable Serde support:

```sh
cargo add o3co-hocon --features serde
```

## Quick Start

```rust
use hocon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = hocon::parse(r#"
        server {
            host = "localhost"
            port = 8080
        }
        database {
            url = "jdbc:postgresql://localhost/mydb"
            pool-size = 10
        }
    "#)?;

    let host = config.get_string("server.host")?;
    let port = config.get_i64("server.port")?;

    println!("Server: {}:{}", host, port);
    Ok(())
}
```

## API Reference

### Parsing

```rust
// Parse a HOCON string
let config = hocon::parse(input)?;

// Parse a HOCON file (resolves include directives relative to file location)
let config = hocon::parse_file("application.conf")?;

// Parse with custom environment variables
use std::collections::HashMap;
let env: HashMap<String, String> = HashMap::new();
let config = hocon::parse_with_env(input, &env)?;
let config = hocon::parse_file_with_env("application.conf", &env)?;
```

### Typed Getters

All typed getters return `Result<T, ConfigError>`. Paths use dot notation.

```rust
let host: String    = config.get_string("server.host")?;
let port: i64       = config.get_i64("server.port")?;
let rate: f64       = config.get_f64("rate")?;
let debug: bool     = config.get_bool("debug")?;        // also accepts "yes"/"no", "on"/"off"
let sub: Config     = config.get_config("database")?;    // sub-object as Config
let items: Vec<HoconValue> = config.get_list("items")?;
```

### Option Variants

Return `Option<T>` instead of `Result` -- return `None` for missing keys or type mismatches.

```rust
let host: Option<String> = config.get_string_option("server.host");
let port: Option<i64>    = config.get_i64_option("server.port");
let rate: Option<f64>    = config.get_f64_option("rate");
let debug: Option<bool>  = config.get_bool_option("debug");
```

### Duration and Byte-Size Values

```rust
use std::time::Duration;

// Supports: ns, us, ms, s/seconds, m/minutes, h/hours, d/days
let timeout: Duration = config.get_duration("server.timeout")?;

// Supports: B, KB, KiB, MB, MiB, GB, GiB, TB, TiB (and long forms)
let max_size: i64 = config.get_bytes("upload.max-size")?;
```

### Inspection

```rust
let exists: bool     = config.has("server.host");
let keys: Vec<&str>  = config.keys();           // top-level keys in insertion order
let raw: Option<&HoconValue> = config.get("server.host");
```

### Fallback Merge

```rust
// Receiver wins; fallback fills missing keys. Objects are deep-merged.
let merged = app_config.with_fallback(&defaults);
```

### Serde Deserialization

Requires the `serde` feature.

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

let config = hocon::parse(input)?;
let server: ServerConfig = config
    .get_config("server")?
    .deserialize()?;
```

## Error Types

| Type | When |
|------|------|
| `ParseError` | Syntax errors during lexing/parsing (includes line and column) |
| `ResolveError` | Substitution failures, cyclic references, missing required variables |
| `ConfigError` | Missing keys or type mismatches during value access |
| `DeserializeError` | Serde deserialization failures (with `serde` feature) |

## HOCON Examples

```hocon
# Comments start with // or #
server {
    host = "0.0.0.0"
    port = 8080
    timeout = 30 seconds
    max-upload = 512 MB
}

# Substitutions
app {
    name = "my-app"
    title = "Welcome to "${app.name}
}

# Array concatenation
base-tags = ["production"]
tags = ${base-tags} ["v2"]

# Include other files
include "defaults.conf"

# Unquoted strings
path = /usr/local/bin

# Multi-line strings
description = """
    This is a multi-line
    string value.
"""

# Object merging
defaults { color = "blue", size = 10 }
defaults { size = 20 }  # merges: color stays, size updated
```

## Spec Compliance

This library targets full compliance with the
[Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md).
The test suite includes the Lightbend equivalence tests (equiv01 through equiv05),
verifying correct handling of object merging, array concatenation, substitutions,
and all other spec-defined behaviors.

## Minimum Supported Rust Version

The MSRV is **1.82**.

## Related Projects

| Project | Language | Description |
|---------|----------|-------------|
| [go.hocon](https://github.com/o3co/go.hocon) | Go | HOCON parser for Go |
| [ts.hocon](https://github.com/o3co/ts.hocon) | TypeScript | HOCON parser for TypeScript/Node.js |
| [hocon2](https://github.com/o3co/hocon2) | Go | CLI tools to convert HOCON → JSON/YAML/TOML/Properties |

All implementations are full Lightbend HOCON spec compliant.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

## Attribution

Designed and built end-to-end with [Claude Code](https://claude.ai/claude-code).
