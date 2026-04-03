# hocon-parser — HOCON Parser for Rust

[![Crates.io](https://img.shields.io/crates/v/hocon-parser.svg)](https://crates.io/crates/hocon-parser)
[![docs.rs](https://docs.rs/hocon-parser/badge.svg)](https://docs.rs/hocon-parser)
[![CI](https://github.com/o3co/rs.hocon/actions/workflows/test.yml/badge.svg)](https://github.com/o3co/rs.hocon/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/o3co/rs.hocon/branch/main/graph/badge.svg)](https://codecov.io/gh/o3co/rs.hocon)
[![License](https://img.shields.io/crates/l/hocon-parser.svg)](LICENSE)

Full [Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md)-compliant
parser for Rust. Hand-written lexer, recursive-descent parser, and a typed `Config` API
with optional Serde integration.

[日本語](README.ja.md)

## Quick Start

### 1. Install

```sh
cargo add hocon-parser
```

To enable Serde support:

```sh
cargo add hocon-parser --features serde
```

### 2. Use

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

## Why HOCON?

| | `.env` | JSON | YAML | HOCON |
|---|---|---|---|---|
| Comments | No | No | Yes | Yes |
| Nesting | No | Yes | Yes | Yes |
| References / Substitution | No | No | No | Yes (`${var}`) |
| File inclusion | No | No | No | Yes (`include`) |
| Object merging | No | No | Anchors (fragile) | Yes (deep merge) |
| Optional values | No | No | No | Yes (`${?var}`) |
| Trailing commas | N/A | No | N/A | Yes |
| Unquoted strings | Yes | No | Yes | Yes |

HOCON gives you the readability of YAML, the structure of JSON, and features that neither has — substitutions, includes, and deep merge. If your config is more than a few flat key-value pairs, HOCON is worth considering.

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

## Performance

Measured with [Criterion](https://github.com/bheisler/criterion.rs). Each iteration includes parsing and a `get_string` lookup. Run `cargo bench` to reproduce.

| Scenario | ops/sec | Time per op |
|---|---|---|
| Small config (10 keys) | ~62,000 | ~16 µs |
| Medium config (100 keys) | ~19,000 | ~52 µs |
| Large config (1,000 keys) | ~2,400 | ~408 µs |
| 10 substitutions | ~37,000 | ~27 µs |
| 50 substitutions | ~12,000 | ~86 µs |
| 100 substitutions | ~6,400 | ~156 µs |
| Depth 5 nesting | ~58,000 | ~17 µs |
| Depth 10 nesting | ~50,000 | ~20 µs |
| Depth 20 nesting | ~39,000 | ~26 µs |

For typical application configs (loaded once at startup), the parsing cost is negligible — even a 1,000-key config parses in under 0.5 ms.

## Comparison

✅ Full support / ⚠️ Partial / ❌ Not supported

### HOCON Implementation

| Feature | rs.hocon | [hocon-rs](https://github.com/mockersf/hocon.rs) |
|---|:---:|:---:|
| Substitutions (`${path}`) | ✅ | ✅ |
| Optional substitutions (`${?path}`) | ✅ | ✅ |
| Include | ✅ | ✅ |
| `include required(file(...))` | ✅ | ❌ |
| Object/Array concatenation | ✅ | ✅ |
| Type coercion | ✅ | ⚠️ |
| Duration parsing | ✅ | ✅ |
| Byte size parsing | ✅ | ✅ |
| `+=` append | ✅ | ❌ |
| Serde deserialization | ✅ | ✅ |
| Env variable fallback | ✅ | ❌ |
| Circular include detection | ✅ | ❌ |

### Config Framework

| | rs.hocon | [config-rs](https://github.com/mehcode/config-rs) |
|---|:---:|:---:|
| **Formats** | | |
| HOCON | ✅ | ❌ |
| JSON | ✅ | ✅ |
| YAML | ❌ | ✅ |
| TOML | ❌ | ✅ |
| Env vars | ✅ (fallback) | ✅ |
| .properties | ✅ (via include) | ❌ |
| **Features** | | |
| Substitutions | ✅ | ❌ |
| File includes | ✅ | ❌ |
| Type coercion | ✅ | ✅ |
| Serde support | ✅ | ✅ |
| Watch/reload | ❌ | ❌ |
| Layered config | ❌ | ✅ |

## Spec Compliance

This library targets full compliance with the
[Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md).
The test suite includes the Lightbend equivalence tests (equiv01 through equiv05),
verifying correct handling of object merging, array concatenation, substitutions,
and all other spec-defined behaviors.

## Minimum Supported Rust Version

The MSRV is **1.82**.

## Related Projects

| Project | Language | Registry | Description |
|---------|----------|----------|-------------|
| [ts.hocon](https://github.com/o3co/ts.hocon) | TypeScript | [npm](https://www.npmjs.com/package/@o3co/ts.hocon) | HOCON parser for TypeScript/Node.js |
| [go.hocon](https://github.com/o3co/go.hocon) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/go.hocon) | HOCON parser for Go |
| [hocon2](https://github.com/o3co/hocon2) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/hocon2) | HOCON → JSON/YAML/TOML/Properties CLI |

All implementations are full Lightbend HOCON spec compliant.

## Best Practices

### Config Structure

- **Split by domain**: Separate configuration into logical units (`database.conf`, `server.conf`, `logging.conf`)
- **Use `include` for composition**: Compose a full config from domain-specific files
- **Avoid logic in config**: HOCON is for declarative data, not conditionals or computation

### Environment Variables

- **Minimize `${ENV}` usage**: Prefer `${?ENV}` (optional) with sensible defaults defined in the config itself
- **Never require env vars for local development**: Defaults should work out of the box
- **Document required env vars**: List them in your project's README or a `.env.example`

### Dev / Prod Separation

```text
config/
├── application.conf    # shared defaults
├── dev.conf            # include "application.conf" + dev overrides
└── prod.conf           # include "application.conf" + prod overrides
```

### Validation

- Always validate config at application startup, not at point-of-use
- Use schema validation (Zod for TypeScript, struct unmarshaling for Go, Serde for Rust) to catch errors early

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Deserialize)]
struct AppConfig {
    server: ServerConfig,
    debug: bool,
}

// requires the `serde` feature
let cfg: AppConfig = config.deserialize()?; // fails fast on startup
```

## Security Considerations

When parsing untrusted HOCON input, be aware of:

- **Path traversal in includes:** `include "../../../etc/passwd"` will resolve relative to `base_dir`. Validate include paths if parsing untrusted input.
- **Input size:** The parser has no built-in input size limit. For untrusted input, validate size before calling `parse()`.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

## Attribution

Designed and built end-to-end with [Claude Code](https://claude.ai/claude-code).
Reviewed by [GitHub Copilot](https://github.com/features/copilot) and [OpenAI Codex](https://openai.com/index/openai-codex/).
