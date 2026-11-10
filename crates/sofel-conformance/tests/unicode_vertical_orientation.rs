//! sofel-unicode's Vertical_Orientation against `VerticalOrientation.txt`
//! itself (ADR 0019). HarfBuzz has no such property, so the file is read
//! here by a reader written apart from tools/gen-unicode/, and every code
//! point must match.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use sofel_conformance::paths;
use sofel_unicode::{VerticalOrientation, vertical_orientation};

fn value(s: &str) -> VerticalOrientation {
    match s {
        "U" => VerticalOrientation::U,
        "R" => VerticalOrientation::R,
        "Tu" => VerticalOrientation::Tu,
        "Tr" => VerticalOrientation::Tr,
        _ => panic!("unknown Vertical_Orientation {s:?}"),
    }
}

/// `first..last ; value` (or `first ; value`), without the comment.
fn line(s: &str) -> (u32, u32, VerticalOrientation) {
    let (cps, v) = s.split_once(';').expect("no ';'");
    let (first, last) = cps
        .trim()
        .split_once("..")
        .unwrap_or((cps.trim(), cps.trim()));
    let hex = |h: &str| u32::from_str_radix(h, 16).expect("bad code point");
    (hex(first), hex(last), value(v.trim()))
}

#[test]
fn every_code_point_matches_the_ucd_file() {
    let path = paths::ucd().join("VerticalOrientation.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}\nRun:\n  scripts/fetch-ucd.sh", path.display()));
    let mut expected = vec![None; 0x110000];
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix("# @missing:") {
            let (first, last, v) = line(rest);
            expected[first as usize..=last as usize]
                .iter_mut()
                .filter(|e| e.is_none())
                .for_each(|e| *e = Some(v));
            continue;
        }
        let data = l.split('#').next().unwrap().trim();
        if !data.is_empty() {
            let (first, last, v) = line(data);
            expected[first as usize..=last as usize].fill(Some(v));
        }
    }
    let mismatches: Vec<String> = (0..0x110000u32)
        .filter_map(|cp| {
            let want = expected[cp as usize].expect("a code point without a value");
            let got = vertical_orientation(cp);
            (got != want).then(|| format!("U+{cp:04X}: {got:?}, expected {want:?}"))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} mismatches, first: {:?}",
        mismatches.len(),
        &mismatches[..mismatches.len().min(10)]
    );
    for cp in [0x110000, 0x7FFF_FFFF, u32::MAX] {
        assert_eq!(vertical_orientation(cp), VerticalOrientation::R);
    }
}
