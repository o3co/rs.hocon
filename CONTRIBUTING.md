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
cargo test
cargo test --features serde
```

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

# Run Lightbend equivalence / compliance tests
cargo test --test lightbend_test
```

All pull requests must pass `cargo test` and `cargo test --features serde`.

## Code Style

- **Format**: Run `cargo fmt` before committing. CI enforces `cargo fmt --check`.
- **Lint**: Run `cargo clippy -- -D warnings`. CI enforces zero warnings.
- **Error handling**: Use `Result` and `Option` patterns. Avoid `.unwrap()` in
  library code.
- **Visibility**: Use `pub(crate)` for internal modules and helpers. Only expose
  types that are part of the public API.
- **Tests**: Every bug fix and new feature must include tests. Prefer small,
  focused test functions.

## Pull Request Process

1. Fork the repository and branch from `develop`.
2. Write your changes with tests.
3. Run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test --features serde`.
4. Open a PR against `develop` with a clear description of what changed and why.
5. Link any related issues.

## Releasing

Releases are published to crates.io automatically by CI when a `v*` tag is pushed.
Use [cargo-release](https://github.com/crate-ci/cargo-release) to do everything in one command:

```sh
# Install once
cargo install cargo-release

# Release a patch (0.1.3 → 0.1.4), minor, or major bump
cargo release patch   # or: cargo release minor / cargo release major
```

This will:

1. Bump the version in `Cargo.toml`
2. Create a commit (`chore: release v0.1.4`)
3. Tag it (`v0.1.4`)
4. Push the commit and tag to origin
5. CI picks up the tag and runs `cargo publish`

> **Do not** run `cargo publish` locally — CI handles it and verifies the tag matches `Cargo.toml`.

## License Agreement

By contributing, you agree that your contributions will be licensed under the
[Apache License 2.0](LICENSE), the same license as the project.
