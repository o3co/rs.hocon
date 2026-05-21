# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed — S17.6 spec compliance

- **`get_string()` on a null-typed scalar now errors instead of returning `Ok("null")`** ([#80](https://github.com/o3co/rs.hocon/issues/80)). Per HOCON spec L1252, "if the application asks for a specific type and finds null instead, that should usually result in an error" — including `String`. Previously `get_i64` and `get_bool` on null correctly errored but `get_string` returned the raw `"null"` literal. The fix adds a `ScalarType::Null` guard at the top of `get_string`, matching the existing behaviour of the other typed getters.

### Fixed — S13.9 spec compliance

- **`null` in config blocks env-var fallback for optional substitutions** ([#74](https://github.com/o3co/rs.hocon/issues/74)). Per HOCON spec L618, "if the same variable is set in the config and in the environment, the config value wins"; for an optional substitution `${?key}` whose config value is null, the null is treated as "not present" and the parent field is dropped. Previously `rs.hocon` blocked the env-var fallback but still yielded `Scalar(null)`, so the field remained present with a null value. Required substitutions are unchanged — `${HOME}` against `HOME = null` still yields the explicit null scalar (the getter layer applies S17.6 coercion).

## [1.4.1] - 2026-05-22

Cross-impl bugfix release: addresses [go.hocon#105](https://github.com/o3co/go.hocon/issues/105) (cgordon-reported Lightbend divergence on empty/comment-only includes) at the rs.hocon layer, and pins go.hocon#106 (include-ordering / self-ref-through-include) which already worked correctly here. Pure include-path behaviour; no public API changes; safe drop-in upgrade from v1.4.0.

### Tests

- **Cross-impl regression tests for include ordering ([go.hocon#106](https://github.com/o3co/go.hocon/issues/106))**. Pin Lightbend-equivalent semantics for `include` directives — scalar override, parent-after-include, self-referential append through include, both-object deep-merge, nested-include scope isolation, and sequential includes — so the existing correct behaviour does not regress when the merge logic is touched. No production-code change; `rs.hocon`'s `deep_merge_res_obj_into` already implements src-wins + prior-capture.

### Changed — include path

- **Empty / comment-only / whitespace-only included files contribute an empty config** ([go.hocon#105](https://github.com/o3co/go.hocon/issues/105), Lightbend compatibility). Previously, `include "empty.conf"` (or comment-only / whitespace-only / BOM-only content) errored with `empty file is not a valid HOCON document (HOCON.md L130)`. This blocked the common optional-override-file pattern. The carve-out is **narrow** — applies only to the file-include code path in `load_file_include`; top-level parses (`parse("")`, `parse_file` on a top-level empty file) and E11 `include package(...)` are unchanged. Cross-impl with [go.hocon PR #110](https://github.com/o3co/go.hocon/pull/110) and [ts.hocon PR #122](https://github.com/o3co/ts.hocon/pull/122).

## [1.4.0] - 2026-05-21

### Added — E12 deferred substitution resolution (external request via [go.hocon#99](https://github.com/o3co/go.hocon/issues/99))

This release adds the Lightbend-aligned `parse_string_with_options` →
`with_fallback` → `resolve()` lifecycle. Existing `parse` / `parse_file`
behaviour is unchanged (still parse-and-resolve in one call); the new API
surface is purely additive. Requested by [@cgordon](https://github.com/cgordon) (see [go.hocon#99](https://github.com/o3co/go.hocon/issues/99) — the cross-impl ask landed in the go.hocon issue tracker; ts.hocon/rs.hocon PRs numbered 99 are unrelated CI PRs).

**New entry points:**
- `parse_string_with_options(input, ParseOptions)` and
  `parse_file_with_options(path, ParseOptions)` — `ParseOptions::defaults().with_resolve_substitutions(false)`
  produces an unresolved `Config` (`is_resolved()` is `false` when `${...}` is present).
- `from_map(serde_json::Map, origin) -> Result<Config, ConfigError>` (**serde feature**) —
  construct a resolved `Config` from a `serde_json` map.
  Lightbend `ConfigValueFactory.fromMap` parallel.
- `empty(origin) -> Config` — always-resolved empty `Config`.
  Lightbend `ConfigFactory.empty` parallel.

**New methods on `Config`:**
- `resolve(ResolveOptions) -> Result<Config, HoconError>` — phase-2 substitution
  resolution over the whole merged fallback stack. Idempotent on already-resolved configs.
- `resolve_with(source: &Config, ResolveOptions) -> Result<Config, HoconError>` —
  resolves receiver using source for substitution lookup. Source keys are NOT merged
  into the result. Precondition: source must be resolved.
- `is_resolved() -> bool` — whole-config resolution state per E12 decision 11.
- `with_fallback(&Config) -> Config` — now accepts unresolved operands; preserves
  substitution placeholders into the merged tree.  Receiver-wins semantics unchanged.

**New types:**
- `ParseOptions` — builder via `ParseOptions::defaults()` and `with_resolve_substitutions(bool)`,
  `with_origin_description(String)`. `ParseOptions` struct literal is **not** a valid
  invocation (documented; `defaults()` enforces Lightbend default of `true`).
- `ResolveOptions` — builder via `ResolveOptions::defaults()` and
  `with_use_system_environment(bool)`, `with_allow_unresolved(bool)`.

**New errors:**
- `NotResolvedError` — returned (wrapped in `HoconError::NotResolved`) when a getter
  is called on a path whose value contains an unresolved substitution placeholder.
  Per E12 decision 12.

**Cross-spec amendments** (no behavioural change for callers using the existing fused API):
- S13a × WithFallback: self-reference lookback (`${?a}` / `${a}`) walks across fallback
  layers.  Receiver `a = ${?a} extra` with fallback `a = base` resolves to `"base extra"`.
- S10 × AllowUnresolved: type-incompatible concat errors surface even under
  `allow_unresolved = true`; only missing-value errors are deferred.
- Optional `${?x}${?y}` where all operands are undefined → field omitted from result
  (HOCON.md §Substitutions L626–L645 concat materialisation rule).
- Deferred concat placeholder survives under `allow_unresolved=true` when all operands
  are unresolved mandatory substitutions; getter on that path raises `NotResolved`.

**Spec source:** [xx.hocon#37](https://github.com/o3co/xx.hocon/issues/37) /
E12 in `docs/extra-spec-conventions.md`.

### Added — E11 `include package("id", "file")` qualifier (feature-gated, default off)

New optional Cargo feature `include-package` (no new dependencies — uses `std` only)
enables the `include package(...)` syntax per xx.hocon cross-impl convention E11.
Spaced form `include package ("id", "file")` is also supported for consistency with
the existing `file(...)` qualifier.

Public API additions (only compiled when `features = ["include-package"]`):

- **`hocon::Parser`** — new public struct with consuming builder API:
  - `Parser::new() -> Self`
  - `Parser::register_package(self, identifier, file, content) -> Self`
  - `Parser::parse(self, input: &str) -> Result<Config, HoconError>`
  - `Parser::parse_file(self, path: impl AsRef<Path>) -> Result<Config, HoconError>`
- **Cascade convention**: downstream packages expose `pub fn register(parser: Parser) -> Parser`
  so callers can chain registrations: `pkg_b::register(pkg_a::register(Parser::new())).parse(…)`.
- **`AstNode::PackageInclude`** (internal `pub(crate)` variant) — not public API.
- **`IncludeKey::Package`** variant on the resolver's internal cycle-detection enum.

Behaviour:

- Registry miss is always a `HoconError` (required semantics apply unconditionally per E11 decision 7).
- Empty registered content returns an empty merge object (not a parse error — E11 carve-out).
- Circular package includes are detected and rejected (`ResolveError`).
- File argument is validated post-unescaping: non-empty, forward-slash separators, no absolute path.
- Identifier and file lookups are case-sensitive (E11 decision 5).
- Panic on duplicate `(identifier, file)` registration with different content; idempotent re-registration of byte-identical content is allowed.

### Changed

- **CI: content-addressable testdata cache** (closes [#101](https://github.com/o3co/rs.hocon/issues/101)). `.github/workflows/test.yml` and `.github/workflows/publish.yml` previously used `actions/cache@v5` with `key: xx-hocon-expected-${{ hashFiles('.xx-hocon-version') }}`. The hash evaluated BEFORE the cache restore step ran, but `.xx-hocon-version` is gitignored and absent on fresh checkouts — so the key collapsed to a constant and cache entries shared the same slot. Split into `actions/cache/restore@v5` (matches via `restore-keys`) + `actions/cache/save@v5` (writes with the post-fetch hash, gated on `make testdata` success). No production code touched.

[Unreleased]: https://github.com/o3co/rs.hocon/compare/v1.4.1...HEAD
[1.4.1]: https://github.com/o3co/rs.hocon/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/o3co/rs.hocon/compare/v1.3.0...v1.4.0

## [1.3.0] - 2026-05-21

v1.3 is a spec-compliance bugfix release. The implementation has been corrected to match the HOCON spec and Lightbend typesafe-config reference behavior across several previously-divergent areas (E8 value-position lexing + leading-zero canonicalization, single-letter byte units, `include` key reservation, concat type-checking, empty-file rejection, `.properties` object-wins, duration/bytes default unit, S13c env-var list). The spec did not change; the parser was simply wrong in places.

A subset of these fixes change observable runtime behavior. The most likely user-visible change is **S21.4** — single-letter byte units (`K`/`M`/`G`/`T`/`P`/`E`) now map to powers of two instead of SI decimal (`1K` was 1,000; now 1,024 — per HOCON.md L1385 java `-Xmx` convention, confirmed against Lightbend 1.4.3). If your `.conf` files use single-letter units and you rely on the numeric result, audit `get_bytes` call sites. Multi-letter forms (`KB`/`MB`/`GB`/`TB`) remain SI decimal and are unchanged. Other fixes have narrow practical impact — read `### Changed` / `### Fixed` below if your CI fails to upgrade cleanly. We elected MINOR (not MAJOR) because no API or architectural changes occurred; v2.0 is reserved for parser/lexer rewrites or similar structural shifts.

### Changed

- **E8 amendment — Lightbend reading of HOCON.md L270-276** ([xx.hocon#31](https://github.com/o3co/xx.hocon/issues/31), [xx.hocon#32](https://github.com/o3co/xx.hocon/pull/32) commit `dd102e8`).
  xx.hocon's extra-spec-conventions E8 was rewritten to adopt Lightbend's pragmatic reading of HOCON.md L270-276: "begin" = **value-position begin** (first component of a concatenation), not token-position begin at any lexer offset. rs.hocon retracts the v1.2.0 strict-spec posture (see the v1.2.0 retraction note below) and now matches:

  - **Reverted BREAKING from v1.2.0** — `a = -foo`, `a = -bar`, `a = -` now lex as unquoted strings (`{"a":"-foo"}` / `{"a":"-"}`), matching Lightbend. The v1.2.0 reject was correct for the strict-spec reading at the time but is superseded by the E8 amendment. RFC 8259's JSON-number grammar requires a digit after `-`, so bare `-` / `-foo` fall outside L270's disallow scope.
  - **Concat-continuation now accepted** — `b = ${a}-bar` (and the symmetric `${a}.bar` / `${a}1bar` / `"foo"-bar` cases) resolves to the expected unquoted concat (e.g. `"foo-bar"`). Previously rejected by the strict `-` reject at the lexer's unquoted-start branch. Driven by external issue [xx.hocon#31](https://github.com/o3co/xx.hocon/issues/31) — first issue from outside o3co (@cgordon).
  - **F3 BREAKING** — `a = 01` now resolves to number `1` with normalized `ScalarValue.raw = "1"`. Pre-E8, rs.hocon stored `raw = "01"` (the `f64::parse` path preserved the original text); JSON serialization already produced `1` via the i64-first serializer, but `get_string("a")` returned `"01"`. Post-E8, `parse_scalar_value` does `i64::parse` first and stores the canonical string form (`01` → `"1"`, `-0` → `"0"`), matching Lightbend's `parseLong` behavior. **Migration**: callers using `get_string("a")` on numeric values will now see the canonical decimal form, not the original input text. Callers using `get_i64`/`get_f64`/JSON serialization are unaffected.
  - **Correctness fix — `-inf` / `-nan` no longer classified as numbers** (Codex review on [#98](https://github.com/o3co/rs.hocon/pull/98)). Pre-E8 (and pre-PR), `parse_scalar_value` accepted any token starting with `-` that `f64::parse` could parse — including Rust-specific `-inf` / `-nan` that Lightbend's `parseDouble` rejects. The new "JSON-number-shaped" gate (`-` must be followed by a digit) keeps these on the string path, matching Lightbend. Not a separate BREAKING beyond the E8 changes above — it is a side effect of tightening the number-coercion entry rule.
  - `+` rejection retained in both value-start and concat-continuation positions (HOCON `+=` operator reservation) — `+` is excluded from `is_unquoted_start`, so it cannot open a value or extend a prior token in concat position.
  - Path-element strict checks preserved (out of E8 scope): `parse_subst_body`'s segment-start `-` check and `parse_key`'s per-segment check — these police path-element composition, not value-position unquoted strings. Tests `${-foo}` and `a.-foo = 1` still throw `ParseError`.
  - Known gap retained as `#[should_panic]` tripwire: us15 `a = 1e+x` (Lightbend errors on `+` mid-token at its value-parser layer; rs.hocon currently accepts `+` mid-unquoted run except when followed by `=`).

- **BREAKING (S21.4)**: Single-letter byte abbreviations `K`/`k`/`M`/`m`/`G`/`g`/`T`/`t`/`P`/`p`/`E`/`e`
  now map to **powers of two** (binary) instead of SI decimal, per HOCON.md L1385 java -Xmx convention.

  | Unit | Old value (SI decimal) | New value (binary) |
  | ---- | ---------------------- | ------------------- |
  | `K`/`k` | 1,000 | 1,024 |
  | `M`/`m` | 1,000,000 | 1,048,576 |
  | `G`/`g` | 1,000,000,000 | 1,073,741,824 |
  | `T`/`t` | 1,000,000,000,000 | 1,099,511,627,776 |
  | `P`/`p` | *(not supported)* | 1,125,899,906,842,624 |
  | `E`/`e` | *(not supported)* | 1,152,921,504,606,846,976 |

  **Migration**: callers that expected SI decimal semantics for single-letter units must switch
  to multi-letter forms (`KB` / `MB` / `GB` / `TB` remain SI decimal and are unchanged).
  Example: `1K` was 1,000 bytes; it is now 1,024 bytes. `1024K` was 1,024,000; it is now 1,048,576.

  Background: this corrects a mis-classification — the prior ✅ for S21.4 was based on the
  `get_bytes_no_space` test which exercised `"512MB"` (multi-letter), never single-letter `K`/`M`/`G`/`T`.
  The source comment citing "L1344 as SI decimal short forms" was wrong; HOCON.md L1385 normatively
  says single-letter → powers of two, confirmed by Lightbend typesafe-config 1.4.3.

### Fixed

- **`parse_duration` overflow guards** (closes #95, non-breaking):
  Integer path now uses `checked_mul` on `u64` instead of `(n as f64 * unit) as u64`
  which silently saturated for large `n` values. Fractional path now checks the
  product against `2f64.powi(64)` (the exact f64 value of 2^64) before the
  `as u64` cast — `(n as f64 * unit) as u64` previously rounded up `u64::MAX` to
  `2^64` on cast, masking overflow. Inputs like `"9223372036854775807 weeks"` and
  `"1e30 d"` now correctly return `None` instead of saturating to `Duration::from_nanos(u64::MAX)`.
  Same pattern as the cluster #3h fractional byte overflow fix in `parse_bytes`.

  Additionally, the integer fast-path now parses via `i128` to range-check both
  negatives (rejected per rs's unsigned-`Duration` limitation) AND values up to
  `u64::MAX` — previously the upper half of the representable nanos range was
  rejected as "parse error" rather than overflow.

- **S3.1 — Empty file is invalid** (Phase 6 #3h):
  `parse` and `parse_file` (and their `_with_env` variants) now return a `ParseError` for
  empty documents — including empty strings, whitespace-only, newlines-only, comment-only,
  BOM-only, and mixed whitespace+comment inputs — per HOCON.md L130 ("empty files are invalid
  documents"). Explicit empty objects (`{}`) and documents with at least one field are unaffected.

- **S23.4 — Properties dotted-key conflict: object wins** (Phase 6 #3h, non-breaking):
  When a `.properties` file contains conflicting keys (e.g. `a=hello` and `a.b=world`),
  the resolved value is now deterministically `{a: {b: "world"}}` (object wins, scalar discarded)
  per HOCON.md L1485.

  Two bugs in `set_nested` (`src/properties.rs`) are corrected:
  - **Leaf overwrite bug**: `a=hello` after `a.b=world` was being silently overwritten by the
    scalar, reversing the object-wins rule.
  - **Non-leaf scalar stranding bug**: when `a=hello` appeared before `a.b=world`, the
    `if let HoconValue::Object(inner) = entry` pattern failed silently on the scalar, causing
    `a.b` to be dropped on the floor (silent data-loss).

  Additionally, property keys are now **processed in sorted order** so the conflict direction
  is deterministic regardless of input line order (mirrors go.hocon's `sort.Strings(keys)`
  and the requirement in HOCON.md L1476-1479).

  Background: this corrects a mis-classification — the prior ✅ for S23.4 was based on
  `converts_to_hocon_value` which exercised `"a.b=1\nc=hello"` (no conflict path).

### Added

- **`Period` struct** (Phase 6 #3d — S18 review fix):
  New public `Period { years: i32, months: i32, days: i32 }` struct, marked `#[non_exhaustive]`
  so future fields can be added without a breaking change. Re-exported from the crate root as
  `hocon::Period`. Constructed via `Period::new(years, months, days)`.

- **`get_period` / `get_period_option` accessors** (Phase 6 #3d):
  New methods on `Config` for reading HOCON period values. Returns `Period` (no `chrono`
  dependency). Supported units: `d`/`day`/`days` (default), `w`/`week`/`weeks` (× 7 days),
  `m`/`mo`/`month`/`months`, `y`/`year`/`years`.
  Negative periods are permitted (signed `i32` fields, matching Lightbend).

### Fixed

- **S18.4 — string value with no unit → family default unit** (Phase 6 #3d):
  All three unit families now correctly interpret a bare number string as the family default:
  duration → milliseconds, period → days, bytes → bytes (HOCON.md L1290 / L1301 / L1321 / L1341).
  Previously `parse_duration` returned `None` for no-unit strings, causing `get_duration` to error.

  Changes:
  - `parse_duration`: added `""` arm (ms default); switched `.trim()` → HOCON_WS trim;
    added integer pre-classification regex `[+-]?[0-9]+` to match Lightbend `Long.parseLong`
    vs `Double.parseDouble` per-family split.
  - `parse_bytes`: switched `.trim()` → HOCON_WS trim; changed `.round()` → `as i64`
    truncation to match Lightbend `BigDecimal.toBigInteger()` semantics.
  - `get_bytes`: added negative-accessor rejection on both the string path and the bare-number
    path — byte sizes must be non-negative regardless of source
    (Lightbend `getBytesBigInteger` positive-only invariant; ub04).
  - `parse_period` (new): integer-only (fractional rejected per Lightbend `Integer.parseInt`);
    default unit days; units as above.
  - `is_hocon_whitespace` in `src/lexer.rs` promoted to `pub(crate)` for reuse.

  **rs-specific limitation** — `get_duration` returns `Err` for negative duration strings
  (e.g. `"-500"`). `std::time::Duration` is unsigned, whereas Lightbend's `java.time.Duration`
  is signed. The `ud06` conformance test in `tests/units_default_test.rs` asserts `is_err()`
  with a comment documenting this divergence.

- **S12.5 — `include` reserved at start of key path** (Phase 6 #3e):
  Unquoted `include` at the start of a key path expression (including the dotted form
  `include.foo = 1`) is now a parse error per HOCON.md L570. The bare forms
  (`include = 1`, `include : 1`, `include += [1]`, `include { ... }`) were already
  rejected via the include-statement branch; this fix adds the dotted case.
  Quoted `"include"` and non-initial `foo.include` are unaffected.
  Closes #71.

- **S10.4/S10.13/S10.19 — concat type-check tightening** (Phase 6 #3b):
  `join_pair` in the substitution resolver now returns `ResolveError` for every
  spec-disallowed value-concatenation type pair instead of silently coercing.
  Closes #65, #67, #68.
  - **S10.4** (`array + object` / `object + array`): after the S15 numeric-object-to-array
    bridge attempt returns `None`, the pair is now rejected as a type error per HOCON L385.
    Previously, the unconverted object was pushed into the array.
  - **S10.13** (`array + scalar` / `scalar + array` / `scalar + object` / `object + scalar`):
    all four pairs now raise `ResolveError` per HOCON L373. The go.hocon-style
    "append scalar to array" path is removed from rs.hocon.
  - **S10.19** (substitution-resolved object/array mixed with the other structured type):
    handled by the same `join_pair` fix — `joinPair` operates on resolved values, so
    the substitution-resolved case is identical to the literal case.
  - **S10.15** (quoted whitespace between structured substitutions): incidentally fixed as
    a side effect — a quoted `" "` scalar between two object/array substitutions now
    triggers the S10.13 scalar+structured error.
  - The S15 numeric-object-to-array bridge (`{"0":"a","1":"b"} concat [1]`) is preserved.
  - Conformance fixtures: `xx.hocon testdata/hocon/concat-errors/ce01–ce15` wired in
    `tests/concat_errors_test.rs`.

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

  > **⚠ Retracted by E8 amendment (2026-05-20)**: the value-position `-` reject described above was reverted in the [1.3.0] section. xx.hocon E8 was rewritten to adopt Lightbend's pragmatic reading of HOCON.md L270-276 ([xx.hocon#31](https://github.com/o3co/xx.hocon/issues/31), driven by external report @cgordon). The substitution-body and dotted-key-segment strict checks are NOT retracted — those remain in force as out-of-E8-scope path-element rules.
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
