//! sofel-shape's syllable machines against HarfBuzz's (ADR 0024): for
//! each machine, every sequence of its categories of length 1 to 4 and
//! a million pseudo-random sequences of length 5 to 32 must be split
//! into the same syllables. There is no list of expected failures.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use sofel_conformance::syllables;

/// The random sequences per machine and their seed.
const COUNT: u32 = 1_000_000;
const SEED: u32 = 1;

#[test]
fn alphabet_is_the_exports_and_one_more() {
    let rl = "export X = 0;\nexport C = 1; # c\nexport A = 9;\nexport VD = 9;\n";
    assert_eq!(syllables::alphabet(rl), vec![0, 1, 9, 2]);
}

#[test]
fn syllables_agree_with_harfbuzz() {
    let items: Vec<_> = syllables::MACHINES
        .iter()
        .map(|m| syllables::compare(m, COUNT, SEED).unwrap_or_else(|e| panic!("{m}: {e}")))
        .collect();
    for i in &items {
        eprintln!(
            "  {}: {} ({})",
            i.id,
            if i.ok { "ok" } else { "FAILED" },
            i.detail.lines().next().unwrap_or("")
        );
    }
    let failed: Vec<String> = items
        .iter()
        .filter(|i| !i.ok)
        .map(|i| format!("{}: {}", i.id, i.detail))
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
