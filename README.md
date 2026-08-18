# hocon-parser — HOCON Parser for Rust

[![Crates.io](https://img.shields.io/crates/v/hocon-parser.svg)](https://crates.io/crates/hocon-parser)
[![docs.rs](https://docs.rs/hocon-parser/badge.svg)](https://docs.rs/hocon-parser)
[![CI](https://github.com/o3co/rs.hocon/actions/workflows/test.yml/badge.svg)](https://github.com/o3co/rs.hocon/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/o3co/rs.hocon/branch/develop/graph/badge.svg)](https://codecov.io/gh/o3co/rs.hocon)
[![License](https://img.shields.io/crates/l/hocon-parser.svg)](LICENSE)

A [Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md)
parser for Rust. Hand-written lexer, recursive-descent parser, and a typed `Config` API
with optional Serde integration. See [Spec Compliance](#spec-compliance) for the current
conformance rate.

[日本語](README.ja.md)

**Library stance** — This library is a HOCON config loader. Its purpose is reading `.hocon` config files and providing typed access via the `Config` API (`get_string`, `get_i64`, `get_f64`, `get_bool`, `get_duration`, `get_bytes`). It is not a low-level parser API; internal types like `ScalarValue` may change between minor versions.

**Cross-language conformance** — This implementation is tested against shared expected-JSON fixtures from [o3co/xx.hocon](https://github.com/o3co/xx.hocon) alongside [ts.hocon](https://github.com/o3co/ts.hocon), [go.hocon](https://github.com/o3co/go.hocon), and [py.hocon](https://github.com/o3co/py.hocon), ensuring all four implementations meet the same Lightbend HOCON specification.

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

Or deserialize straight into your own types (with the `serde` feature):

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct App {
    server: Server,
}

#[derive(Deserialize)]
struct Server {
    host: String,
    port: u16,
}

// one step: text → T
let app: App = hocon::from_str(r#"
    server {
        host = "localhost"
        port = 8080
    }
"#)?;

// same, from a file
let app: App = hocon::from_file("app.conf")?;
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

HOCON isn't just a serialization format — it's a **config-injection language**. JSON, YAML, and TOML describe data structures and leave file layering, environment variables, and reference resolution to your code (Pydantic, Serde, Zod, etc.). HOCON bakes those into the spec itself: by the time your program reads the config, fallback files are merged and `${VAR}` references resolved into a single composed object. Conditional branching from "is this value present in this layer?" disappears at the format boundary.

On top of that, HOCON combines the readability of YAML with the structure of JSON — making it a strong fit for anything beyond flat key-value config.

## Features

- Complete HOCON syntax: objects, arrays, comments, multi-line strings, unquoted strings
- Substitutions (`${foo}`, `${?foo}`) with cycle detection
- `include "file.conf"` and `include file("file.conf")` directives with relative path resolution
- Object merging and array concatenation per spec
- String, array, and object value concatenation
- Duration and byte-size parsing (`10 seconds`, `512 MB`)
- Environment variable substitution (`${HOME}`)
- Dot-separated path expressions (`server.host`)
- Fallback configuration merging (`with_fallback`)
- Deferred resolution lifecycle: `parse_string_with_options` → `with_fallback` → `resolve()`
  per Lightbend `parseString` / `withFallback` / `resolve()` API (E12, v1.4.0)
- Optional Serde deserialization: one-step `hocon::from_str::<T>()` /
  `from_file::<T>()`, path-scoped `Config::get_as::<T>(path)`, and
  `Config::deserialize::<T>()`
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

### Deferred Resolution (v1.4.0)

Parse without resolving, add a runtime fallback, then resolve in a single pass:

```rust
use hocon::{parse_string_with_options, ParseOptions, ResolveOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = parse_string_with_options(
        r#"version = ${shortversion}-${CI_RUN_NUMBER}
           variables { shortversion = "1.2.3" }"#,
        ParseOptions::defaults().with_resolve_substitutions(false),
    )?;
    assert!(!cfg.is_resolved());

    let runtime = hocon::empty(None); // or from_map with runtime values
    let vars = cfg.get_config("variables")?;
    let resolved = cfg
        .with_fallback(&runtime)
        .with_fallback(&vars)
        .resolve(ResolveOptions::defaults())?;

    println!("{}", resolved.get_string("version")?); // e.g. "1.2.3-42"
    Ok(())
}
```

You can also use `resolve_with` to supply a resolved source for substitution lookup
without merging its keys into the result:

```rust
let resolved = cfg.resolve_with(&source_config, ResolveOptions::defaults())?;
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

// One step, text (or file) → T — like serde_json::from_str:
let server: ServerConfig = hocon::from_str("host = localhost, port = 8080")?;
let server: ServerConfig = hocon::from_file("server.conf")?;

// Path-scoped: deserialize any node (object, array, or scalar) at a path:
let config = hocon::parse(input)?;
let server: ServerConfig = config.get_as("server")?;
let ports: Vec<u16> = config.get_as("ports")?;

// Or the whole Config, e.g. after with_fallback / resolve:
let server: ServerConfig = config.deserialize()?;
```

## Error Types

| Type | When |
|------|------|
| `ParseError` | Syntax errors during lexing/parsing (includes line and column) |
| `ResolveError` | Substitution failures, cyclic references, missing required variables |
| `ConfigError` | Missing keys or type mismatches during value access |
| `ConfigError` (use `.is_not_resolved()` to detect "value not yet resolved") | Getter called on a path containing an unresolved substitution placeholder (v1.4.0) |
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

Conformance against the [Lightbend HOCON specification](https://github.com/lightbend/config/blob/main/HOCON.md) is tracked at item granularity in [`docs/spec-compliance.md`](docs/spec-compliance.md), which is the source these rates are computed from — `tests/docs.rs` recomputes them and fails the build if this table drifts. See [`xx.hocon/docs/compliance-matrix.md`](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md) for the cross-implementation roll-up.

| Metric                                | Status        |
| ------------------------------------- | ------------- |
| Spec total (incl. out-of-scope)       | **92.9%**     |
| In-scope only                         | **100.0%**    |
| Lightbend `equiv01`–`equiv05` suite   | 5/5 passing   |

## Minimum Supported Rust Version

The MSRV is **1.82**, for every feature combination including all five
adapters. CI runs the full test suite at 1.82 with `--all-features`, so this is
verified rather than asserted.

## Related Projects

| Project | Language | Registry | Description |
|---------|----------|----------|-------------|
| [ts.hocon](https://github.com/o3co/ts.hocon) | TypeScript | [npm](https://www.npmjs.com/package/@o3co/ts.hocon) | HOCON parser for TypeScript/Node.js |
| [go.hocon](https://github.com/o3co/go.hocon) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/go.hocon) | HOCON parser for Go |
| [py.hocon](https://github.com/o3co/py.hocon) | Python | [PyPI](https://pypi.org/project/hocon-parser/) | HOCON parser for Python |
| [hocon2](https://github.com/o3co/hocon2) | Go | [pkg.go.dev](https://pkg.go.dev/github.com/o3co/hocon2) | HOCON → JSON/YAML/TOML/Properties CLI |

The four parser implementations ([ts.hocon](https://github.com/o3co/ts.hocon), [rs.hocon](https://github.com/o3co/rs.hocon), [go.hocon](https://github.com/o3co/go.hocon), [py.hocon](https://github.com/o3co/py.hocon)) are all tracked against the same Lightbend HOCON spec — see the [cross-impl roll-up](https://github.com/o3co/xx.hocon/blob/main/docs/compliance-matrix.md) for per-impl conformance rates.

## Best Practices

### Config Structure

- **Split by domain**: Separate configuration into logical units (`database.conf`, `server.conf`, `logging.conf`)
- **Use `include` for composition**: Compose a full config from domain-specific files
- **Avoid logic in config**: HOCON is for declarative data, not conditionals or computation

### Environment Variables

- **Minimize `${ENV}` usage**: Prefer `${?ENV}` (optional) with sensible defaults defined in the config itself
- **Never require env vars for local development**: Defaults should work out of the box
- **Document required env vars**: List them in your project's README or a `.env.example`

**Non-UTF-8 entries never abort a parse.** An environment entry whose name or
value is not valid UTF-8 is treated as absent everywhere this crate resolves
`${...}` — `parse`, `parse_file`, `Parser::parse`, `Parser::parse_file`,
`Config::resolve` and `Config::resolve_with` (the latter two with
`use_system_environment`). A `${VAR}` naming such an entry behaves exactly as if
the variable were **unset**: `${?VAR}` is undefined and `${VAR}` is the usual
"unresolved substitution" error. It never resolves to lossily-converted text, so
a non-UTF-8 byte sequence cannot reach your config as mangled data.

This cannot change the meaning of a config that used to work: `std::env::vars()`
panicked on the first undecodable entry regardless of which variables the
document named, so the previous behaviour for anyone affected was a crash, not a
successful parse.

**A bulk mount is the exception, and errors instead** — see
[Format adapters](#format-adapters).

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

## Format adapters

Config files that belong to *other* programs can be mounted as HOCON, so a
`${...}` in your document can reach into them:

```rust
use hocon::adapters::env;

// APP_DB__HOST=db.internal  ->  db.host
let base = env::load(env::Options { prefix: "APP_".into(), ..Default::default() })?;

let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
let cfg = hocon::parse_string_with_options(src, opts)?;
let merged = cfg.with_fallback(&base).resolve(hocon::ResolveOptions::defaults())?;
```

Deferring resolution matters: the plain `parse` resolves as it goes, so a
`${...}` aimed at the fallback would fail before the fallback is attached.

| Feature | Adapter | Extra dependency |
| --- | --- | --- |
| `adapters-properties` | `java.util.Properties`, sharing the `include` syntax layer | — |
| `adapters-env` | Bulk-mounts a prefixed namespace; also reads `.env` | — |
| `adapters-json5` | JSON5 documents (JSON5 1.0.0, hand-rolled scanner) | — |
| `adapters-jsonc` | JSON with comments and trailing commas | `serde_json` |
| `adapters-toml` | TOML documents | `toml` |
| `adapters-yaml` | YAML documents | `yaml-rust2` |

`adapters` enables all six. Every one is opt-in, so the default build still
depends on `indexmap` alone. Plain JSON needs no adapter — HOCON is a JSON
superset, so `hocon::parse` accepts it as it stands.

```sh
cargo add hocon-parser --features adapters        # all six
cargo add hocon-parser --features adapters-env    # or just the one you need
```

Foreign data stays data: a `${a.b}` in a mounted value is literal text, never a
reference, because the file belongs to a program that never agreed to HOCON's
syntax.

### How env variable names become paths

**`__` is the only thing that creates hierarchy.** A single `_` stays part of
the segment, and a literal `.` in a variable name is key *text* — not a
separator:

```text
APP_DB__MAX_CONN=10   ->  db.max_conn      (nested: "db" contains "max_conn")
APP_FOO.BAR=flat      ->  "foo.bar"        (one top-level key that contains a dot)
```

The second form is a single key, so it is read with a quoted path —
`cfg.get_string("\"foo.bar\"")` — while `APP_FOO__BAR` is read as
`cfg.get_string("foo.bar")`. They are distinct paths and can be set at the same
time without conflicting. Segments are lowercased after mapping.

Two variables that *do* map to the same path (`APP_A__B` and `APP_a__b`) are an
error rather than a silent last-wins, because environment iteration order is not
deterministic. A `.env` file has a definite line order, so there the later line
wins as usual.

Unlike `${VAR}`, `adapters::env::load` **errors** if an entry matching the mount
prefix has a name or value that is not valid UTF-8. A bulk mount is a request
for a whole namespace, so silently omitting one key would hand back a subtree
that looks complete while an operator's setting is missing — and a stale config
default would then win with no signal. Entries outside the prefix are ignored
whether they decode or not, so an unrelated undecodable variable can never fail
a mount.

### JSONC comments separate tokens

A comment is replaced by whitespace, never removed outright, so it can never
splice its neighbors together:

```jsonc
{"a": 1/*x*/2}   // syntax error — NOT the number 12
```

### JSON5 is scanned, not preprocessed

JSON5 changes the token grammar itself — unquoted identifier keys, single
quotes, hex integers, line continuations — so `adapters::json5` is a
hand-rolled scanner and recursive-descent parser rather than a preprocessor in
front of a JSON decoder, and needs no extra dependency. The accepted grammar
is JSON5 1.0.0 as defined by the reference implementation (the json5 npm
package). Where the mapping spec is stricter than JSON5, the spec wins:
integers must fit in `i64` (F0.5), `Infinity`/`NaN` are errors (F0.6), a lone
`\uXXXX` surrogate is an error (F3.5), and duplicate keys follow HOCON
semantics — objects merge, otherwise the later value wins (F0.7).

### YAML scalar resolution is the library's answer

For YAML, scalar resolution belongs to the library, not to this crate: whether
`010` is 8 or 10 is `yaml-rust2`'s answer. `adapters::yaml::from_value` takes an
already-decoded tree, so a caller who needs a different library or schema
decodes it themselves and hands the result over.

## Known Limitations

- **`include url(...)`** is not supported. Fetching remote configuration is outside the scope of this parser. Use your application's HTTP client to fetch the content, then pass it to `parse()`.
- **`include classpath(...)`** is not supported. This is a JVM-specific include form with no equivalent outside Java runtimes.
- **No watch/reload** — the library parses config at load time. For live-reloading, call `parse()` / `parse_file()` again on change.
- **No streaming parser** — the entire input is loaded into memory.

For full API documentation, see [docs.rs](https://docs.rs/hocon-parser) (available after crate publication).

## Security Considerations

When parsing untrusted HOCON input, be aware of:

- **Path traversal in includes:** `include "../../../etc/passwd"` will resolve relative to `base_dir`. Validate include paths if parsing untrusted input.
- **Input size:** The parser has no built-in input size limit. For untrusted input, validate size before calling `parse()`.
- **Document nesting depth:** limited to 128 levels of objects and arrays,
  enforced by `parse` and by `from_map`. Unlike the sibling implementations,
  which catch their runtime's own recursion error, exhausting the stack in Rust
  is `SIGABRT` — it takes the caller's process with it and no `catch_unwind`
  contains it, so the limit has to come before the overflow rather than after.
  128 is `serde_json`'s limit, which the `jsonc` adapter has always enforced,
  and it holds on the 2 MiB stack a spawned thread gets.
- **Mapped path depth:** an environment variable's `__` segments and a
  `.properties` dotted key are limited to 64 segments, matching ts.hocon and
  py.hocon.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

## Attribution

Designed and built end-to-end with [Claude Code](https://claude.ai/claude-code).
Reviewed by [GitHub Copilot](https://github.com/features/copilot) and [OpenAI Codex](https://openai.com/index/openai-codex/).
