//! A non-UTF-8 environment entry must not panic any public entry point.
//!
//! `std::env::vars()` panics *while iterating* if any entry's name or value is
//! not valid UTF-8, so every entry point that inherits the process environment
//! used to panic as soon as such an entry existed — however unrelated to the
//! config being parsed. The crate now reads the environment via `vars_os` and
//! skips non-UTF-8 entries: a non-UTF-8 name can never be referenced from
//! UTF-8 HOCON source, and a skipped value makes `${?VAR}` resolve as
//! undefined instead of handing over silently mangled text.
//!
//! The entry has to exist in the *real* environment, and planting one
//! in-process would poison every concurrently running test, so the test
//! re-execs itself as a child process with the bad entries injected.
#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

const CHILD_MARKER: &str = "HOCON_NON_UTF8_ENV_CHILD";

#[test]
fn public_api_survives_non_utf8_environment_entries() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        child();
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    let output = Command::new(exe)
        .args([
            "public_api_survives_non_utf8_environment_entries",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        // A name that is not UTF-8 …
        .env(OsString::from_vec(b"HOCON_BAD_NAME_\xFF".to_vec()), "x")
        // … and a value that is not UTF-8.
        .env("HOCON_BAD_VALUE", OsString::from_vec(b"\xFF".to_vec()))
        // Entries the env adapter should still see (and skip) under a prefix.
        .env("NUTF8_GOOD", "ok")
        .env("NUTF8_BAD", OsString::from_vec(b"\xFF".to_vec()))
        .output()
        .expect("spawn child");

    assert!(
        output.status.success(),
        "child failed ({})\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Runs in the child, where the environment really contains non-UTF-8 entries.
fn child() {
    // The fused parse path collects the process environment.
    let cfg = hocon::parse("a = 1").expect("parse must not panic");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);

    // A variable whose value was skipped behaves like an unset variable.
    let cfg = hocon::parse("b = ${?HOCON_BAD_VALUE}").expect("parse must not panic");
    assert!(
        cfg.get("b").is_none(),
        "skipped value must read as undefined"
    );

    // The deferred path collects the environment at resolve time instead.
    let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
    let cfg = hocon::parse_string_with_options("c = ${?NUTF8_GOOD}", opts)
        .expect("deferred parse must not panic");
    let resolved = cfg
        .resolve(hocon::ResolveOptions::defaults())
        .expect("resolve must not panic");
    assert_eq!(resolved.get_string("c").unwrap(), "ok");

    #[cfg(feature = "adapters-env")]
    {
        let cfg = hocon::adapters::env::load(hocon::adapters::env::Options {
            prefix: "NUTF8_".into(),
            ..Default::default()
        })
        .expect("env::load must not panic");
        assert_eq!(cfg.get_string("good").unwrap(), "ok");
        assert!(
            cfg.get("bad").is_none(),
            "the non-UTF-8 value entry must be skipped, not mangled"
        );
    }
}
