//! A non-UTF-8 environment entry must not panic any public entry point, and
//! must not silently vanish from a namespace the caller explicitly mounted
//! (spec F1.9).
//!
//! `std::env::vars()` panics *while iterating* if any entry's name or value is
//! not valid UTF-8, so every entry point that inherits the process environment
//! used to panic as soon as such an entry existed — however unrelated to the
//! config being parsed. The crate now reads the environment via `vars_os`, and
//! what happens next depends on whether the caller asked for the variable:
//! substitution treats it as absent (F1.9a), a bulk mount refuses (F1.9b).
//!
//! The entry has to exist in the *real* environment, and planting one
//! in-process would poison every concurrently running test, so the test
//! re-execs itself as a child process with the bad entries injected.
//!
//! Unix only. On Windows an `OsString` can hold an unpaired surrogate, which
//! `into_string()` also rejects, so the same policy applies there — but
//! injecting one needs a different mechanism and the Windows CI row therefore
//! compiles this file to an empty binary. Deliberate, and recorded here so it
//! is a known gap rather than an oversight.
#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

const CHILD_MARKER: &str = "HOCON_NON_UTF8_ENV_CHILD";
const CHILD_NAME: &str = "public_api_survives_non_utf8_environment_entries";
/// The child prints this only after every assertion has run. The parent greps
/// for it because a test binary given a filter that matches nothing exits 0
/// with "0 passed; 1 filtered out" — so `status.success()` alone would turn a
/// renamed test into a silently green no-op, which is precisely the
/// empty-binary failure mode this branch exists to remove.
const CHILD_SENTINEL: &str = "__hocon_non_utf8_child_completed__";

#[test]
fn public_api_survives_non_utf8_environment_entries() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        child();
        println!("{CHILD_SENTINEL}");
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    let output = Command::new(exe)
        .args([CHILD_NAME, "--exact", "--nocapture"])
        .env(CHILD_MARKER, "1")
        // A name that is not UTF-8 …
        .env(OsString::from_vec(b"HOCON_BAD_NAME_\xFF".to_vec()), "x")
        // … and a value that is not UTF-8. Neither matches a mount prefix
        // used below, so neither may affect anything.
        .env("HOCON_BAD_VALUE", OsString::from_vec(b"\xFF".to_vec()))
        // A namespace whose every entry decodes …
        .env("GOOD_NAME", "ok")
        .env("GOOD_DB__HOST", "prod.example.com")
        // … and one holding an undecodable value, so a mount of GOOD_ proves
        // the blast radius is bounded while a mount of BAD_ must refuse.
        .env("BAD_DB__HOST", "prod.example.com")
        .env(
            "BAD_DB__PASSWORD",
            OsString::from_vec(b"s\xE9cret".to_vec()),
        )
        .output()
        .expect("spawn child");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "child failed ({})\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        stdout.contains(CHILD_SENTINEL),
        "child exited 0 without running its assertions — the filter {CHILD_NAME:?} \
         probably matches nothing.\nstdout:\n{stdout}",
    );
}

/// Runs in the child, where the environment really contains non-UTF-8 entries.
fn child() {
    // ── F1.9a: substitution treats an undecodable entry as absent ──────────
    //
    // The fused parse path collects the process environment.
    let cfg = hocon::parse("a = 1").expect("parse must not panic");
    assert_eq!(cfg.get_i64("a").unwrap(), 1);

    // A variable whose value was skipped behaves like an unset variable, so
    // `${?VAR}` falls through rather than yielding mangled text.
    let cfg = hocon::parse("b = ${?HOCON_BAD_VALUE}").expect("parse must not panic");
    assert!(
        cfg.get("b").is_none(),
        "skipped value must read as undefined"
    );

    let cfg = hocon::parse("b = \"dflt\"\nb = ${?HOCON_BAD_VALUE}").expect("parse must not panic");
    assert_eq!(
        cfg.get_string("b").unwrap(),
        "dflt",
        "an undecodable value must not overwrite a default"
    );

    // The deferred path collects the environment at resolve time instead.
    let opts = hocon::ParseOptions::defaults().with_resolve_substitutions(false);
    let cfg = hocon::parse_string_with_options("c = ${?GOOD_NAME}", opts)
        .expect("deferred parse must not panic");
    let resolved = cfg
        .resolve(hocon::ResolveOptions::defaults())
        .expect("resolve must not panic");
    assert_eq!(resolved.get_string("c").unwrap(), "ok");

    #[cfg(feature = "adapters-env")]
    {
        use hocon::adapters::env;

        fn mount(prefix: &str) -> Result<hocon::Config, hocon::adapters::AdapterError> {
            env::load(env::Options {
                prefix: prefix.into(),
                ..Default::default()
            })
        }

        // ── F1.9b: a bulk mount refuses an undecodable entry it was asked
        // for, rather than handing back a subtree that looks complete while
        // the operator's password is missing and a stale default wins ──────
        let err = mount("BAD_").expect_err("a bulk mount must refuse an undecodable entry (F1.9b)");
        assert!(
            err.message.contains("BAD_DB__PASSWORD") && err.message.contains("F1.9"),
            "{}",
            err.message
        );

        // ── the blast radius is bounded to the prefix ──────────────────────
        //
        // BAD_DB__PASSWORD, HOCON_BAD_NAME_\xFF and HOCON_BAD_VALUE are all
        // still set and all undecodable. None matches GOOD_, so none may
        // affect this mount — that is what stops F1.9b from resurrecting the
        // abort-on-an-unrelated-entry bug this file was written for.
        let cfg = mount("GOOD_").expect("an undecodable entry outside the prefix must be ignored");
        assert_eq!(cfg.get_string("name").unwrap(), "ok");
        assert_eq!(cfg.get_string("db.host").unwrap(), "prod.example.com");
    }
}
