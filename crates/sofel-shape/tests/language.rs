//! Language normalization as `hb_language_from_string` does it
//! (ADR 0020, テスト設計「区間の性質」).

#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_shape::Language;

fn norm(s: &[u8]) -> Option<String> {
    Language::new(s).map(|l| l.as_str().to_string())
}

#[test]
fn locale_strings_are_normalized() {
    assert_eq!(norm(b"ja_JP.UTF-8").as_deref(), Some("ja-jp"));
    assert_eq!(norm(b"en_US").as_deref(), Some("en-us"));
    assert_eq!(norm(b"fa_IR.utf8").as_deref(), Some("fa-ir"));
    assert_eq!(norm(b"zH-HanT-hK").as_deref(), Some("zh-hant-hk"));
}

#[test]
fn the_tag_ends_at_the_first_byte_outside_the_map() {
    assert_eq!(norm(b"tr@foo=bar").as_deref(), Some("tr"));
    assert_eq!(norm(b"de DE").as_deref(), Some("de"));
    assert_eq!(norm(b"ab\0cd").as_deref(), Some("ab"));
    assert_eq!(norm("é-fr".as_bytes()).as_deref(), Some(""));
    assert_eq!(norm(b"en\xC3\xA9").as_deref(), Some("en"));
    assert_eq!(norm(b"x-hbot-41424344").as_deref(), Some("x-hbot-41424344"));
    assert_eq!(norm(b"__a1").as_deref(), Some("--a1"));
}

#[test]
fn empty_input_is_no_language() {
    // `!len || !*str` in hb_language_from_string.
    assert_eq!(norm(b""), None);
    assert_eq!(norm(b"\0en"), None);
}

#[test]
fn a_first_byte_outside_the_map_is_the_empty_language() {
    // HarfBuzz keeps such a string as the empty language, not as
    // HB_LANGUAGE_INVALID.
    assert_eq!(norm(b"."), Some(String::new()));
    assert_eq!(norm(b"@en"), Some(String::new()));
}

#[test]
fn input_is_cut_to_63_bytes_before_normalization() {
    // The `char strbuf[64]` of hb_language_from_string with a length.
    let long = [b'a'; 100];
    assert_eq!(norm(&long).unwrap().len(), 63);
    let mut s = vec![b'b'; 62];
    s.extend_from_slice(b"cd");
    assert_eq!(norm(&s).unwrap(), format!("{}c", "b".repeat(62)));
}

#[test]
fn equal_after_normalization() {
    assert_eq!(Language::new(b"EN-us"), Language::new(b"en_US"));
    assert_ne!(Language::new(b"en"), Language::new(b"en-us"));
}
