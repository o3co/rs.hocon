> **Project-local copy** of the cross-impl E12 design spec. Canonical source:
> `.claude/superpowers/specs/2026-05-21-e12-deferred-resolution-design.md`
> in the hocon scope (not part of this repo). Spec reference:
> [xx.hocon `docs/extra-spec-conventions.md` § E12](https://github.com/o3co/xx.hocon/blob/main/docs/extra-spec-conventions.md#e12).

# E12 — Deferred substitution resolution (parse / merge / resolve lifecycle)

**Date**: 2026-05-21
**Phase**: E-series (extra-spec conventions)
**Target item**: E12 (new, will be appended to `docs/extra-spec-conventions.md` after E11)
**External request**: [o3co/go.hocon#99](https://github.com/o3co/go.hocon/issues/99) (cgordon)
**Tracking**: xx.hocon#TBD (to be filed after ★1)

---

## Summary

Expose Lightbend's `parse / withFallback / resolve` lifecycle as a public API in all three impls. Currently `ParseString`/`ParseFile` parse-and-resolve in a single step, which prevents callers from composing fallback layers before resolution and forces ugly workarounds (synthetic HOCON injection, env mutation, pre-serialised fallback layers).

The internal pipeline in all three impls is **already two-phase** (parse → unresolved tree → resolve to value tree). This spec only exposes the existing seam — no resolution-engine redesign is required.

---

## Motivation

### Current behaviour (all three impls)

`ParseString` / `parseString` / `parse` invokes parse and substitution resolution in one shot. A HOCON like:

```hocon
version = ${shortversion}-${CI_RUN_NUMBER}
variables { shortversion = "1.2.3" }
```

fails at parse time if `CI_RUN_NUMBER` is not in `os.Environ` / `process.env` / `std::env`, even when the caller wants to supply `CI_RUN_NUMBER` from a programmatic fallback layer after parsing.

### Lightbend reference behaviour

In `com.typesafe.config`:

- `ConfigFactory.parseString(str)` returns an **unresolved** `Config` by default.
- `Config.withFallback(ConfigMergeable other)` composes configs (receiver wins).
- `Config.resolve()` / `resolve(ConfigResolveOptions)` resolves substitutions explicitly.
- `Config.isResolved()` reports completion.
- `ConfigFactory.defaultReferenceUnresolved()` exists specifically to return unresolved configs for callers that want to defer resolution.

The three impls deviated from Lightbend by fusing parse+resolve. Restoring the lifecycle separation is a Lightbend-parity fix, not a feature addition.

### Acceptance criteria (from issue #99)

- Existing `ParseString` / `ParseFile` behaviour remains backward compatible.
- A caller can parse a config containing unresolved substitutions without receiving an error.
- A caller can merge fallback layers after parsing and before resolution.
- A caller can resolve the merged config explicitly.
- Substitutions resolve against the full merged fallback stack.
- `WithFallback` precedence matches Lightbend (receiver wins).
- `AllowUnresolved` supports partial resolution.
- `UseSystemEnvironment=false` supports deterministic, explicit-source-only resolution.
- Error messages after explicit `Resolve` still preserve useful source path, line, and column information.

---

## Resolved decisions (★1-pending — see Open Q)

OQ-1 through OQ-10 from the Open-Questions pass on 2026-05-21:

| OQ | Decision |
|---|---|
| OQ-1 (default `ResolveSubstitutions`) | Keep existing `ParseString`/`ParseFile` fused (parse+resolve) for back-compat. New opt-in `ParseStringWithOptions` for parse-only. Future v2 may flip default; not in scope. |
| OQ-2 (options shape) | `ParseOptions` / `ResolveOptions` struct passed to `*WithOptions` entry points. No builder pattern. |
| OQ-3 (`WithFallback` on unresolved) | Existing `WithFallback` is extended to accept both resolved and unresolved configs. Merge operates at unresolved-tree level when either operand is unresolved. |
| OQ-4 (`ResolveWith`) | Spec text includes `ResolveWith(source)` (Lightbend semantic: source used for lookup only, not merged into result). Impl conformance: MUST in v1 for the impl where the issue was filed (go.hocon); MAY for ts/rs in v1 (follow-on PR ok). |
| OQ-5 (`FromMap`) | `FromMap(values, originDescription)` (plain keys, Lightbend `ConfigValueFactory.fromMap`). **`FromAnyRef` deferred to follow-on** (requires public `ConfigValue` type — see § "Value factories"). Path-expression `parseMap` also deferred. |
| OQ-6 (Unresolved getter) | Getters on unresolved `Config` return language-idiomatic `NotResolved` error. `AllowUnresolved=true` still errors on getters that hit unresolved paths. |
| OQ-7 (Custom resolver chain) | Out of scope for v1. Lightbend `ConfigResolveOptions.appendResolver` is a v1.3.2+ feature; track as follow-on E-item if needed. |
| OQ-8 (xx.hocon spec category) | `docs/extra-spec-conventions.md` E12 entry (HOCON.md is silent on API surface, so cross-impl convention applies). |
| OQ-9 (Include timing) | Includes are resolved at parse phase, NOT deferred. `UnresolvedConfig` has includes already expanded; only `${...}` substitutions are deferred. E11 `package(...)` resolver runs at parse time. |
| OQ-10 (Other ConfigParseOptions fields) | v1 `ParseOptions`: **`ResolveSubstitutions` + `OriginDescription`**. `FromMap`/`Empty` also accept `originDescription`. Other Lightbend `ConfigParseOptions` fields (`setAllowMissing`, `setIncluder`, `setClassLoader`, `setSyntax`) deferred. (Revised from initial "ResolveSubstitutions only" — adding parse origin was trivial and improves error messages for non-file sources.) |

---

## Definitions

- **Unresolved Config**: a `Config` produced by parsing where one or more `${...}` substitutions remain unresolved. `IsResolved()` returns `false`. Getters raise `NotResolved` for paths whose value (or transitive parent) contains an unresolved substitution.
- **Resolved Config**: a `Config` where no `${...}` substitution remains. `IsResolved()` returns `true`. Getters operate normally.
- **Substitution placeholder**: the in-memory representation of an unresolved `${foo}` / `${?foo}` / `${X[]}` reference. Each impl already has an internal type for this (go: `substPlaceholder`, ts: `SubstPlaceholder`, rs: `ResolverValue::Subst`). The type stays internal; the public surface only exposes it via `IsResolved()` and error paths.
- **Phase-1 (parse)**: tokenize → AST → unresolved value tree (with includes expanded, substitution placeholders intact).
- **Phase-2 (resolve)**: walk unresolved tree, look up each substitution path against the merged tree + (optionally) env, replace placeholders with values.
- **Merged tree**: the value tree resulting from a chain of `WithFallback` invocations. Logical structure: `[receiver, fallback₁, fallback₂, …]`, with receiver winning. Substitution placeholders within any layer survive into the merged tree.
- **Fallback stack**: synonym for *merged tree* when emphasising the layered composition.

### Immutability invariant

All `Config` instances are immutable. `WithFallback`, `Resolve`, `ResolveWith` return new `Config` instances; receivers are never mutated. This matches Lightbend `Config` (immutable, all "modifier" methods return new instances).

### Idempotency of Resolve

`Resolve` on an already-resolved `Config` is a no-op that returns an equivalent `Config` (either the same instance or a copy with identical value tree). Matches Lightbend's documented `resolve()` behaviour: "Resolving an already-resolved config is a harmless no-op".

### Single-pass resolution over fallback stack (one top-level operation)

`Resolve()` performs **one top-level resolve operation over the entire fallback stack**. This is the Lightbend recommendation: "ideally [resolve] should be invoked on root config objects … resolved one time for your entire stack of fallbacks". Per-layer resolution before `WithFallback` is allowed but discouraged because substitutions in upper layers cannot see lower-layer values.

"One top-level operation" does NOT mean "one tree walk". Substitution resolution is **transitive and lazy** within that one operation: resolving `${a}` where `a = ${b}` and `b = ${c}` and `c = 1` MUST yield `1` (not leave `${b}` unresolved). Cycles within the transitive chain are detected per § "Cross-layer cycle detection".

Transitive resolution fixture: dr20 (chained substitution within a single source) and dr21 (chained substitution across fallback layers).

### Hidden substitutions are not evaluated

HOCON's substitution semantics (HOCON.md §Substitutions L670–L703) require that **substitutions in overridden values are discarded before resolution**:

```hocon
foo = ${does-not-exist}
foo = 42
```

This MUST resolve to `{ foo: 42 }` without error. The first `foo = ${does-not-exist}` is overridden by the second definition and removed from the resolution tree.

The same rule applies across fallback layers. `A.WithFallback(B)` produces a merged tree where A's keys win. If A has `foo = ${does-not-exist}` and B has `foo = 42`, then **A's substitution wins** (A is receiver) — error. But `B.WithFallback(A)` makes B the receiver, B's `foo = 42` wins, and A's substitution is dropped — no error.

**Definition refinement**: the **merged tree** (the input to resolution) is the *visible* value tree post-merge. Overridden values, including their substitution placeholders, are NOT in the merged tree and are NOT evaluated.

**Lookback exception**: self-reference lookback (s13a) preserves a separate "lookback chain" of prior values that were overridden by the substituting definition. This chain is consulted only by self-reference resolution and is otherwise invisible. See § "s13a × WithFallback".

Fixtures dr22 (hidden unresolved within single source) and dr23 (hidden unresolved across fallback layers) pin this behaviour.

### Transitive substitution resolution

`${a}` where `a` resolves to a value containing `${b}` MUST trigger resolution of `${b}` as part of the same top-level resolve operation. Resolution does not stop at one level of indirection.

Implementations typically achieve this via lazy / recursive evaluation of the substitution graph with cycle detection. The conformance requirement is the outcome, not the algorithm. Fixtures dr20/dr21 cover direct and cross-layer cases.

### Cross-layer cycle detection

Substitution cycles must be detected in the merged tree, including cycles that emerge only after merging:

```hocon
# receiver
a = ${b}
# fallback
b = ${a}
```

After `WithFallback`, resolving the merged tree must detect the `a → b → a` cycle and raise `ResolveError` (or `NotResolved` with `AllowUnresolved=true`, per § "Conformance levels"). The cycle-detection algorithm is impl-internal; the conformance requirement is that emerging-on-merge cycles are detected.

---

## Public API surface

The surface is defined here language-agnostically; per-impl naming follows each language's idiom (see § "Per-impl naming").

### Parse entry points

```text
ParseString(input) -> Config                    (existing, fused parse+resolve)
ParseFile(path)    -> Config                    (existing, fused parse+resolve)

ParseStringWithOptions(input, ParseOptions) -> Config   (new)
ParseFileWithOptions(path, ParseOptions)    -> Config   (new)
```

When `ParseOptions.ResolveSubstitutions = true` (the spec-defined default), `ParseStringWithOptions` produces a `Config` indistinguishable from `ParseString` (same resolved value tree, same origin chain). When `false`, the returned `Config` has `IsResolved() == false` if the input contains any `${...}`, otherwise `true`.

`ParseOptions` v1 *semantic* fields (per-language encoding follows § "Options encoding per language"):

```text
ResolveSubstitutions: bool    // default true
OriginDescription:    string  // optional, default ""; user-visible source name (Lightbend ConfigParseOptions.setOriginDescription)
```

`OriginDescription` is included in v1 because it is trivial to plumb through and improves error messages when the source isn't a file path (e.g. in-memory strings, REST API payloads). Other Lightbend `ConfigParseOptions` fields (`setSyntax`, `setAllowMissing`, `setIncluder`, `setClassLoader`) are deferred (see § "Out of scope").

### Options encoding per language

The spec defines option **semantics** (which defaults are which). Each impl encodes options idiomatically to its language. The hard constraint: an invocation equivalent to "use all defaults" MUST produce Lightbend default behaviour without requiring the caller to set anything.

| Lang | Encoding | Default invocation |
|---|---|---|
| **Go** | Builder pattern: unexported fields + `DefaultParseOptions()` / `DefaultResolveOptions()` factory functions + `WithX(v) ParseOptions` setter methods that return modified copies. **`ParseOptions{}` zero-value literal is NOT a valid invocation** and is documented as such. | `hocon.ParseString(s)` (no opts); or `hocon.ParseStringWithOptions(s, hocon.DefaultParseOptions())` |
| **TS** | `Partial<ParseOptions>` (interface where every field is optional). Omitted field → spec default. | `parseString(s)` (no opts); or `parseString(s, { resolveSubstitutions: false })` |
| **Rust** | `Default` impl returning spec defaults + chainable builder methods. `ParseOptions::defaults()` returns defaults. | `hocon::parse(s)` (no opts); or `hocon::parse_string_with_options(s, ParseOptions::defaults().with_resolve_substitutions(false))` |

### Value factories

```text
FromMap(values, originDescription) -> Config            (Lightbend ConfigValueFactory.fromMap)
Empty(originDescription) -> Config                      (Lightbend ConfigFactory.empty)
```

**`FromAnyRef` is OUT OF SCOPE for v1**. See § "Out of scope".

**`Empty()` equivalence**: `Empty(o)` is equivalent to `FromMap({}, o)`. Impls MAY implement one in terms of the other.

### Composition

```text
config.WithFallback(other) -> Config
```

- Receiver's keys win.
- Accepts both resolved and unresolved operands. Result is unresolved iff either operand is unresolved.
- Non-object values do not merge: `obj.WithFallback(nonObj).WithFallback(otherObj)` ignores `otherObj` (Lightbend `ConfigMergeable` semantic).
- Substitution placeholders survive merge unchanged. Substitution lookup at `Resolve()` time uses the merged tree.

### Resolution

```text
config.Resolve(ResolveOptions) -> Config
config.ResolveWith(source, ResolveOptions) -> Config
config.IsResolved() -> bool
```

`ResolveOptions` fields (Lightbend `ConfigResolveOptions` subset):

```text
UseSystemEnvironment: bool   // default true; if false, no os.Environ/process.env fallback
AllowUnresolved:      bool   // default false; if true, partial resolution doesn't error
```

`Resolve(ResolveOptions{})` with default values is equivalent to Lightbend `Config.resolve()`.

`ResolveWith(source, opts)` semantic: substitutions in receiver are looked up in `source`, but `source`'s keys are NOT merged into the result. Differs from `WithFallback(source).Resolve(opts)` because the latter includes `source`'s keys in the resulting `Config`.

**Precondition on `ResolveWith` source**: `source` MUST be resolved. If `source` is unresolved, `ResolveWith` MUST error with `NotResolved` BEFORE attempting to resolve the receiver.

`IsResolved()` returns `false` if any substitution placeholder remains in the value tree (whole-config granularity, matching Lightbend; no per-value `isResolved`).

### Getters on unresolved Config

Reading any path whose value (or any transitive parent's value) contains an unresolved substitution placeholder returns the language-idiomatic `NotResolved` error.

`AllowUnresolved=true` does NOT make getters lenient — it only makes `Resolve()` itself non-erroring. Paths that resolve cleanly are returned; paths that don't error at getter call.

### Optional substitution materialisation in concat contexts

Per HOCON.md §Substitutions L626–L645 + §Concatenation L387–L441, when an optional `${?foo}` is undefined, the materialised value depends on the surrounding concat context. Normative rules:

| Context | Undefined `${?foo}` materialises as | Example | Result |
|---|---|---|---|
| Standalone field value | Field is **omitted** from parent object | `a = ${?x}` (x undef) | `{}` (no `a` key) |
| String concat | Empty string | `a = ${?x} "tail"` | `a = " tail"` |
| String concat (multiple optional, all undef) | Empty string; if entire value is empty, field is omitted | `a = ${?x}${?y}` (both undef) | `{}` (no `a` key) |
| Array concat | Empty array (no elements contributed) | `a = ${?x} [1,2]` (x undef) | `a = [1,2]` |
| Object merge | Empty object (no keys contributed) | `a = ${?x} { k = 1 }` (x undef) | `a = { k = 1 }` |

Fixtures dr24–dr28 cover these cases.

---

## Per-impl naming (Rust)

| Lightbend (Java) | Rust (`hocon` crate) |
|---|---|
| `ConfigFactory.parseString(s)` | `hocon::parse(s)` |
| `ConfigFactory.parseString(s, opts)` | `hocon::parse_string_with_options(s, opts)` |
| `ConfigParseOptions` | `hocon::ParseOptions` |
| `ConfigResolveOptions` | `hocon::ResolveOptions` |
| `Config.withFallback` | `Config::with_fallback` |
| `Config.resolve()` | `Config::resolve(&self, opts)` |
| `Config.resolveWith(src)` | `Config::resolve_with(&self, src, opts)` |
| `Config.isResolved()` | `Config::is_resolved(&self)` |
| `ConfigValueFactory.fromMap(m, origin)` | `hocon::from_map(m, origin)` (serde feature) |
| `ConfigFactory.empty()` | `hocon::empty(origin)` |

---

## Cross-spec interactions

### s13a (self-reference lookback) × WithFallback

S13a defines that `${?a}` inside the definition of `a` looks at the **prior value** of `a`. The "prior value" semantic extends to merged trees:

> After `WithFallback`, the receiver's definition of `a` (if any) is the "current" value, and the fallback's definition of `a` is the "prior value" for self-reference lookback purposes within the receiver's `a` definition.

Edge cases:

1. Receiver has `a = ${?a} extra`, fallback has `a = base`. After merge + resolve: `a = "base extra"`.
2. Receiver has `a = ${a} extra` (required), fallback has `a = base`. After merge + resolve: `a = "base extra"`.
3. Receiver has `a = ${a} extra`, fallback has no `a`. After merge + resolve: **error**.

### s10 (concat type-check) × AllowUnresolved

Under `AllowUnresolved=true`, `${foo}` may not resolve. The concat type-check must:

- **Resolved operands present**: type-check fires if at least one operand's type is determined.
- **All operands unresolved**: no type-check; concat remains as concat-placeholder. Getter on this path → `NotResolved`.
- **Mixed operands, types incompatible**: type-error fires immediately even under `AllowUnresolved`.

Rationale: `AllowUnresolved` defers *missing-value* errors, not *type* errors.

---

## Error types

Rust impl uses:
- `NotResolvedError { path: String }` — returned (as `HoconError::NotResolved`) when a getter is called on a path whose value contains an unresolved substitution placeholder. Per E12 decision 12.
- Existing `ResolveError` — unchanged; fires from `Resolve()` when `AllowUnresolved=false` and a substitution can't be resolved.

---

## Fixture inventory (30 scenarios, dr01–dr30)

See `tests/testdata/hocon/deferred-resolution/` for the full fixture set. Summary:

| ID | Scenario |
|---|---|
| dr01 | Basic fallback (issue #99 example) |
| dr02 | FromMap-only fallback |
| dr03 | Multi-layer fallback (3+ layers) |
| dr04 | Self-reference across fallback (`a = ${?a} extra` + fallback `a = base`) |
| dr05 | Required self-reference with fallback prior |
| dr06 | Required self-reference without fallback prior → error |
| dr07 | AllowUnresolved=true partial resolution |
| dr08 | UseSystemEnvironment=false ignores process env |
| dr09 | Getter on unresolved → NotResolved error |
| dr10 | WithFallback non-object override (composition barrier) |
| dr11a | ResolveWith vs WithFallback().Resolve(): source keys absent in former |
| dr11b | ResolveWith with unresolved source → NotResolved error |
| dr12 | Origin preserved through merge + resolve (skipped in YAML runner — origin format diverges) |
| dr13 | Type-check under AllowUnresolved (type error fires even under partial) |
| dr14 | Type-check under AllowUnresolved (deferred concat-placeholder for fully-unresolved) |
| dr15 | Include + deferred resolve (include expanded at parse; only `${...}` deferred) |
| dr16 | FromMap nested coercion |
| dr17 | E11 `include package(...)` + deferred resolve (skipped in YAML runner) |
| dr18 | Cross-layer cycle (receiver `a = ${b}`, fallback `b = ${a}`) → error |
| dr19 | Resolve idempotency |
| dr20 | Transitive substitution within single source |
| dr21 | Transitive substitution across fallback layers |
| dr22 | Hidden unresolved within single source |
| dr23 | Hidden unresolved across fallback layers |
| dr24 | Optional `${?x}` standalone, undefined → field omitted |
| dr25 | Optional `${?x}` in string concat, undefined → empty-string contribution |
| dr26 | Optional `${?x}` in array concat, undefined → empty-array contribution |
| dr27 | Optional `${?x}` in object merge, undefined → empty-object contribution |
| dr28 | Multiple optional `${?x}${?y}` all undefined → field omitted |
| dr29 | Empty config edge cases |
| dr30 | Object-merge barrier |

---

## Conformance levels

| Item | Level |
|---|---|
| Existing `ParseString`/`ParseFile` parse+resolve | MUST |
| `parse_string_with_options(s, ParseOptions::defaults().with_resolve_substitutions(false))` returns unresolved | MUST |
| `with_fallback` accepts unresolved operands | MUST |
| `resolve` with `UseSystemEnvironment=false` does not consult env | MUST |
| `resolve` with `AllowUnresolved=true` does not error on unresolved | MUST |
| `is_resolved()` reports completeness accurately | MUST |
| Getters on unresolved → `NotResolved` error | MUST |
| `from_map(values, origin)` accepts plain-key map (serde feature) | MUST |
| `empty(origin)` | SHOULD |
| Transitive substitution | MUST |
| Hidden substitution in overridden value not evaluated | MUST |
| Optional `${?foo}` materialisation in concat contexts | MUST |
| `resolve_with` with unresolved source errors with `NotResolved` | MUST |
| s13a lookback walks merged tree | MUST |
| s10 type-check under `AllowUnresolved=true` | MUST |
