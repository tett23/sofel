//! Single values of every property (ADR 0019, テスト設計).

#![allow(clippy::all, clippy::unwrap_used)]

use sofel_unicode::{
    GeneralCategory as Gc, Script, UNICODE_VERSION, VerticalOrientation as Vo, combining_class,
    compose, decompose, general_category, is_extended_pictographic, mirroring, script,
    vertical_orientation,
};

fn tag(s: &[u8; 4]) -> Script {
    Script::from_iso15924(s).unwrap()
}

#[test]
fn version() {
    assert_eq!(UNICODE_VERSION, (18, 0, 0));
}

#[test]
fn latin_capital_a() {
    assert_eq!(general_category(0x41), Gc::UppercaseLetter);
    assert_eq!(combining_class(0x41), 0);
    assert_eq!(script(0x41), tag(b"Latn"));
    assert_eq!(mirroring(0x41), 0x41);
}

#[test]
fn combining_acute() {
    assert_eq!(general_category(0x301), Gc::NonspacingMark);
    assert_eq!(combining_class(0x301), 230);
    assert_eq!(script(0x301), Script::INHERITED);
    assert_eq!(Script::INHERITED.tag(), *b"Zinh");
}

#[test]
fn japanese() {
    assert_eq!(general_category(0x3042), Gc::OtherLetter);
    assert_eq!(script(0x3042), tag(b"Hira"));
    assert_eq!(general_category(0x30A2), Gc::OtherLetter);
    assert_eq!(script(0x30A2), tag(b"Kana"));
    assert_eq!(general_category(0x4E00), Gc::OtherLetter);
    assert_eq!(script(0x4E00), tag(b"Hani"));
    assert_eq!(general_category(0x30FC), Gc::ModifierLetter);
    assert_eq!(script(0x30FC), Script::COMMON);
    assert_eq!(Script::COMMON.tag(), *b"Zyyy");
}

#[test]
fn mirrors() {
    assert_eq!(mirroring(0x28), 0x29);
    assert_eq!(mirroring(0x29), 0x28);
}

#[test]
fn unassigned_private_use_and_surrogates() {
    assert_eq!(general_category(0x378), Gc::Unassigned);
    assert_eq!(script(0x378), Script::UNKNOWN);
    assert_eq!(Script::UNKNOWN.tag(), *b"Zzzz");
    assert_eq!(general_category(0xE000), Gc::PrivateUse);
    assert_eq!(general_category(0xD800), Gc::Surrogate);
}

#[test]
fn defaults_at_and_beyond_the_end() {
    for cp in [0x10FFFF, 0x110000, 0x7FFF_FFFF, u32::MAX] {
        assert_eq!(general_category(cp), Gc::Unassigned, "{cp:X}");
        assert_eq!(combining_class(cp), 0);
        assert_eq!(script(cp), Script::UNKNOWN);
        assert_eq!(mirroring(cp), cp);
        assert_eq!(decompose(cp), None);
        assert!(!is_extended_pictographic(cp));
        assert_eq!(vertical_orientation(cp), Vo::R);
    }
}

#[test]
fn general_category_values_are_harfbuzz_order() {
    assert_eq!(Gc::Control as u8, 0);
    assert_eq!(Gc::Unassigned as u8, 2);
    assert_eq!(Gc::UppercaseLetter as u8, 9);
    assert_eq!(Gc::NonspacingMark as u8, 12);
    assert_eq!(Gc::SpaceSeparator as u8, 29);
}

#[test]
fn decompositions() {
    assert_eq!(decompose(0xC5), Some((0x41, 0x30A)));
    assert_eq!(decompose(0x212B), Some((0xC5, 0)));
    assert_eq!(decompose(0xAC00), Some((0x1100, 0x1161)));
    assert_eq!(decompose(0xAC01), Some((0xAC00, 0x11A8)));
    assert_eq!(decompose(0x41), None);
}

#[test]
fn compositions() {
    assert_eq!(compose(0x41, 0x30A), Some(0xC5));
    assert_eq!(compose(0x1100, 0x1161), Some(0xAC00));
    assert_eq!(compose(0xAC00, 0x11A8), Some(0xAC01));
    // U+0958 is a composition exclusion.
    assert_eq!(decompose(0x958), Some((0x915, 0x93C)));
    assert_eq!(compose(0x915, 0x93C), None);
    assert_eq!(compose(0, 0x30A), None);
    assert_eq!(compose(0x41, 0), None);
    assert_eq!(compose(0x41, 0x42), None);
}

#[test]
fn extended_pictographic() {
    assert!(is_extended_pictographic(0x1F600));
    assert!(is_extended_pictographic(0xA9));
    assert!(!is_extended_pictographic(0x41));
}

#[test]
fn vertical_orientations() {
    assert_eq!(vertical_orientation(0x41), Vo::R);
    assert_eq!(vertical_orientation(0x3042), Vo::U);
    assert_eq!(vertical_orientation(0x3001), Vo::Tu);
    assert_eq!(vertical_orientation(0x30FC), Vo::Tr);
    assert_eq!(vertical_orientation(0xFF08), Vo::Tr);
}

#[test]
fn script_tags() {
    assert_eq!(Script::from_iso15924(b"latn"), Some(tag(b"Latn")));
    assert_eq!(
        Script::from_iso15924(b"LATN").map(Script::tag),
        Some(*b"Latn")
    );
    assert_eq!(Script::from_iso15924(b"Qaai"), Some(Script::INHERITED));
    assert_eq!(Script::from_iso15924(b"Qaac"), Some(tag(b"Copt")));
    assert_eq!(Script::from_iso15924(b"Aran"), Some(tag(b"Arab")));
    assert_eq!(Script::from_iso15924(b"Lat"), None);
    assert_eq!(Script::from_iso15924(b"Latnx"), None);
    assert_eq!(Script::from_iso15924(b""), None);
    assert_eq!(Script::from_iso15924(b"La1n"), None);
}
