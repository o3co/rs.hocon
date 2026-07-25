// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! The single place this crate reads the process environment (spec F1.9).
//!
//! `std::env::vars()` panics *while iterating* if any entry's name or value is
//! not valid UTF-8 — which on Unix any parent process can arrange — so
//! collecting it made every entry point that inherits the environment abort on
//! an entry the config never mentions. Everything here goes through
//! [`std::env::vars_os`] instead, and what happens to an undecodable entry
//! depends on whether the caller asked for it:
//!
//! - [`vars`] — **substitution lookup** (F1.9a). Undecodable entries are
//!   absent, so `${?VAR}` falls through to its default and `${VAR}` raises the
//!   ordinary unresolved error. Never lossy or replacement-character text.
//!   Nobody's working config changes meaning as a result: before this existed,
//!   a single undecodable entry *anywhere* in the environment crashed the
//!   parse regardless of which variables the document named, so the prior
//!   behaviour for every affected user was an abort, not a working parse.
//! - [`entries`] — **bulk mount** (F1.9b). The caller named a whole namespace,
//!   so `adapters::env` must be able to tell a decode failure from an absent
//!   variable and refuse rather than hand back a subtree that looks complete
//!   while the operator's setting is missing.

use std::ffi::OsString;

/// The process environment as UTF-8 pairs, skipping entries that are not
/// (F1.9a). This is what substitution resolution consumes.
///
/// Deliberately not written in terms of [`entries`]: that type carries the raw
/// name bytes a bulk mount needs for prefix matching, and only the adapter
/// wants them, so keeping the two apart avoids a feature-shaped struct.
pub(crate) fn vars() -> impl Iterator<Item = (String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((os_into_string(name)?, os_into_string(value)?)))
}

/// One process-environment entry, decoded where possible.
///
/// `name_bytes` is always present so a caller can prefix-match and name the
/// offender in a diagnostic even when the name itself is not UTF-8.
#[cfg(feature = "adapters-env")]
pub(crate) struct Entry {
    /// The decoded name, or `None` when it is not valid UTF-8.
    pub(crate) name: Option<String>,
    /// The decoded value, or `None` when it is not valid UTF-8.
    pub(crate) value: Option<String>,
    /// The raw name. ASCII bytes survive `as_encoded_bytes` unchanged on every
    /// platform, which is all prefix matching needs.
    pub(crate) name_bytes: Vec<u8>,
}

#[cfg(feature = "adapters-env")]
impl Entry {
    /// The name for a human-readable message. Lossy on purpose: this is
    /// diagnostic text, never data that reaches the config.
    pub(crate) fn display_name(&self) -> String {
        match &self.name {
            Some(n) => n.clone(),
            None => String::from_utf8_lossy(&self.name_bytes).into_owned(),
        }
    }
}

/// Every entry in the process environment, decoded where possible (F1.9b).
#[cfg(feature = "adapters-env")]
pub(crate) fn entries() -> impl Iterator<Item = Entry> {
    std::env::vars_os().map(|(name, value)| Entry {
        name_bytes: name.as_encoded_bytes().to_vec(),
        name: os_into_string(name),
        value: os_into_string(value),
    })
}

fn os_into_string(s: OsString) -> Option<String> {
    s.into_string().ok()
}
