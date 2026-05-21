# E11 `include package(...)` — rs.hocon Design Notes

**Status**: Design / pre-implementation (★1 user review pending)
**Branch**: `feat/include-package-design`
**Spec ref**: `xx.hocon/docs/extra-spec-conventions.md` §E11

---

## 1. Crate survey summary

| Aspect | Current state |
|---|---|
| Crate name | `hocon-parser`, lib name `hocon` |
| MSRV | 1.82 |
| Features | `default = []`, `serde = [dep:serde, dep:serde_json]` — one optional feature today |
| `no_std` | **No.** `std` is used throughout (`std::fs`, `std::path::PathBuf`, `std::collections::HashMap`, `std::io`). No `#![no_std]` attribute anywhere. |
| Async | **No.** No tokio/futures dep, no async fn anywhere. |
| Builder pattern | None currently. The public API is free functions: `parse`, `parse_file`, `parse_with_env`, `parse_file_with_env`. `ResolveOptions` uses a consuming builder (`pub fn with_base_dir(mut self, …) -> Self`). |
| Error types | `ParseError`, `ResolveError`, `ConfigError`, `HoconError` (enum wrapping the three). All use `#[non_exhaustive]`. |
| Include handling | `AstNode::Include { path, required, is_file, pos }` — all include kinds fold into this one variant. Include loading is in `src/resolver/include_loader.rs`. Cycle detection uses `opts.include_stack: Vec<PathBuf>` carried through `ResolveOptions`. |
| Module structure | `src/lib.rs` re-exports public surface. Internal: `lexer`, `parser`, `resolver/{mod, include_loader, structure_builder, substitution_resolver, types, utils}`. `pub mod config`, `pub mod error`, `pub mod value`, optionally `pub mod serde`. |

---

## 2. Design decisions

### 2.1 Builder method signature for `register_package`

**Decision**: consuming builder, returns `Self`.

```rust
pub fn Parser(/* private fields */) { ... }

impl Parser {
    pub fn new() -> Self { ... }

    pub fn register_package(
        mut self,
        identifier: impl Into<String>,
        file: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        // insert into registry, return self
        self
    }

    pub fn parse(self, input: &str) -> Result<Config, HoconError> { ... }
    pub fn parse_file(self, path: impl AsRef<Path>) -> Result<Config, HoconError> { ... }
}
```

**Justification**: `ResolveOptions` already uses the consuming-builder style (`with_base_dir(mut self) -> Self`). Consistent. `&mut self -> &mut Self` (mutable-ref builder) is usable but is the secondary convention in the Rust ecosystem for types that don't need to be stored in a variable mid-construction. `impl Into<String>` for all three args (see §2.2 for content lifetime rationale) keeps the site ergonomic: `include_str!` gives `&'static str` which coerces to `String` via `Into`; runtime-loaded `String` passes through unchanged.

**On `Cow`**: `Cow<'static, str>` is the most flexible, but adds cognitive overhead at the call site and requires callers to import `std::borrow::Cow`. Since the crate is fully `std`-dependent already and the primary use case is `include_str!` (which allocates once at registration time via `String::from(&str)`), `String` is the right choice — it unifies both cases without a new type at the surface.

---

### 2.2 Content lifetime

**Decision**: `impl Into<String>` — stored as owned `String`.

**Rationale**:
- `include_str!` produces `&'static str`. `impl Into<String>` accepts this via `String: From<&str>` — it allocates and copies the bytes once at registration time (not per-parse). Cost is one startup allocation; negligible.
- Owned `String` allows runtime-loaded content (file read, test fixtures). `&'static str` only would block testability without `leak` hacks.
- `Cow<'static, str>` would avoid the allocation for `include_str!` callers, but the ergonomic benefit is small and the API surface cost is real (callers must manage `Cow` at the call site, or the impl must accept `impl Into<Cow<'static, str>>` which is not `impl Into<String>`-compatible).
- **Consequence for `include_str!` callers**: `parser.register_package("id", "file", include_str!("foo.conf"))` — the `&'static str` converts to `String` via `Into`. No explicit cast needed. This is the primary use-case pattern.

---

### 2.3 Cascade convention design

**Decision**: free function convention — no trait.

**Recommended convention**:

```rust
// In pkg_a/src/hocon.rs (or lib.rs)
pub fn register(parser: hocon::Parser) -> hocon::Parser {
    // register pkg_a's own files
    let parser = parser
        .register_package("github.com/org/pkg_a", "reference.conf", include_str!("../conf/reference.conf"));
    // cascade to deps
    let parser = pkg_b::hocon::register(parser);
    parser
}
```

Caller side:
```rust
let config = pkg_a::hocon::register(hocon::Parser::new())
    .parse_file("app.conf")?;
```

**Why free function, not trait**:

The primary reason is ergonomics, not Rust coherence. A `pub trait HoconPackage { fn register(parser: Parser) -> Parser; }` defined in `hocon-parser` *can* be implemented by downstream crates without violating coherence rules (orphan rule applies to blanket impls over foreign types, not to concrete impls — downstream crates can implement upstream traits for their own types). However:

- Dynamic dispatch on the trait is not useful here — callers name the packages explicitly at registration time, not via a vtable.
- Callers still write `pkg_a::hocon::register(p)` either way — the trait adds no abstraction benefit at the call site.
- Free functions compose naturally with the consuming-builder chain without requiring an `impl HoconPackage for SomeMarkerType` boilerplate in each package crate.
- A macro (`register_hocon_package!(pkg_a, pkg_b)`) could auto-cascade but adds complexity with no benefit; free function calls are readable and greppable.
- Free functions parallel the Go-style `init()` side-effect pattern without global state.

**Convention location**: the `register` function should be in a dedicated module within the package crate (`pub mod hocon` or `pub mod config`), not in `lib.rs` root. This avoids polluting the package's top-level API with a HOCON-specific symbol that most users of that package may not need.

---

### 2.4 Error type

**Decision**: no new exported error type. Existing types cover all E11 error paths.

**Detailed error placement**:

- **Resolve-time lookup miss** (E11 decision 4): fires inside `load_package_include` in `resolver/include_loader.rs` when the resolver encounters `AstNode::PackageInclude` and the registry does not contain the `(identifier, file)` pair. This is a `ResolveError` (not a `ParseError`) because the registry is threaded through `ResolveOptions`, which is only available at resolve-time — not at parse-time when the AST is being built. `ResolveError { message: format!("include package not found: ({:?}, {:?}) — was Parser::register_package called?", id, file), ... }`. No new error variant needed; the existing `ResolveError { message, path, line, col }` struct covers it.

  *Note*: validation of argument shape (arity, non-string args, empty identifier, file constraints per E11 decision 6) fires at **parse-time** inside `parse_include` in `parser.rs` and produces `ParseError`. But registry lookup is **resolve-time** and produces `ResolveError`. These two error origins are different; do not conflate them.

- **Registration-time collision** (E11 decision 3): fires inside `register_package()` on `Parser`. Options:
  - **Option A**: `register_package` panics on collision. Simple; matches Rust convention for setup-time programming errors (e.g. `std::env::set_var` invariant failures, `HashMap`-backed registry patterns in `inventory`/Bevy). Idempotent re-registration of byte-equal content is allowed (no panic).
  - **Option B**: `register_package` returns `Result<Self, HoconError>`. Call site becomes `let parser = parser.register_package(…)?;` — works in `fn main() -> Result<…>` but awkward in `lazy_static!` / `OnceLock` / test harness contexts.
  - **Option C**: builder stores pending errors, surfaces on `.parse()`.

  **Recommendation**: **Option A** (panic on collision). Collision is a programming error (two package crates registering the same `(id, file)` key with different content), not a recoverable runtime error. Panic is the Rust convention for "invariant violated during initialization." Document: "Panics if different content is registered for the same `(identifier, file)` pair. Re-registering byte-identical content is idempotent."

**No new exported type for E11 errors** — the existing error surface (`ParseError` for syntax/arity errors, `ResolveError` for registry misses, panic for collision) is sufficient.

---

### 2.5 Feature flag

**Decision**: `include-package` optional feature flag, default off.

**Rationale**:
- Current default features: none (empty `default = []`). The `serde` feature is the only optional feature today. This is deliberate: the crate stays lean by default.
- `package(...)` support requires a `HashMap<(String, String), String>` registry on `Parser`. This adds ~48 bytes to `Parser` even when unused (the `HashMap` exists but is empty). With a feature flag, users who do not need `package(...)` pay zero cost.
- A `Parser` struct with the registry field is only introduced under `include-package`. Without the feature, the current free-function API (`parse`, `parse_file`, etc.) remains the entire public surface.
- **Trade-off against discoverability**: users who don't know about `package(...)` won't find `Parser::register_package` in docs unless they enable the feature. This is acceptable — `package(...)` is a cross-impl extension (E11), not a core HOCON feature. Users of cross-impl portable configs will know to opt in.
- **Cargo feature unification hazard**: Cargo unifies features across the dependency graph — if *any* crate in the build graph depends on `hocon-parser` with `features = ["include-package"]`, all other dependees also build with that feature enabled. The "zero cost for users who don't need it" claim holds only when *no* transitive dependency enables the feature. This is the standard Cargo feature trade-off: feature flags add complexity at the ecosystem level even when individual users don't opt in. Document this in crate-level docs. Since `include-package` adds no new `[dependencies]` entries (only `std` types), the unification hazard is limited to code-size/API-surface rather than dependency graph pollution.
- **Cargo.toml addition**: `include-package = []` (no new dep — `HashMap` is already in scope from `std`). No indirect dependency changes.

---

### 2.6 Module placement

**Decision**: `pub mod registry` nested under a new `pub mod include` in lib root, behind the `include-package` feature flag.

```
src/
  lib.rs
  include/           ← new, feature-gated
    mod.rs           ← pub use registry::PackageRegistry; re-exports
    registry.rs      ← PackageRegistry struct + impl
  parser.rs          ← existing; gains package(...) parse branch
  resolver/
    include_loader.rs ← gains load_package_include() fn
    ...
```

**Why `pub mod include` not `pub mod registry` at root**:
- `include` is the vocabulary the HOCON spec and the E11 spec use. Future `include`-related extensions (e.g., a custom include hook/callback) belong under this module.
- `registry` nested under `include` matches the scope of responsibility: it is the include-system's registry, not a general registry.
- However, the `PackageRegistry` type and `Parser` struct are re-exported from `hocon::` (lib root) so callers don't need to reach into `hocon::include::registry::`.

**Naming note**: `Parser` struct is the new public type introduced by this feature. It wraps the current `ResolveOptions`-based pipeline. The name `Parser` is slightly overloaded (there's already an internal `parser::Parser` struct, which is `pub(crate)`), but the public `hocon::Parser` is unambiguous at the consumer level.

---

### 2.7 `no_std` and Wasm

**Finding**: rs.hocon does **not** support `no_std`. The crate uses `std::fs`, `std::path`, `std::io`, `std::collections::HashMap` throughout. No `#![no_std]` and no `alloc` import.

**Implication for E11**: the registry (`HashMap<(String, String), String>`) is entirely `std` — no special `no_std` path needed or possible. If a future `no_std` effort is undertaken, the registry would need to switch to `alloc::collections::BTreeMap` and the `include_str!`-loaded content would stay as `&'static str` rather than `String`. That is a future concern, not in scope for E11.

**Wasm**: `wasm32-unknown-unknown` does not have a filesystem; `parse_file` already fails on Wasm. `package(...)` resolution via the in-parser registry (no `fs::read`) **does work on Wasm** — a key advantage over `include file(...)`. This should be documented as a use case: Wasm consumers can embed config via `include_str!` and `register_package`.

---

### 2.8 Async support

**Finding**: no async API. All parse functions are synchronous. No tokio/futures in `Cargo.toml`.

**Implication for E11**: `package(...)` resolution reads from an in-memory registry, not from disk or network. No async path is needed — registry lookup is a `HashMap::get` which is always synchronous and `O(1)`. Even if an async `parse_file` is added in the future, `package(...)` resolution stays sync (memory lookup inside the registry).

---

### 2.9 File argument validation (E11 decision 6)

**Decision**: validate at the parser layer (`parse_include` in `parser.rs`), not in the include loader.

**Validation shape** (illustrative, not implementation):

```rust
fn validate_package_file_arg(file: &str, line: usize, col: usize) -> Result<(), ParseError> {
    if file.is_empty() {
        return Err(ParseError { message: "package() file argument must be non-empty".into(), line, col });
    }
    if file.starts_with('/') {
        return Err(ParseError { message: "package() file argument must not be an absolute path".into(), line, col });
    }
    if file.contains('\\') {
        return Err(ParseError { message: "package() file argument must use forward-slash separators".into(), line, col });
    }
    for segment in file.split('/') {
        if segment.is_empty() {
            return Err(ParseError { message: "package() file argument must not contain consecutive slashes".into(), line, col });
        }
        if segment == "." || segment == ".." {
            return Err(ParseError { message: "package() file argument must not contain '.' or '..' segments".into(), line, col });
        }
    }
    Ok(())
}
```

**Why parser layer**: validation is syntactic (shape of the string literal argument), not semantic (does the content exist). Errors here are parse errors. Placing it in the include loader would delay the error to resolve-time, which is late and confusing — the `file` argument is visible in source; the parser should reject it immediately.

---

### 2.10 Cycle detection (E11 decision 8)

**Current mechanism**: `ResolveOptions::include_stack: Vec<PathBuf>`. Each include call checks whether the candidate `PathBuf` is already in the stack (`opts.include_stack.iter().any(|p| p.as_path() == candidate)`). The `PathBuf`-keyed stack does not accommodate `package(...)` includes (no filesystem path).

**Decision**: introduce a parallel `package_include_stack: Vec<(String, String)>` field on `ResolveOptions` to hold `(identifier, file)` pairs already being processed. Alternatively, use a unified `IncludeKey` enum:

```rust
#[derive(PartialEq, Eq, Clone)]
pub(crate) enum IncludeKey {
    Path(PathBuf),
    Package { identifier: String, file: String },
}
```

and change `include_stack: Vec<PathBuf>` to `include_stack: Vec<IncludeKey>`. This is cleaner — one stack, one cycle check, no duplication.

**Cycle check** in `load_package_include` (new function in `include_loader.rs`):

```rust
let key = IncludeKey::Package { identifier: identifier.to_string(), file: file.to_string() };
if opts.include_stack.contains(&key) {
    return Err(ResolveError {
        message: format!("circular package include: ({:?}, {:?})", identifier, file),
        ...
    });
}
// clone opts, push key, recurse
let mut child_opts = opts.clone_for_child();
child_opts.include_stack.push(key);
```

**Where this lands**: `IncludeKey` is a `pub(crate)` type in `resolver/types.rs`. `ResolveOptions::include_stack` changes from `Vec<PathBuf>` to `Vec<IncludeKey>`. The existing file-include path wraps its `PathBuf` in `IncludeKey::Path`. This is a purely internal change.

**Note on `ResolveOptions` field visibility**: `ResolveOptions` is defined as `pub struct` inside `pub(crate) mod resolver`. It is accessible within the crate via `pub use types::ResolveOptions` in `resolver/mod.rs`, and from `resolver::ResolveOptions` in `lib.rs`. It is **not** part of the external public API (the `pub use` at `lib.rs` level does not re-export it). Therefore `include_stack`'s type change from `Vec<PathBuf>` to `Vec<IncludeKey>` has zero external API impact.

**Final decision**: unified `IncludeKey` (not dual stacks). Removing this from open questions — it is an internal-only refactor with no ambiguity for callers.

---

### 2.11 Grammar validation in `parse_include` (E11 decisions 1, 2, 6)

The `parse_include` function in `parser.rs` must validate the `package(...)` qualifier fully at parse-time before producing an `AstNode::PackageInclude`. This covers argument shape — not registry lookup (which is resolve-time per §2.4).

**Mandatory parse-time checks**:

1. **Two-arg form** (E11 decision 2): after consuming `package(`, the parser must find exactly two `TokenKind::QuotedString` tokens separated by a comma. One-arg form (single string or path, no comma), zero-arg form, and three-or-more-arg form are all `ParseError`. Non-string args (unquoted tokens in the arg positions) are `ParseError`.

2. **Non-empty identifier** (E11 decision 1): the first argument, after HOCON string unescaping, must be non-empty. An empty identifier `""` is a `ParseError`.

3. **File argument constraints** (E11 decision 6): see §2.9 — validated by calling `validate_package_file_arg` on the second argument at parse-time.

**Parse-time validation shape** (illustrative, extending `parse_include`):

```rust
// Inside parse_include, after detecting "package(" prefix:
// Consume first quoted string (identifier)
let identifier = expect_quoted_string(self)?;  // ParseError if non-QuotedString
if identifier.is_empty() {
    return Err(ParseError { message: "package() identifier must be non-empty".into(), line, col });
}
// Consume comma separator
expect_comma(self)?;  // ParseError if absent
// Consume second quoted string (file)
let file_arg = expect_quoted_string(self)?;  // ParseError if non-QuotedString
validate_package_file_arg(&file_arg, line, col)?;
// Consume closing ")"
expect_close_paren(self)?;
```

---

## 3. `AstNode::Include` extension

The current variant:
```rust
AstNode::Include {
    path: String,
    required: bool,
    is_file: bool,
    pos: Pos,
}
```

For `package(...)`, we need to carry `identifier` and `file` separately. Two options:

**Option A**: Add a `qualifier` field to the existing variant:
```rust
AstNode::Include {
    qualifier: IncludeQualifier,  // enum: Bare, File, Url, Package { identifier, file }
    path: String,                 // for Bare/File/Url; empty for Package
    required: bool,
    pos: Pos,
}
```

**Option B**: Add a new variant `AstNode::PackageInclude { identifier, file, required, pos }`.

**Decision**: **Option A** (extend with a `qualifier` enum). Rationale: all include forms share `required` and position. An enum qualifier follows the principle of "make the set of valid states representable" — the `path` field is only meaningful for non-Package qualifiers. However, the `path: String` field becomes meaningless for `Package` — this is the code smell. Therefore, **Option B** is actually cleaner: a dedicated variant with no `path` field.

**Revised decision: Option B**. Add a dedicated variant:

```rust
AstNode::PackageInclude {
    identifier: String,
    file: String,
    required: bool,
    pos: Pos,
}
```

**On `#[non_exhaustive]` for `PackageInclude`**: `AstNode` is defined in `pub(crate) mod parser` — it is not part of the external public API. Adding `#[non_exhaustive]` to the new variant (as was done for `AstNode::Substitution`) is optional here, since all match arms are internal. The `#[non_exhaustive]` on `Substitution` was added for forward-compatibility within the internal codebase (making it easier to add fields later without updating every internal match site). The same rationale applies to `PackageInclude` — applying `#[non_exhaustive]` is recommended for consistency with `Substitution`, but is not a safety requirement. Critically, `#[non_exhaustive]` on a *variant* does NOT protect against exhaustive matches on the *enum* — all internal `match ast { ... }` arms must be updated to handle `PackageInclude` regardless. There is no need to add `#[non_exhaustive]` to `AstNode` itself; it remains a plain enum (internal only).

The `StructureBuilder::apply_field` match arm already handles `AstNode::Include` by calling `load_include`. A new arm handles `AstNode::PackageInclude` by calling `load_package_include`. Both arms are guarded by `field.key.is_empty()`.

---

## 4. `Parser` public type

Since `package(...)` requires pre-registering content before parsing, the current free-function API must be extended or a new entry point introduced. The cleanest approach is a new `Parser` struct (builder-style) under the `include-package` feature:

```rust
#[cfg(feature = "include-package")]
pub struct Parser {
    registry: HashMap<(String, String), String>,
}

#[cfg(feature = "include-package")]
impl Parser {
    pub fn new() -> Self {
        Parser { registry: HashMap::new() }
    }

    pub fn register_package(
        mut self,
        identifier: impl Into<String>,
        file: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let id = identifier.into();
        let f = file.into();
        let c = content.into();
        if let Some(existing) = self.registry.get(&(id.clone(), f.clone())) {
            if existing != &c {
                panic!(
                    "hocon: conflicting content registered for package ({:?}, {:?})",
                    id, f
                );
            }
            // byte-identical: idempotent, no-op
        } else {
            self.registry.insert((id, f), c);
        }
        self
    }

    pub fn parse(self, input: &str) -> Result<Config, HoconError> { ... }
    pub fn parse_file(self, path: impl AsRef<Path>) -> Result<Config, HoconError> { ... }
    pub fn parse_with_env(self, input: &str, env: &HashMap<String, String>) -> Result<Config, HoconError> { ... }
    pub fn parse_file_with_env(self, path: impl AsRef<Path>, env: &HashMap<String, String>) -> Result<Config, HoconError> { ... }
}
```

The `registry` is threaded into `ResolveOptions` (new optional field under the feature flag, `#[cfg(feature = "include-package")]`) so `load_package_include` can read it.

**`Default` impl**: `impl Default for Parser` (delegates to `Parser::new()`) — conventional for builder types.

**Empty content handling**: E11 decision 4 states that registered empty content (zero bytes, or content that parses to an empty document) is NOT a lookup failure — it succeeds and merges `{}`. However, the existing `assert_non_empty_document` guard in `parse_with_env` / `load_single_include` would reject an empty string. Therefore `load_package_include` must NOT call `assert_non_empty_document` on package-registered content. Instead it must handle the empty-token-stream case explicitly:

```rust
// In load_package_include:
let tokens = crate::lexer::tokenize(content)?;
// E11 decision 4: empty registered content => empty merge object, not an error
let has_content = tokens.iter().any(|t| !matches!(t.kind, TokenKind::Newline | TokenKind::Eof));
if !has_content {
    return Ok(ResObj::new());  // empty doc => {} merge, not an error
}
// continue with normal parse + resolve pipeline
```

This is the only place where the empty-document rule is intentionally relaxed.

---

## 5. Open questions for ★1

The following decisions are **not yet resolved** and require Yoshi's call before implementation can begin.

1. **`Parser` struct name**: the internal `parser::Parser` is `pub(crate)`. The public `hocon::Parser` does not collide at the public API level, but may be confusing to contributors reading source alongside the internal `parser::Parser`. Alternatives: `hocon::ParserBuilder` or `hocon::HoconParser`. Which does Yoshi prefer?

2. **Panic vs `Result` for registration collision**: the design recommends panic (§2.4 Option A). If `register_package` should return `Result<Self, HoconError>` instead (Option B), the cascade convention changes to `fn register(parser: Parser) -> Result<Parser, HoconError>`. Trade-off: call-site ergonomics vs explicit error propagation. Which policy?

3. **Feature flag default**: the design proposes `include-package` is not in `default`. If this feature is considered core to the non-JVM HOCON story, it could join `default`. Consequences: all users build with the `Parser` struct and registry even if they don't use `package(...)` syntax. Confirm `default = []` stays (recommended) or `include-package` joins it.

4. **`AstNode::PackageInclude` match arm update scope**: adding the variant requires updating all internal `match ast_node { ... }` exhaustive match arms in `structure_builder.rs` and anywhere else `AstNode` is matched. Is this acceptable scope for the E11 impl PR, or should a prep commit add `#[non_exhaustive]` to `AstNode` first to make the existing matches forward-compatible? (Note: `#[non_exhaustive]` on an internal `pub(crate)` type has limited benefit — it only silences `non_exhaustive_omitted_patterns` warnings, not compiler errors. The choice is a code-hygiene call, not a correctness one.)

**Resolved by design (not open questions)**:
- `IncludeKey` unification: decided in §2.10 — unified `IncludeKey` enum, not dual stacks. `ResolveOptions` is internal only (not externally public), so no API-impact concern.
- Lookup miss fires at **resolve-time** (in `load_package_include`), not parse-time — §2.4 clarified.
- Empty registered content handled in `load_package_include` without `assert_non_empty_document` — §4 specified.
- Grammar/arity validation fires at **parse-time** in `parse_include` — §2.11 specified.

---

## 6. Non-goals (per E11 spec)

Not designed for and must not be inferred:
- Auto-discovery, auto-`reference.conf` merge, transitive auto-resolution.
- Wildcard/glob lookups.
- Identifier shape validation at parse time.
- `url(...)` or `classpath(...)` qualifiers.
- Any async parse path.
