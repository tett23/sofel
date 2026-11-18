//! ADR 0091 A1: the MATH table's acceptance (`hb_ot_math_has_data`) and
//! every glyph's extended shape (`hb_ot_math_is_glyph_extended_shape`)
//! agree with `hb-dump --math` on every test font with a MATH table, on
//! copies of them whose MathConstants, MathGlyphInfo or MathVariants
//! offset points out of the table, and on the boundary tables of the
//! ADR's invariant table.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "../../sofel-font/tests/support/math_acceptance_fixture.rs"]
mod fixture;

use std::path::{Path, PathBuf};
use std::process::Command;

use sofel_conformance::compare::Item;
use sofel_conformance::math::{HbMath, compare_math, parse_math, sofel_math};
use sofel_conformance::{fonts, paths};
use sofel_font::Font;

fn hb_math(font: &Path) -> Vec<HbMath> {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe).arg("--math").arg(font).output().unwrap();
    assert!(out.status.success(), "hb-dump --math {}", font.display());
    parse_math(&String::from_utf8(out.stdout).unwrap()).unwrap()
}

fn temp_font(name: &str, bytes: &[u8]) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-hb-math-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

/// The items of every face of `bytes`, written to a temporary file when
/// `path` is `None`.
fn face_items(id: &str, path: Option<&Path>, bytes: &[u8]) -> (Vec<Item>, Vec<HbMath>) {
    let temp = path.is_none().then(|| temp_font("case", bytes));
    let faces = hb_math(path.or(temp.as_deref()).unwrap());
    if let Some(t) = temp {
        std::fs::remove_file(t).ok();
    }
    let items = faces
        .iter()
        .map(|face| match Font::new(bytes, face.index) {
            Ok(font) => compare_math(id, face, &sofel_math(&font, face)),
            Err(e) => Item {
                id: format!("{id}#{}:hb:math", face.index),
                ok: false,
                detail: format!("sofel cannot open the face: {e:?}"),
            },
        })
        .collect();
    (items, faces)
}

/// The offset and length of the MATH table of a single-font sfnt file.
fn math_record(bytes: &[u8]) -> Option<(usize, usize)> {
    let be16 = |at: usize| u16::from_be_bytes([bytes[at], bytes[at + 1]]) as usize;
    let be32 = |at: usize| {
        u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
    };
    if bytes.starts_with(b"ttcf") {
        return None;
    }
    (0..be16(4))
        .map(|i| 12 + 16 * i)
        .find(|&r| &bytes[r..r + 4] == b"MATH")
        .map(|r| (be32(r + 8), be32(r + 12)))
}

fn report(items: &[Item]) {
    let failed: Vec<&Item> = items.iter().filter(|i| !i.ok).collect();
    for i in &failed {
        eprintln!("{}: {}", i.id, i.detail);
    }
    assert!(
        failed.is_empty(),
        "{} of {} items failed",
        failed.len(),
        items.len()
    );
}

#[test]
fn adr0091_a1_test_fonts_and_corrupted_copies_agree_with_harfbuzz() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let candidates = fonts::math_candidate_fonts(&testdata);
    let mut all: Vec<Item> = Vec::new();
    let (mut with_math, mut corrupted, mut rejected) = (0, 0, 0);
    for path in &candidates {
        let id = fonts::id(path, &testdata);
        let bytes = std::fs::read(path).unwrap();
        let (items, faces) = face_items(&id, Some(path), &bytes);
        all.extend(items);
        with_math += faces.iter().filter(|f| f.has_data).count();
        let Some((offset, length)) = math_record(&bytes) else {
            continue;
        };
        for field in [4usize, 6, 8] {
            let mut copy = bytes.clone();
            let bad = u16::try_from(length).unwrap_or(0xFFFF);
            copy[offset + field..offset + field + 2].copy_from_slice(&bad.to_be_bytes());
            let (items, faces) = face_items(&format!("{id}+offset{field}"), None, &copy);
            all.extend(items);
            corrupted += 1;
            rejected += faces.iter().filter(|f| !f.has_data).count();
        }
    }
    eprintln!(
        "{} fonts, {} faces with MATH, {} corrupted copies ({} rejected by HarfBuzz), {} items",
        candidates.len(),
        with_math,
        corrupted,
        rejected,
        all.len()
    );
    // HarfBuzz's eight MathTestFont* with MATH, math-variations.ttf and
    // the MATH fonts among the shaping, subsetting and extra fonts.
    assert!(with_math >= 12, "found only {with_math} faces with MATH");
    assert!(
        rejected >= 3 * 9,
        "only {rejected} corrupted copies rejected"
    );
    report(&all);
}

#[test]
fn adr0091_a1_boundary_tables_agree_with_harfbuzz() {
    use fixture::*;
    let cases: Vec<(&str, Option<Vec<u8>>, bool, Vec<u32>)> = vec![
        ("none", None, false, vec![]),
        ("nine-bytes", Some(nine_bytes()), false, vec![]),
        ("major-two", Some(major_two()), false, vec![]),
        ("empty", Some(empty()), true, vec![]),
        ("format1", Some(extended(&coverage1(&[3]))), true, vec![3]),
        (
            "format2",
            Some(extended(&coverage2(&[(3, 3)]))),
            true,
            vec![3],
        ),
        (
            "format2-ranges",
            Some(extended(&coverage2(&[(3, 5), (28, 28)]))),
            true,
            vec![3, 4, 5, 28],
        ),
        ("empty-set", Some(extended(&coverage1(&[]))), true, vec![]),
        (
            "unknown-format",
            Some(extended(&coverage_unknown())),
            true,
            vec![],
        ),
        (
            "truncated",
            Some(extended(&coverage_truncated())),
            false,
            vec![],
        ),
        ("constants-out", Some(out_of_range(4)), false, vec![]),
        ("glyph-info-out", Some(out_of_range(6)), false, vec![]),
        ("variants-out", Some(out_of_range(8)), false, vec![]),
        (
            "budget-ok",
            Some(shared_constructions(10, 200)),
            true,
            vec![],
        ),
        (
            "budget-over",
            Some(shared_constructions(200, 200)),
            false,
            vec![],
        ),
    ];
    let mut all = Vec::new();
    for (name, math, has_data, extended) in cases {
        let bytes = font(math);
        let (items, faces) = face_items(name, None, &bytes);
        assert_eq!(faces.len(), 1);
        assert_eq!(
            (faces[0].has_data, &faces[0].extended),
            (has_data, &extended),
            "{name}: HarfBuzz"
        );
        all.extend(items);
    }
    report(&all);
}
