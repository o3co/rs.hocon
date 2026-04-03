# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-04-04

### Added

- `HoconError` unified error type for parse functions (preserves Parse, Resolve, Io variants)
- `#[non_exhaustive]` on all public types for semver safety
- `DeserializeError` re-exported from crate root
- `include required()` and `include required(file())` directives
- Circular include detection with include stack
- Performance benchmarks in README
- Library comparison tables in README (vs hocon-rs, vs config-rs)
- Security Considerations section in README
- Known Limitations section in README
- `Debug`, `Clone`, `PartialEq` derives on `Config`

### Fixed

- Include probe order: `.properties → .json → .conf` (`.conf` wins)
- Error on unknown escape sequences in quoted strings
- Unquoted string forbidden characters aligned with HOCON spec (`?!@*&^\`)

### Changed

- **Breaking:** Crate renamed from `o3co-hocon` to `hocon-parser`
- **Breaking:** Parse functions return `Result<Config, HoconError>` instead of `Result<Config, ParseError>`
- **Breaking:** Serde internal types (`HoconDeserializer`, `HoconMapAccess`, `HoconSeqAccess`) hidden from public API
- **Breaking:** `#[non_exhaustive]` added to `HoconValue`, `ScalarValue`, `ParseError`, `ResolveError`, `ConfigError`, `DeserializeError`
- Cross-language spec alignment with ts.hocon and go.hocon
- README: "zero-copy lexer" corrected to "hand-written lexer"

## [0.1.2] - 2026-03-30

### Fixed

- Use official Apache 2.0 LICENSE text (was incorrectly AI-generated)

## [0.1.1] - 2026-03-30

### Fixed

- Set MSRV to 1.82 (required by indexmap 2)
- Apply `cargo fmt` formatting
- Fix Windows line ending issue in Lightbend test suite
- Add CI workflows for test (multi-OS, MSRV) and lint (clippy, fmt)
- Rename crate to `o3co-hocon` (lib name remains `hocon`)

## [0.1.0] - 2026-03-30

### Added

- Full HOCON lexer and recursive-descent parser
- Substitution resolution (`${foo}`, `${?foo}`) with cycle detection
- `include` directive support (file, classpath, URL) with relative path resolution
- Object merging and array concatenation per the Lightbend HOCON specification
- String, array, and object value concatenation
- `Config` API with typed getters: `get_string`, `get_i64`, `get_f64`, `get_bool`, `get_config`, `get_list`
- `Option` variants for all typed getters
- Duration parsing (`get_duration`) supporting ns, us, ms, s, m, h, d units
- Byte-size parsing (`get_bytes`) supporting B, KB, KiB, MB, MiB, GB, GiB, TB, TiB
- Dot-separated path expressions for nested value access
- `has()` for key existence checks and `keys()` for top-level key listing
- `with_fallback()` for deep-merging configurations
- Environment variable substitution
- Optional Serde deserialization support (`serde` feature flag)
- Lightbend equivalence tests (equiv01 through equiv05)

[1.0.0]: https://github.com/o3co/rs.hocon/compare/v0.1.5...v1.0.0
[0.1.2]: https://github.com/o3co/rs.hocon/releases/tag/v0.1.2
[0.1.1]: https://github.com/o3co/rs.hocon/releases/tag/v0.1.1
[0.1.0]: https://github.com/o3co/rs.hocon/releases/tag/v0.1.0
