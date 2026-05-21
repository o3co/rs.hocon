// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! YAML scenario types for the E12 cross-impl deferred-resolution fixture runner.
//!
//! Schema reference:
//!   repos/xx.hocon/testdata/hocon/deferred-resolution/README.md

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Scenario {
    #[allow(dead_code)]
    pub description: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub xref: Vec<String>,
    #[serde(default, rename = "lightbendSkip")]
    #[allow(dead_code)]
    pub lightbend_skip: bool,
    pub sources: HashMap<String, Source>,
    pub build: Vec<Step>,
    pub expect: Expect,
}

#[derive(Debug, Deserialize)]
pub struct Source {
    #[serde(default, rename = "parseString")]
    pub parse_string: Option<String>,
    #[serde(default, rename = "parseOptions")]
    pub parse_options: Option<ParseOpts>,
    #[serde(default, rename = "fromMap")]
    pub from_map: Option<serde_yaml::Value>,
    #[serde(default, rename = "originDescription")]
    pub origin_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ParseOpts {
    #[serde(default, rename = "resolveSubstitutions")]
    pub resolve_substitutions: Option<bool>,
    #[serde(default, rename = "originDescription")]
    pub origin_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Step {
    pub op: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, rename = "this")]
    pub this: Option<String>,
    #[serde(default)]
    pub other: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "allowUnresolved")]
    pub allow_unresolved: Option<bool>,
    #[serde(default, rename = "useSystemEnvironment")]
    pub use_system_environment: Option<bool>,
    #[serde(default, rename = "as")]
    pub r#as: String,
}

#[derive(Debug, Deserialize)]
pub struct Expect {
    pub outcome: String,
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default, rename = "isResolved")]
    pub is_resolved: Option<bool>,
    #[serde(default)]
    pub getter: Vec<GetterAssert>,
    #[serde(default, rename = "errorAt")]
    pub error_at: Option<usize>,
    #[serde(default, rename = "errorCategory")]
    pub error_category: Option<String>,
    #[serde(default, rename = "errorContains")]
    pub error_contains: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetterAssert {
    pub path: String,
    #[serde(default, rename = "expectString")]
    pub expect_string: Option<String>,
    #[serde(default, rename = "expectInt")]
    pub expect_int: Option<i64>,
    #[serde(default, rename = "expectBool")]
    pub expect_bool: Option<bool>,
    #[serde(default, rename = "expectError")]
    pub expect_error: Option<String>,
}
