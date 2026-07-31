//! Bounds on how deep a config tree may go.
//!
//! Two different limits, because two different things can be too deep, and
//! because Rust leaves us no third option for either.
//!
//! [`MAX_PATH_SEGMENTS`] bounds a *name* that maps to a path — an environment
//! variable's `__` segments, a Properties file's dotted key. One name produces
//! one arbitrarily deep chain, so the input needed to exhaust the stack is a
//! single long string. ts.hocon and py.hocon cap the same mapping at the same
//! number, so a name that mounts in one implementation mounts in the other.
//!
//! [`MAX_DOCUMENT_DEPTH`] bounds nesting in the document itself. The sibling
//! implementations do *not* cap this: they catch the interpreter's or engine's
//! own error (`RecursionError`, `RangeError`) and rethrow it as their own type,
//! so a deep document fails cleanly without anyone having to name a number.
//! Rust has no equivalent — a stack overflow is `SIGABRT`, which no
//! `catch_unwind` contains and which takes the caller's whole process with it.
//! Refusing before the stack runs out is the only way to give the caller
//! something to handle.
//!
//! # Why 128
//!
//! Measured, not guessed:
//!
//! - The limit has to hold on a **2 MiB stack**, which is what Rust gives a
//!   spawned thread — a library used from a request handler gets that, not the
//!   main thread's 8 MiB. On 2 MiB this parser survives depth 600 and aborts by
//!   1000, so a usable cap sits well under 600, with room left for the resolver
//!   pass that walks the built tree again.
//! - **`serde_json` refuses at 128**, and this crate's own `adapters::jsonc`
//!   has therefore been rejecting documents deeper than 127 since it shipped.
//!   JSON is a subset of HOCON, so a HOCON document deeper than that already
//!   could not round-trip through this crate's own JSONC adapter. Picking a
//!   different number for the core would mean the same crate enforced two.
//! - Every fixture in the shared xx.hocon corpus is at most **8** levels deep,
//!   apart from one adversarial file (`max_depth.conf`, 1728) harvested
//!   specifically to probe this. The margin over real configs is three orders
//!   of magnitude.
//!
//! Capping the parse also bounds the tree, which matters twice over: the
//! resolver's walk and `HoconValue`'s own `Drop` are both recursive, so an
//! uncapped depth could abort during teardown even after a successful parse.

/// Ceiling on the number of path segments one name may map to (F1.2 for env,
/// S23.x for Properties). Matches ts.hocon and py.hocon.
///
/// Only the env adapter reads it, and that adapter is feature-gated, so a
/// default build sees it unused.
#[cfg_attr(not(feature = "adapters-env"), allow(dead_code))]
pub(crate) const MAX_PATH_SEGMENTS: usize = 64;

/// Ceiling on object/array nesting in a document. See the module docs for why
/// this crate caps what the siblings catch, and why the number is 128.
pub(crate) const MAX_DOCUMENT_DEPTH: usize = 128;
