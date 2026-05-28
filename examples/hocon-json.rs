//! rs.hocon adapter for the cross-impl differential harness
//! (xx.hocon/generate). Parses+resolves a HOCON file and prints the resolved
//! tree as canonical JSON to stdout. On any parse/resolve error it prints a
//! single-line `{"__error__":{"type":..,"message":..}}` record to stdout and
//! exits 3, so the differential driver can compare error-vs-success behaviour
//! uniformly across go/rs/ts and the Lightbend oracle.
//!
//! Usage: `cargo run --example hocon-json -- <conf-file>`
//!
//! Environment substitutions resolve against the process environment, so the
//! driver controls hermeticity by clearing/setting the subprocess env.

use std::process::exit;

const EXIT_ERROR: i32 = 3;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: hocon-json <conf-file>");
        exit(2);
    }
    match hocon::parse_file(&args[1]) {
        Ok(cfg) => println!("{}", hocon::_render_json_for_test(&cfg)),
        Err(e) => {
            // Enum variant name (first token of Debug) as a coarse error type;
            // the driver mainly distinguishes success-vs-error.
            let dbg = format!("{e:?}");
            let ty = dbg
                .split(|c| c == '(' || c == '{' || c == ' ')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("HoconError");
            println!(
                "{{\"__error__\":{{\"type\":\"{}\",\"message\":\"{}\"}}}}",
                json_escape(ty),
                json_escape(&e.to_string())
            );
            exit(EXIT_ERROR);
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
