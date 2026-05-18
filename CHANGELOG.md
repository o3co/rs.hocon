# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **S13c — `${X[]}` / `${?X[]}` env-var list expansion** (Phase 6 #3g): substitution
  bodies now accept a literal `[]` suffix signalling env-var-list expansion. The lexer
  `parse_subst_body` recognises the `[]` suffix (E7: ASCII space/tab before `[` is
  tolerated); the resolver's new `resolve_env_list` helper scans `NAME_0`, `NAME_1`, …
  until the first absent key and returns an `Array` of strings. Empty-string elements
  are preserved (stop on absent key, not empty value). Required substitution with no
  `_0` element raises `ResolveError`; optional form drops the key.
  - **E6 compliance** (config-defined wins): when the substitution path resolves to a
    config value, the `[]` suffix is a no-op — env vars `NAME_*` are not consulted.
  - **E7 compliance** (whitespace before `[]`): `${X []}` and `${X\t[]}` parse
    identically to `${X[]}`.
  - **S13c.5 enforcement**: scalar env fallback (`NAME` without suffix) is suppressed
    when `list_suffix=true` — only `NAME_0`, `NAME_1`, … are consulted.
  - Conformance tests: `tests/env_var_list_test.rs` with fixtures ev01–ev11 from
    `xx.hocon/testdata/hocon/env-var-list/`.

### Changed

- **BREAKING (rare)**: `SubstPayload` gains a new public field `list_suffix: bool`
  and is now `#[non_exhaustive]`. `SubstPayload` IS publicly re-exported from
  the crate root (see `lib.rs`), so downstream crates that constructed it via
  struct literal or pattern-matched all fields exhaustively need to update.
  Migration: add `list_suffix: false` to existing struct literals (or use
  `Default` once it's added in a future release), and add `..` to exhaustive
  patterns. Most consumers should be unaffected — `SubstPayload` is primarily
  an internal pipeline value produced by the lexer and consumed by the resolver.

  `AstNode::Substitution` also gains `list_suffix: bool` and is now
  `#[non_exhaustive]`, but the `parser` module is `pub(crate)` so this is NOT
  a public-API change. (Internal callers in `structure_builder.rs` are updated
  in this same commit.)

  Rationale for taking `#[non_exhaustive]` in this release: future field
  additions on these types would otherwise each be breaking; installing the
  discipline now (in the same minor release that adds the first such field)
  amortizes the migration cost to a single update.

## [1.2.0] - 2026-05-18

### Changed

- **BREAKING (S8.6)**: `a = -foo`, `a = -bar`, `a = -` and other `-`-not-followed-by-digit inputs are now lex errors. Per HOCON.md L270–276, a leading `-` must begin a number literal (i.e. be followed by a digit). Previously these were silently accepted as unquoted strings (`"-foo"`, `"-"`). The same rule is applied to substitution paths (`${-foo}` rejected) and dotted key segments (`a.-foo = 1` rejected). Mitigation: quote the value (`a = "-foo"`). Note: this is intentionally stricter than Lightbend's reference implementation, which falls back to unquoted on number-parse failure. Digit-leading inputs (e.g. `123abc`, `01`, `1e+x`) are unaffected — rs.hocon's token model has no separate `Number` kind, so the resolved value continues to match Lightbend's value-concat output for the common cases (see `docs/spec-compliance.md` §S8.6 for the remaining gaps tracked under #63).
- Substitution body tokenization: `${...}` internals are now tokenized
  by a dedicated `parse_subst_body` in the lexer, matching Lightbend
  `PathParser` + `WhitespaceSaver` semantics. Quoted segments receive
  full JSON escape expansion; whitespace between two simple values is
  preserved as part of the segment text; whitespace around `DOT` or at
  the body edges is discarded.
- `SubstPlaceholder.segments` is now `Vec<Segment>` (text + source
  position) instead of `Vec<String>`. `AstNode::Substitution.segments`
  follows suit.
- Unified `TokenKind::Substitution` and `TokenKind::OptionalSubstitution`
  into a single `Substitution` kind; optionality lives in
  `SubstPayload.optional`.

### Fixed

- `${"a\nb"}` now decodes the `\n` escape to an actual newline in the
  segment text (previously kept literal backslash-n).
- Invalid escapes like `${"a\xb"}` are rejected at lex time with
  `invalid escape sequence`.
- `${"a" "b"}` produces a single segment `["a b"]` with whitespace
  preserved between simple values (previously rejected / mis-split).
- `${""}` resolves to the empty-string key correctly (closes #38).
- Path errors (`${}`, `${.foo}`, `${foo.}`, `${foo..bar}`) are detected
  at lex time with a specific error message.
- `${foo.}` trailing-dot error now reports position at the offending dot
  instead of at the `${` start.

## [1.1.0] - 2026-04-05

### Changed

- **Scalar internal representation**: `ScalarValue` changed from enum (`String`/`Int`/`Float`/`Bool`/`Null` variants) to struct `{raw: String, value_type: ScalarType}`. Scalars now store the original text and a type discriminant instead of converted Rust values. This eliminates type erasure (e.g., `0100` → `100`) and preserves original text.
- `get_string()` now returns `raw` for **all** scalar types (number, boolean, null), matching Lightbend behavior.
- Env var lookup uses raw dot-join instead of `segments_to_key` (no quoting), matching Lightbend behavior.

### Fixed

- `.33` (no leading zero) now correctly classified as string, not number — aligned with Lightbend reference implementation.
- `get_i64()` f64 fallback restricted to float-like literals only — prevents silent saturation on overflow for integer-like strings (e.g., `"9223372036854775808"`).
- `get_duration()` / `parse_duration()` reject negative values instead of wrapping via `as u64`.
- `get_duration()` guards against `Duration::from_secs_f64` panic on very large values.
- `get_bytes()` rejects bare float numbers that would silently round.
- Serde `parse_int_from_scalar`: removed dead code path, restricted f64 fallback to float-like literals.
- Quoted-key include relativization: `${"a.b".c}` inside included files now resolves correctly.
- `include file("path")` now resolves relative to the process working directory (or as absolute), not relative to the including file's directory, matching the HOCON spec.
- `tempfile` dev-dependency pinned to `<3.20` for MSRV 1.82 compatibility.

### Added

- `ScalarType` enum and `ScalarValue` struct exported from crate root.
- `#[non_exhaustive]` on `ScalarType` and `ScalarValue`.
- Substitution path segments: `SubstPlaceholder` uses `segments: Vec<String>` for correct quoted-key handling.

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
