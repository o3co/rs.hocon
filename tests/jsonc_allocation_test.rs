//! Peak-allocation guard for the JSONC strip passes (issue #155).
//!
//! The issue is a memory bound, so the regression test has to be one too — a
//! functional test cannot tell 3x from 8x. This file installs a counting
//! global allocator, which is why it is a separate integration binary: the
//! counter is process-wide, so anything else running in the same process would
//! be counted too. **Keep exactly one test here.**
#![cfg(feature = "adapters")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use hocon::adapters::jsonc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

struct Tracking;

fn record(delta: isize) {
    let live = if delta >= 0 {
        LIVE.fetch_add(delta as usize, Ordering::Relaxed) + delta as usize
    } else {
        let d = delta.unsigned_abs();
        LIVE.fetch_sub(d, Ordering::Relaxed) - d
    };
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every method forwards to `System` unchanged and only adds
// bookkeeping around it, so the allocator contract is whatever `System`'s is.
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            record(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        record(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            record(new_size as isize - layout.size() as isize);
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Tracking = Tracking;

/// A document whose *text* is large but whose *tree* is small: a few keys, one
/// of them holding a very long string, plus a minority of comments.
///
/// Both halves of that shape are load-bearing, and getting either wrong makes
/// the measurement meaningless:
///
/// - **Mostly content, not mostly comment.** A comment-heavy document shrinks
///   during stripping, so the second pass runs on a fraction of the input. An
///   early version of this test did that and reported 1.07x.
/// - **Few nodes, not many.** With many small keys the peak is dominated by
///   `serde_json`'s `Value` plus the `Config` tree — measured at roughly 4.5x
///   on its own — which swamps the strip passes entirely: old and new came out
///   at 5.8x and 5.5x, a difference this test could not have defended. One
///   long string keeps the tree at about two copies of the text and leaves the
///   strip passes as the dominant term.
fn document(bytes: usize) -> String {
    let body = bytes * 9 / 10;
    let mut s = String::with_capacity(bytes + 256);
    s.push_str("{\n  /* a comment, with a \"quoted\" marker /* inside it */\n");
    s.push_str("  // a line comment, and a trailing comma below,\n");
    s.push_str("  \"long\": \"");
    while s.len() < body {
        // An escaped quote every so often, so `end_of_string`'s escape branch
        // is on the measured path rather than only the functional one.
        s.push_str("filler text that survives stripping, with an escaped \\\" quote; ");
    }
    s.push_str("\",\n");
    while s.len() < bytes {
        s.push_str("  /* trailing filler comment */\n");
    }
    s.push_str("  \"last\": [1, 2, 3,],\n}\n");
    s
}

/// `parse` must stay within a small multiple of the input.
///
/// Before the `char_indices` rewrite each pass built a `Vec<char>` — 4 bytes
/// per character on top of the text it was built from — so the second pass
/// peaked at roughly 6n over its input. Slicing the `&str` directly leaves one
/// intermediate `String` and the output.
///
/// Measured here on a 1 MiB document, peak over baseline (baseline already
/// holds the input, so this is everything `parse` adds):
///
/// ```text
/// Vec<char>      5.6x     ← fails this guard
/// char_indices   3.45x    ← passes
/// ```
///
/// The remainder in both figures is `serde_json`'s `Value` and the `Config`
/// tree, about 1.8x here and not something this change touches.
///
/// The bound is 4.5x: below the old implementation, comfortably above the new
/// one, so reintroducing a `Vec<char>` in either pass fails while an allocator
/// or `serde_json` change that shifts the constant does not.
#[test]
fn parse_stays_within_a_small_multiple_of_the_input() {
    let src = document(1 << 20);
    let n = src.len();

    // Warm up: the first parse initialises whatever is lazily allocated
    // (serde_json scratch, formatting machinery) and would otherwise be
    // charged to the measured run.
    drop(jsonc::parse(&src, None).expect("warm-up parse"));

    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);

    let cfg = jsonc::parse(&src, None).expect("measured parse");
    let peak = PEAK.load(Ordering::Relaxed);
    drop(cfg);

    let over = peak.saturating_sub(baseline);
    let multiple = over as f64 / n as f64;
    assert!(
        over * 2 < n * 9,
        "peak was {over} bytes over baseline for a {n}-byte document ({multiple:.2}x); \
         the guard allows up to 4.5x"
    );
    // Print for the record; `cargo test -- --nocapture` shows it.
    println!("peak {over} bytes over baseline for {n} bytes in ({multiple:.2}x)");
}
