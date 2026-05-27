//! Cross-impl regression tests for go.hocon#128 — include-child
//! `${?ENV_VAR}` env-with-default pattern silently erases the prior
//! duplicate-key assignment when the env var is unset.
//!
//! Pattern under test (canonical Lightbend reference.conf idiom):
//!
//! ```hocon
//! registry {
//!   instance-id = "localhost"
//!   instance-id = ${?REGISTRY_INSTANCE_ID}
//! }
//! ```
//!
//! Spec basis: S7.1 (later non-object overrides earlier) +
//! S13.2/S13.11 (optional substitution undefined → field not created,
//! i.e. the second assignment "disappears", leaving the prior) +
//! S14b.2 (included keys merge per duplicate-key rules — include
//! boundary is invisible to the merge semantics).
//!
//! go.hocon v1.4.1–v1.5.2 lost the include-child's `priorValues` across
//! a separate lenient-resolve pass; rs.hocon merges include content
//! into the parent's tree at structure-build time
//! (`deep_merge_res_obj_into` preserves both fields and `prior_values`,
//! see `src/resolver/utils.rs`), so a single substitution-resolve pass
//! over the merged tree never strips the prior. These tests pin that
//! behaviour so a future refactor to a multi-pass shape can't silently
//! regress.
//!
//! Hermeticity: env is injected via `Parser::parse_with_env` and
//! `parse_file_with_env`; `std::env` is never read or mutated. Matches
//! the cross-impl convention used by `ts.hocon` (`parse(input, { env })`).
//! As a result these tests are safe to run in parallel — no shared
//! mutable process state.
//!
//! Run: `cargo test --features include-package --test issue128_include_env_fallback`

#![cfg(feature = "include-package")]

use hocon::Parser;
use std::collections::HashMap;
use tempfile::tempdir;

const CHILD_DEFAULT_PLUS_OPTIONAL_FILE_UNSET: &str = r#"
registry {
  instance-id = "localhost"
  instance-id = ${?GH128_RS_FILE_UNSET}
}
"#;

const CHILD_DEFAULT_PLUS_OPTIONAL_FILE_SET: &str = r#"
registry {
  instance-id = "localhost"
  instance-id = ${?GH128_RS_FILE_SET}
}
"#;

const CHILD_DEFAULT_PLUS_OPTIONAL_PKG_UNSET: &str = r#"
registry {
  instance-id = "localhost"
  instance-id = ${?GH128_RS_PKG_UNSET}
}
"#;

const CHILD_DEFAULT_PLUS_OPTIONAL_PKG_SET: &str = r#"
registry {
  instance-id = "localhost"
  instance-id = ${?GH128_RS_PKG_SET}
}
"#;

// Note on deferred-resolve coverage: rs.hocon's `Parser` (the
// package-registry entry point) does not currently expose a
// deferred-resolve variant — `Parser::parse_with_env` always runs both
// phases. The module-level `parse_string_with_options(... with_resolve_substitutions(false))`
// supports deferred resolution but does not thread a package registry.
// Pinning the deferred path through `include package(...)` therefore
// awaits a Parser API addition; the immediate-resolve coverage below
// is sufficient to pin the priorValues-through-merge invariant exercised
// by go.hocon#128.

#[test]
fn issue128_include_file_env_unset_preserves_prior_default() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(
        dir.path().join("child.conf"),
        CHILD_DEFAULT_PLUS_OPTIONAL_FILE_UNSET,
    )
    .unwrap();
    let input = format!("include \"{}/child.conf\"\n", dir_str);
    let env: HashMap<String, String> = HashMap::new();
    let cfg = hocon::parse_with_env(&input, &env).expect("parse must succeed");
    let got = cfg
        .get_string("registry.instance-id")
        .expect("registry.instance-id missing — prior default must remain when ${?ENV} is unset");
    assert_eq!(got, "localhost");
}

#[test]
fn issue128_include_file_env_set_applies_env_value() {
    let dir = tempdir().unwrap();
    let dir_str = dir.path().display().to_string().replace('\\', "/");
    std::fs::write(
        dir.path().join("child.conf"),
        CHILD_DEFAULT_PLUS_OPTIONAL_FILE_SET,
    )
    .unwrap();
    let input = format!("include \"{}/child.conf\"\n", dir_str);
    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("GH128_RS_FILE_SET".into(), "from-env".into());
    let cfg = hocon::parse_with_env(&input, &env).expect("parse must succeed");
    let got = cfg
        .get_string("registry.instance-id")
        .expect("registry.instance-id missing");
    assert_eq!(got, "from-env");
}

#[test]
fn issue128_include_package_env_unset_preserves_prior_default() {
    let parser = Parser::new().register_package(
        "github.com/o3co/rs.hocon/test/issue128-unset",
        "reference.conf",
        CHILD_DEFAULT_PLUS_OPTIONAL_PKG_UNSET,
    );
    let env: HashMap<String, String> = HashMap::new();
    let cfg = parser
        .parse_with_env(
            r#"include package("github.com/o3co/rs.hocon/test/issue128-unset", "reference.conf")"#,
            &env,
        )
        .expect("parse must succeed");
    let got = cfg
        .get_string("registry.instance-id")
        .expect("registry.instance-id missing — prior default must remain when ${?ENV} is unset");
    assert_eq!(got, "localhost");
}

#[test]
fn issue128_include_package_env_set_applies_env_value() {
    let parser = Parser::new().register_package(
        "github.com/o3co/rs.hocon/test/issue128-set",
        "reference.conf",
        CHILD_DEFAULT_PLUS_OPTIONAL_PKG_SET,
    );
    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("GH128_RS_PKG_SET".into(), "from-pkg-env".into());
    let cfg = parser
        .parse_with_env(
            r#"include package("github.com/o3co/rs.hocon/test/issue128-set", "reference.conf")"#,
            &env,
        )
        .expect("parse must succeed");
    let got = cfg
        .get_string("registry.instance-id")
        .expect("registry.instance-id missing");
    assert_eq!(got, "from-pkg-env");
}
