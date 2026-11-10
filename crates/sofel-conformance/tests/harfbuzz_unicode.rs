//! sofel's Unicode character data against HarfBuzz (ADR 0019): every
//! value `hb-dump --unicode`, `--arabic-pua` and `--vowel-constraints`
//! write must match, and so must the Arabic shaper's constant tables and
//! HarfBuzz's generated sources. There is no list of expected failures;
//! one difference fails the test.

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
    // Arabic letter beh joins on both sides (JOINING_TYPE_D = 3); the
    // acute accent is transparent (T = 6) although the table lacks it.
    assert_eq!(hb.chars[0x628].joining, 3);
    assert_eq!(hb.chars[0x301].joining, 6);
    // Devanagari KA is a consonant (1) at the base (4); VOWEL SIGN I is a
    // matra (7) before the consonants (2). USE: Balinese KA is a base (1).
    assert_eq!(hb.chars[0x915].indic, 1 | (4 << 8));
    assert_eq!(hb.chars[0x93F].indic, 7 | (2 << 8));
    assert_eq!(hb.chars[0x1B13].use_category, 1);
    let last = hb.chars.last().unwrap();
    assert_eq!((last.cp, last.gc, &last.script), (u32::MAX, 2, b"Zzzz"));
    assert!(hb.compositions.contains(&(0x41, 0x30A, Some(0xC5))));
    assert!(hb.compositions.contains(&(0x1100, 0x1161, Some(0xAC00))));
    assert!(hb.compositions.contains(&(0xAC00, 0x11A8, Some(0xAC01))));
    assert!(hb.compositions.contains(&(0, 0x30A, None)));
    // ADR 0020's columns: the Arabic shadda moves before the other marks,
    // ZWJ is ignorable, EM SPACE is one em, the ideographic comma has a
    // vertical form.
    assert_eq!(hb.chars[0x651].modified_combining_class, 27);
    assert!(hb.chars[0x200D].default_ignorable && !hb.chars[0x115F].default_ignorable);
    assert_eq!(hb.chars[0x2003].space_fallback, 1);
    assert_eq!(hb.chars[0x3001].vertical, 0xFE11);

    // A changed value is reported under its item only.
    let mut changed = hb.clone();
    changed.chars[0x41].gc = 0;
    changed.compositions[0].2 = Some(0x41);
    changed.chars[0x915].indic = 0;
    changed.chars[0x1B13].use_category = 0;
    changed.chars[0x628].joining = 0;
    let failed: Vec<String> = unicode::compare(&changed)
        .into_iter()
        .filter(|i| !i.ok)
        .map(|i| i.id)
        .collect();
    assert_eq!(
        failed,
        [
            "unicode:general_category",
            "unicode:compose",
            "unicode:indic",
            "unicode:use",
            "unicode:joining"
        ]
    );
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

fn vowel_dump() -> Vec<unicode::HbVowel> {
    let file = paths::testdata().join("harfbuzz/src/ms-use/IndicShapingInvalidCluster.txt");
    paths::require(&file, "the HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let text = hb_dump(&["--vowel-constraints".as_ref(), file.as_os_str()]);
    unicode::parse_vowel_constraints(&text).unwrap()
}

#[test]
fn hb_dump_writes_known_vowel_constraints() {
    let v = vowel_dump();
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

#[test]
fn vowel_constraints_agree_with_harfbuzz() {
    let v = vowel_dump();
    eprintln!("{} sequences", v.len());
    // Gujarati A + AA + CANDRA E: only the shorter A + AA is a constraint.
    assert_eq!(
        unicode::vowel_constraints(*b"Gujr", &[0xA85, 0xABE, 0xAC5]),
        vec![0xA85, 0x25CC, 0xABE, 0xAC5]
    );
    // Devanagari RA + VIRAMA needs a third character.
    assert_eq!(
        unicode::vowel_constraints(*b"Deva", &[0x930, 0x94D]),
        vec![0x930, 0x94D]
    );
    assert_eq!(
        unicode::vowel_constraints(*b"Latn", &[0x905, 0x93E]),
        vec![0x905, 0x93E]
    );
    let item = unicode::compare_vowel_constraints(&v);
    assert_all_ok(&[item]);

    // A changed line is reported.
    let mut changed = v.clone();
    let deva = changed
        .iter_mut()
        .find(|x| x.input == [0x905, 0x93E])
        .unwrap();
    deva.output = deva.input.clone();
    assert!(!unicode::compare_vowel_constraints(&changed).ok);
}

#[test]
fn arabic_tables_agree_with_harfbuzz_sources() {
    let src = paths::testdata().join("harfbuzz/src");
    paths::require(&src, "the HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let read = |f: &str| std::fs::read_to_string(src.join(f)).unwrap();
    let table = read("hb-ot-shaper-arabic-table.hh");
    let list = read("hb-ot-shaper-arabic-joining-list.hh");
    let scripts = read("hb-script-list.h");
    assert_all_ok(&unicode::compare_arabic_tables(&table, &list, &scripts));

    // A changed row, ligature or script is reported.
    let failed = |t: &str, l: &str| -> Vec<String> {
        unicode::compare_arabic_tables(t, l, &scripts)
            .into_iter()
            .filter(|i| !i.ok)
            .map(|i| i.id)
            .collect()
    };
    let t = table.replacen("0xFE80u}", "0xFE81u}", 1);
    let t = t.replacen("0xFC08u", "0xFC09u", 1);
    let l = list.replace("case HB_SCRIPT_SYRIAC:", "case HB_SCRIPT_LATIN:");
    assert_eq!(
        failed(&t, &l),
        [
            "unicode:arabic_shaping_table",
            "unicode:arabic_ligature_table",
            "unicode:arabic_joining_scripts"
        ]
    );
}
