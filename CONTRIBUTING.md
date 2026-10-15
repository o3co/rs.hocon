# Contributing to hocon

Thank you for your interest in contributing! This document covers everything you
need to get started.

## Bug Reports

Please open a [GitHub issue](https://github.com/o3co/rs.hocon/issues) with:

- **Rust version** (`rustc --version`)
- **hocon version** (from `Cargo.toml` or `cargo tree`)
- **Minimal HOCON snippet** that reproduces the problem
- **Expected behavior** vs. **actual behavior**
- **OS and platform**

## Feature Proposals

Before writing code, open an issue to discuss the idea. This avoids duplicate
work and ensures the feature fits the project direction.

## Development Setup

```sh
git clone https://github.com/o3co/rs.hocon.git
cd rs.hocon
make testdata   # fetch the conformance fixtures (see below)
make test       # cargo test, --features serde and --all-features
```

`make testdata` downloads the shared conformance fixtures from
[o3co/xx.hocon](https://github.com/o3co/xx.hocon) into `tests/testdata/`. They
are gitignored, so on a fresh clone `cargo test` fails with missing-fixture
errors until you run it. It pins the fetched commit in `.xx-hocon-version` and
skips the download while that pin is current; run it again to pick up new
fixtures.

## Testing

```sh
# Run all tests
cargo test

# Run tests for a specific module
cargo test --test integration_test
cargo test --test include_test
cargo test --test serde_test

# Run with serde feature enabled
cargo test --features serde

# Run the format-adapter suites. They are `#![cfg(feature = "adapters")]`, so a
# plain `cargo test` compiles them to EMPTY binaries that pass without
# asserting anything — this is the only run that exercises them.
cargo test --all-features
cargo test --all-features --test adapters_test --test format_ingestion_test

# Run Lightbend equivalence / compliance tests
cargo test --test lightbend_test
```

All pull requests must pass `cargo test`, `cargo test --features serde` and
`cargo test --all-features`.

## Code Style

- **Format**: Run `cargo fmt` before committing. CI enforces `cargo fmt --check`.
- **Lint**: Run `cargo clippy -- -D warnings` and
  `cargo clippy --all-features --all-targets -- -D warnings` (the adapter
  modules are feature-gated, so the first command never sees them;
  `--all-targets` also covers tests and examples). CI enforces zero warnings
  for both.
- **Error handling**: Use `Result` and `Option` patterns. Avoid `.unwrap()` in
  library code.
- **Visibility**: Use `pub(crate)` for internal modules and helpers. Only expose
  types that are part of the public API.
- **Tests**: Every bug fix and new feature must include tests. Prefer small,
  focused test functions.

## Pull Request Process

1. Fork the repository and branch from `develop`.
2. Write your changes with tests.
3. Run `cargo fmt`, both clippy configurations (`cargo clippy -- -D warnings` and
   `cargo clippy --all-features --all-targets -- -D warnings` — CI enforces
   both), and `make test`.
4. Open a PR against `develop` with a clear description of what changed and why.
5. Link any related issues.

## Releasing

Releases are cut by pushing a `v*` tag; `.github/workflows/publish.yml` does
the rest. `Cargo.toml`'s `version` is not bumped by hand — the workflow sets it
from the tag (`cargo set-version`) before testing and publishing.

1. **Release-prep PR** against `develop`, with one commit
   `chore(release): X.Y.Z` that:
   - renames `## [Unreleased]` in `CHANGELOG.md` to `## [X.Y.Z] - YYYY-MM-DD`
     and opens a new empty `## [Unreleased]` above it;
   - updates the link references at the bottom of `CHANGELOG.md`
     (`[Unreleased]` compares from `vX.Y.Z`, plus a `[X.Y.Z]` line).
2. **Tag** the merged commit on `develop` with `vX.Y.Z` and push the tag.
3. **CI** (`publish.yml`) then:
   1. sets the crate version from the tag;
   2. fetches the fixtures and runs `cargo test`, `--features serde` and
      `--all-features`;
   3. extracts the `## [X.Y.Z]` section of `CHANGELOG.md` as the release
      notes — a missing section fails the run before anything is published;
   4. creates the GitHub Release, then runs `cargo publish`.

> **Do not** run `cargo publish` locally. crates.io never accepts the same
> version twice, so a release has to go through the gated workflow.

## License Agreement

By contributing, you agree that your contributions will be licensed under the
[Apache License 2.0](LICENSE), the same license as the project.
