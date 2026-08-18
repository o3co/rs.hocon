// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

#![cfg(feature = "serde")]

//! E18 — the shared emitter round-trip corpus (xx.hocon
//! `testdata/emitter-roundtrip/`, synced into `tests/testdata/emitter-roundtrip/`
//! by `make testdata`). Each fixture is a JSON value tree; the contract is
//! `parse(render(tree)) == tree`, compared as canonical trees, never as text.
//! See xx.hocon docs/extra-spec-conventions.md §E18.

use std::path::PathBuf;

#[test]
fn e18_emitter_roundtrip_corpus() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("testdata")
        .join("emitter-roundtrip");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => {
            eprintln!("skip: emitter-roundtrip corpus not synced — run `make testdata`");
            return;
        }
    };
    let mut ran = 0;
    for entry in entries {
        let path = entry.expect("read_dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        ran += 1;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("fixture file name is UTF-8")
            .to_owned();
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: read: {e}"));
        // serde_json::Map keeps the fixture's number tokens exact within the
        // corpus' 2^53 bound (integers land as i64, floats as f64) — the same
        // split `from_map` expects (spec F0.5).
        let tree: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{name}: fixture is not a JSON object: {e}"));
        let cfg =
            hocon::from_map(tree, Some(&name)).unwrap_or_else(|e| panic!("{name}: from_map: {e}"));
        let before = hocon::_render_json_for_test(&cfg);
        let text = cfg
            .render_hocon()
            .unwrap_or_else(|e| panic!("{name}: render_hocon: {e}"));
        let reparsed = hocon::parse(&text).unwrap_or_else(|e| {
            panic!("{name}: re-parse of emitted HOCON failed: {e}\n--- emitted ---\n{text}")
        });
        let after = hocon::_render_json_for_test(&reparsed);
        assert_eq!(
            before, after,
            "{name}: round trip changed the tree\n--- emitted ---\n{text}"
        );
    }
    assert!(ran > 0, "corpus directory exists but holds no fixtures");
}
