// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

use hocon::error::NotResolvedError;
use std::error::Error;

#[test]
fn not_resolved_error_display_contains_path() {
    let e = NotResolvedError { path: "foo.bar".into() };
    let msg = format!("{}", e);
    assert!(
        msg.contains("foo.bar"),
        "Display must include path; got: {}",
        msg
    );
    assert!(
        msg.contains("not resolved") || msg.contains("unresolved"),
        "Display must mention resolution state; got: {}",
        msg
    );
}

#[test]
fn not_resolved_error_implements_error() {
    let e = NotResolvedError { path: "a.b.c".into() };
    let _: &dyn Error = &e;  // must compile — impl Error
}

#[test]
fn not_resolved_error_source_is_none() {
    let e = NotResolvedError { path: "x".into() };
    assert!(e.source().is_none(), "source must be None (no underlying cause)");
}

#[test]
fn not_resolved_error_debug() {
    let e = NotResolvedError { path: "p".into() };
    let _ = format!("{:?}", e);  // must not panic
}

#[test]
fn hocon_error_wraps_not_resolved() {
    use hocon::HoconError;
    let e = NotResolvedError { path: "x.y".into() };
    let wrapped = HoconError::NotResolved(e);
    let msg = format!("{}", wrapped);
    assert!(msg.contains("x.y"), "HoconError::NotResolved display must include path");
}
