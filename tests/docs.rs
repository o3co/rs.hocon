//! The README states facts that go stale on their own: the compliance rates,
//! the MSRV, and "unreleased" markers left behind after the release that
//! shipped the behavior. Nothing else exercises them, so they only ever drift
//! in one direction. These tests recompute each from the artifact that actually
//! decides it — `docs/spec-compliance.md` and `Cargo.toml` — and run in the
//! publish workflow, so a stale README fails the cut.

use std::fs;

/// Number of S-items in the shared spec checklist
/// (xx.hocon/docs/spec-checklist.md). Both compliance rates use it as the
/// denominator, so a per-impl doc that drifted away from the checklist would
/// silently change the rates; the count is asserted rather than derived.
const SPEC_ITEM_TOTAL: usize = 210;

#[derive(Default)]
struct ComplianceCounts {
    pass: usize,
    partial: usize,
    fail: usize,
    unverified: usize,
    out_of_scope: usize,
}

impl ComplianceCounts {
    fn total(&self) -> usize {
        self.pass + self.partial + self.fail + self.unverified + self.out_of_scope
    }

    /// Rate to one decimal, matching how the README writes it.
    fn rate(&self, denominator: usize) -> String {
        let scored = self.pass as f64 + self.partial as f64 * 0.5;
        format!("{:.1}", scored / denominator as f64 * 100.0)
    }
}

fn read(relative: &str) -> String {
    fs::read_to_string(relative).unwrap_or_else(|e| panic!("read {relative}: {e}"))
}

/// True for an S-item heading: `- **S13a.10** Some rule — §Section (L123)`.
/// E-items (extra-spec conventions) use the same block shape but are not part
/// of the checklist, so the heading is what separates them.
fn spec_item_head(line: &str) -> bool {
    bold_head(line).is_some_and(|id| id.starts_with('S'))
}

fn other_head(line: &str) -> bool {
    bold_head(line).is_some_and(|id| !id.starts_with('S'))
}

/// Returns the bold identifier of a `- **ID** …` bullet, if the line is one.
/// An identifier contains no whitespace, which is what keeps ordinary prose
/// bullets like `- **Value-start `-`** …` from being read as item headings.
fn bold_head(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("- **")?;
    let id = rest.split_once("**")?.0;
    if id.is_empty() || id.contains(char::is_whitespace) {
        return None;
    }
    Some(id)
}

/// Tallies the status glyph of every S-item block in `docs/spec-compliance.md`.
/// Only the first `status:` line after an S-item heading counts: a block may
/// carry sub-bullets, and the E-item blocks that follow the S-items must not be
/// picked up.
fn count_compliance() -> ComplianceCounts {
    let doc = read("docs/spec-compliance.md");
    let mut counts = ComplianceCounts::default();
    let mut in_spec_item = false;

    for line in doc.lines() {
        if spec_item_head(line) {
            in_spec_item = true;
        } else if other_head(line) {
            in_spec_item = false;
        } else if in_spec_item && line.trim_start().starts_with("status:") {
            in_spec_item = false;
            if line.contains('✅') {
                counts.pass += 1;
            } else if line.contains('⚠') {
                counts.partial += 1;
            } else if line.contains('❌') {
                counts.fail += 1;
            } else if line.contains('🤷') {
                counts.unverified += 1;
            } else if line.contains('➖') {
                counts.out_of_scope += 1;
            } else {
                panic!("status line carries no known glyph: {}", line.trim());
            }
        }
    }
    counts
}

/// Returns the text between `prefix` and `suffix` on the first line that has
/// both, panicking when no line does — a README rewrite that drops the claim
/// must fail loudly rather than silently stop checking it.
fn find_between(text: &str, prefix: &str, suffix: &str, what: &str) -> String {
    text.lines()
        .find_map(|line| {
            let after = line.split_once(prefix)?.1;
            Some(after.split_once(suffix)?.0.trim().to_string())
        })
        .unwrap_or_else(|| {
            panic!("{what} not found (looked for {prefix:?} … {suffix:?}); update the pattern if the doc was restructured")
        })
}

/// Returns the value cell of the Markdown table row whose label cell is
/// `label`, panicking when there is no such row — same reasoning as
/// [`find_between`].
fn table_value(text: &str, label: &str, what: &str) -> String {
    text.lines()
        .find_map(|line| {
            let mut cells = line.trim().strip_prefix('|')?.split('|');
            if cells.next()?.trim() != label {
                return None;
            }
            Some(cells.next()?.trim().to_string())
        })
        .unwrap_or_else(|| {
            panic!("{what} not found (no table row labelled {label:?}); update the label if the doc was restructured")
        })
}

#[test]
fn spec_compliance_covers_every_checklist_item() {
    let counts = count_compliance();
    assert_eq!(
        counts.total(),
        SPEC_ITEM_TOTAL,
        "docs/spec-compliance.md item count — an item was added, dropped, or its status line is malformed"
    );
    assert_eq!(
        counts.unverified, 0,
        "every item must be pinned by a test, none may stay 🤷"
    );
}

#[test]
fn readme_compliance_rates_match_the_per_item_doc() {
    let counts = count_compliance();
    let readme = read("README.md");

    let spec_total = table_value(
        &readme,
        "Spec total (incl. out-of-scope)",
        "spec-total compliance rate",
    );
    let in_scope = table_value(&readme, "In-scope only", "in-scope compliance rate");

    assert_eq!(
        spec_total,
        format!("**{}%**", counts.rate(SPEC_ITEM_TOTAL)),
        "README spec-total rate vs docs/spec-compliance.md"
    );
    assert_eq!(
        in_scope,
        format!(
            "**{}%**",
            counts.rate(SPEC_ITEM_TOTAL - counts.out_of_scope)
        ),
        "README in-scope rate vs docs/spec-compliance.md"
    );
}

#[test]
fn readme_msrv_matches_cargo_toml() {
    let declared = find_between(
        &read("Cargo.toml"),
        "rust-version = \"",
        "\"",
        "rust-version",
    );
    let claimed = find_between(&read("README.md"), "The MSRV is **", "**", "README MSRV");
    assert_eq!(
        claimed, declared,
        "README MSRV vs Cargo.toml rust-version — a user on the version the README names cannot build this crate"
    );
}

#[test]
fn readme_marks_nothing_as_unreleased() {
    let offenders: Vec<String> = read("README.md")
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("(Unreleased)"))
        .map(|(i, line)| format!("README.md:{}: {}", i + 1, line.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "shipped behavior is still marked unreleased:\n{}",
        offenders.join("\n")
    );
}
