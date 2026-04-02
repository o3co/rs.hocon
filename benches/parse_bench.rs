// Copyright 2026 1o1 Co. Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Generate a flat/shallow HOCON config string with `total_keys` keys spread
/// across objects up to `max_depth` levels deep.
fn generate_config(total_keys: usize, max_depth: usize) -> String {
    let mut buf = String::new();
    for i in 0..total_keys {
        let depth = if max_depth == 0 { 0 } else { i % max_depth };
        let segments: Vec<String> = (0..=depth).map(|d| format!("level{d}")).collect();
        let path = segments.join(".");
        buf.push_str(&format!("{path}.key{i} = \"value{i}\"\n"));
    }
    buf
}

/// Generate a HOCON string with `count` base keys followed by `count`
/// substitution keys that reference them via `${}`.
fn generate_with_substitutions(count: usize) -> String {
    let mut buf = String::new();
    for i in 0..count {
        buf.push_str(&format!("base.key{i} = \"val{i}\"\n"));
    }
    for i in 0..count {
        buf.push_str(&format!("sub.key{i} = ${{base.key{i}}}\n"));
    }
    buf
}

/// Generate a deeply nested HOCON object with `depth` levels and 5 leaf keys
/// at the innermost level.
fn generate_deep_nested(depth: usize) -> String {
    let mut buf = String::new();
    // Open nested objects
    for d in 0..depth {
        let indent = "  ".repeat(d);
        buf.push_str(&format!("{indent}nest{d} {{\n"));
    }
    // Leaf keys
    let indent = "  ".repeat(depth);
    for k in 0..5 {
        buf.push_str(&format!("{indent}leaf{k} = \"deep_value{k}\"\n"));
    }
    // Close nested objects
    for d in (0..depth).rev() {
        let indent = "  ".repeat(d);
        buf.push_str(&format!("{indent}}}\n"));
    }
    buf
}

// ---------------------------------------------------------------------------
// Benchmark group: Config size
// ---------------------------------------------------------------------------

fn bench_config_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_size");

    let small = generate_config(10, 2);
    let medium = generate_config(100, 3);
    let large = generate_config(1000, 4);

    group.bench_function("parse_small_10", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&small)).unwrap();
            black_box(config.get_string("level0.key0").unwrap());
        });
    });

    group.bench_function("parse_medium_100", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&medium)).unwrap();
            black_box(config.get_string("level0.key0").unwrap());
        });
    });

    group.bench_function("parse_large_1000", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&large)).unwrap();
            black_box(config.get_string("level0.key0").unwrap());
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark group: Substitutions
// ---------------------------------------------------------------------------

fn bench_substitutions(c: &mut Criterion) {
    let mut group = c.benchmark_group("substitutions");

    let sub10 = generate_with_substitutions(10);
    let sub50 = generate_with_substitutions(50);
    let sub100 = generate_with_substitutions(100);

    group.bench_function("substitutions_10", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&sub10)).unwrap();
            black_box(config.get_string("sub.key0").unwrap());
        });
    });

    group.bench_function("substitutions_50", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&sub50)).unwrap();
            black_box(config.get_string("sub.key0").unwrap());
        });
    });

    group.bench_function("substitutions_100", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&sub100)).unwrap();
            black_box(config.get_string("sub.key0").unwrap());
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark group: Deep nesting
// ---------------------------------------------------------------------------

fn bench_deep_nesting(c: &mut Criterion) {
    let mut group = c.benchmark_group("deep_nesting");

    let nest5 = generate_deep_nested(5);
    let nest10 = generate_deep_nested(10);
    let nest20 = generate_deep_nested(20);

    // Build the path to the deepest leaf for each depth.
    let path5: String = (0..5)
        .map(|d| format!("nest{d}"))
        .collect::<Vec<_>>()
        .join(".")
        + ".leaf0";
    let path10: String = (0..10)
        .map(|d| format!("nest{d}"))
        .collect::<Vec<_>>()
        .join(".")
        + ".leaf0";
    let path20: String = (0..20)
        .map(|d| format!("nest{d}"))
        .collect::<Vec<_>>()
        .join(".")
        + ".leaf0";

    group.bench_function("deep_nest_5", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&nest5)).unwrap();
            black_box(config.get_string(&path5).unwrap());
        });
    });

    group.bench_function("deep_nest_10", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&nest10)).unwrap();
            black_box(config.get_string(&path10).unwrap());
        });
    });

    group.bench_function("deep_nest_20", |b| {
        b.iter(|| {
            let config = hocon::parse(black_box(&nest20)).unwrap();
            black_box(config.get_string(&path20).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_config_size,
    bench_substitutions,
    bench_deep_nesting
);
criterion_main!(benches);
