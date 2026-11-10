//! sofel's Unicode character data against HarfBuzz (ADR 0019): every
//! value `hb-dump --unicode` and `--arabic-pua` write must match. There
//! is no list of expected failures; one difference fails the test.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::process::Command;

use sofel_conformance::compare::Item;
use sofel_conformance::{paths, unicode};

fn hb_dump(args: &[&std::ffi::OsStr]) -> String {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe).args(args).output().unwrap();
    assert!(out.status.success(), "hb-dump {args:?} failed");
    String::from_utf8(out.stdout).unwrap()
}

fn assert_all_ok(items: &[Item]) {
    for i in items {
        eprintln!("  {}: {}", i.id, if i.ok { "ok" } else { "FAILED" });
    }
    let failed: Vec<String> = items
        .iter()
        .filter(|i| !i.ok)
        .map(|i| format!("{}: {}", i.id, i.detail))
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn hb_dump_writes_known_unicode_values() {
    let hb = unicode::parse(&hb_dump(&["--unicode".as_ref()])).unwrap();
    assert_eq!(hb.chars.len(), 0x110000 + 3);
    let a = hb.chars[0x41];
    assert_eq!(
        (a.cp, a.gc, a.ccc, &a.script, a.mirroring, a.decomposition),
        (0x41, 9, 0, b"Latn", 0x41, None)
    );
    assert_eq!(hb.chars[0x28].mirroring, 0x29);
    assert_eq!(hb.chars[0xC5].decomposition, Some((0x41, 0x30A)));
    assert_eq!(hb.chars[0x212B].decomposition, Some((0xC5, 0)));
    assert!(hb.chars[0x1F600].extended_pictographic);
    // Arabic letter beh joins on both sides (JOINING_TYPE_D = 3).
    assert_eq!(hb.chars[0x628].joining, 3);
    let last = hb.chars.last().unwrap();
    assert_eq!((last.cp, last.gc, &last.script), (u32::MAX, 2, b"Zzzz"));
    assert!(hb.compositions.contains(&(0x41, 0x30A, Some(0xC5))));
    assert!(hb.compositions.contains(&(0x1100, 0x1161, Some(0xAC00))));
    assert!(hb.compositions.contains(&(0xAC00, 0x11A8, Some(0xAC01))));
    assert!(hb.compositions.contains(&(0, 0x30A, None)));

    // A changed value is reported under its item only.
    let mut changed = hb.clone();
    changed.chars[0x41].gc = 0;
    changed.compositions[0].2 = Some(0x41);
    let failed: Vec<String> = unicode::compare(&changed)
        .into_iter()
        .filter(|i| !i.ok)
        .map(|i| i.id)
        .collect();
    assert_eq!(failed, ["unicode:general_category", "unicode:compose"]);
}

#[test]
fn unicode_agrees_with_harfbuzz() {
    let hb = unicode::parse(&hb_dump(&["--unicode".as_ref()])).unwrap();
    eprintln!(
        "{} code points, {} pairs",
        hb.chars.len(),
        hb.compositions.len()
    );
    assert_all_ok(&unicode::compare(&hb));
}

#[test]
fn arabic_pua_agrees_with_harfbuzz() {
    let hb = unicode::parse_arabic_pua(&hb_dump(&["--arabic-pua".as_ref()])).unwrap();
    assert_eq!(hb.len(), 0x10000);
    assert_eq!(hb[0x20], (0x20, 0xF120, 0xF220));
    assert_all_ok(&unicode::compare_arabic_pua(&hb));
}

#[test]
fn hb_dump_writes_known_vowel_constraints() {
    // The comparison waits for sofel-shape's table (ADR 0020, ADR 0024);
    // this checks the dump itself.
    let file = paths::testdata().join("harfbuzz/src/ms-use/IndicShapingInvalidCluster.txt");
    paths::require(&file, "the HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let text = hb_dump(&["--vowel-constraints".as_ref(), file.as_os_str()]);
    let v = unicode::parse_vowel_constraints(&text).unwrap();
    let find = |input: &[u32]| {
        v.iter()
            .find(|x| &x.script == b"Deva" && x.input == input)
            .unwrap()
            .output
            .clone()
    };
    assert_eq!(find(&[0x905, 0x93E]), vec![0x905, 0x25CC, 0x93E]);
    assert_eq!(find(&[0x93E, 0x905]), vec![0x93E, 0x905]);
    assert_eq!(
        find(&[0x930, 0x94D, 0x907]),
        vec![0x930, 0x94D, 0x25CC, 0x907]
    );
    assert!(v.iter().any(|x| &x.script == b"Sinh"));
}
