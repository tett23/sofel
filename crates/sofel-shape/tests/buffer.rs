//! Buffers, segment properties and `diff` (ADR 0020, テスト設計): the
//! examples of HarfBuzz's `test/api/test-buffer.c` and the rules of
//! `src/hb-utf.hh`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::bool_assert_comparison
)]

use sofel_shape::{
    BufferFlags, ClusterLevel, DiffFlags, Direction, GlyphBuffer, GlyphFlags, GlyphInfo,
    GlyphPosition, Language, ResolvedProperties, Script, SegmentProperties, ShapePlan,
    UnicodeBuffer, diff, diff_unicode, script_horizontal_direction,
};

fn contents(b: &UnicodeBuffer) -> Vec<(u32, u32)> {
    b.codepoints().zip(b.clusters()).collect()
}

#[test]
fn utf8_conversion_tests() {
    // test-buffer.c `utf8_conversion_tests`: the first and last bytes are
    // left out of the item; the replacement is (hb_codepoint_t) -1.
    let m = u32::MAX;
    let cases: &[(&[u8], &[u32])] = &[
        (b"a\xC3\x87", &[m]),
        (b"a\xC3\x87b", &[0xC7]),
        (b"ab\xC3cd", &[0x62, m, 0x63]),
        (b"ab\xC3\xC2\xC1cd", &[0x62, m, m, m, 0x63]),
    ];
    for (text, want) in cases {
        let mut b = UnicodeBuffer::new();
        b.set_replacement_codepoint(m);
        b.push_utf8(text, 1, Some(text.len() - 2));
        assert_eq!(b.codepoints().collect::<Vec<_>>(), *want, "{text:x?}");
    }
}

#[test]
fn utf16_conversion_tests() {
    // test-buffer.c `utf16_conversion_tests`.
    let m = u32::MAX;
    let cases: &[(&[u16], &[u32])] = &[
        (
            &[0x41, 0x004D, 0x0430, 0x4E8C, 0xD800, 0xDF02, 0x61],
            &[0x004D, 0x0430, 0x4E8C, 0x10302],
        ),
        (&[0x41, 0xD800, 0xDF02, 0x61], &[0x10302]),
        (&[0x41, 0xD800, 0xDF02], &[m]),
        (&[0x41, 0x61, 0xD800, 0xDF02], &[0x61, m]),
    ];
    for (text, want) in cases {
        let mut b = UnicodeBuffer::new();
        b.set_replacement_codepoint(m);
        b.push_utf16(text, 1, Some(text.len() - 2));
        assert_eq!(b.codepoints().collect::<Vec<_>>(), *want, "{text:x?}");
    }
}

#[test]
fn utf8_clusters_and_context() {
    let text = b"a\xC2\x80\xE1\x88\xB4\xF0\x90\x80\x81";
    let mut b = UnicodeBuffer::new();
    b.push_utf8(text, 1, Some(text.len() - 2));
    // The four-byte sequence is cut by the item's end: one replacement
    // per byte inside the item, and its last byte alone after it.
    assert_eq!(
        contents(&b),
        [
            (0x80, 1),
            (0x1234, 3),
            (0xFFFD, 6),
            (0xFFFD, 7),
            (0xFFFD, 8)
        ]
    );
    assert_eq!(b.pre_context(), [u32::from(b'a')]);
    assert_eq!(b.post_context(), [0xFFFD]);
}

#[test]
fn utf8_replacement_per_byte() {
    // Each ill-formed byte becomes one replacement character (hb-utf.hh).
    let cases: &[(&[u8], &[u32])] = &[
        (b"\x80", &[0xFFFD]),
        (b"\xC0\x80", &[0xFFFD, 0xFFFD]),
        (b"\xE0\x80\x80", &[0xFFFD, 0xFFFD, 0xFFFD]),
        (b"\xED\xA0\x80", &[0xFFFD, 0xFFFD, 0xFFFD]),
        (b"\xF4\x90\x80\x80", &[0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD]),
        (b"\xE1\x88", &[0xFFFD, 0xFFFD]),
        (b"\xF0\x9F\x98\x80", &[0x1F600]),
        (b"a\xFFb", &[0x61, 0xFFFD, 0x62]),
    ];
    for (text, want) in cases {
        let mut b = UnicodeBuffer::new();
        b.push_utf8(text, 0, None);
        assert_eq!(b.codepoints().collect::<Vec<_>>(), *want, "{text:x?}");
    }
    let mut b = UnicodeBuffer::new();
    b.set_replacement_codepoint(u32::from(b'?'));
    b.push_utf8(b"a\x80", 0, None);
    assert_eq!(contents(&b), [(0x61, 0), (0x3F, 1)]);
}

#[test]
fn utf8_pre_context_backward() {
    // Up to five characters before the item, nearest first; a cut
    // sequence before the item reads as one replacement per byte.
    let text = "abcdefg\u{1234}h".as_bytes();
    let mut b = UnicodeBuffer::new();
    b.push_utf8(text, 10, None);
    assert_eq!(contents(&b), [(u32::from(b'h'), 10)]);
    assert_eq!(b.pre_context(), [0x1234, 0x67, 0x66, 0x65, 0x64]);
    let mut b = UnicodeBuffer::new();
    b.push_utf8(text, 9, None);
    assert_eq!(b.pre_context()[0], 0xFFFD);
    // Pre-context is only taken into an empty buffer.
    let mut b = UnicodeBuffer::new();
    b.push_utf8(b"x", 0, None);
    b.push_utf8(b"yz", 1, None);
    assert_eq!(b.pre_context(), [] as [u32; 0]);
    assert_eq!(contents(&b), [(0x78, 0), (0x7A, 1)]);
}

#[test]
fn utf16_lone_surrogates() {
    let text: [u16; 6] = [0x61, 0xD800, 0x62, 0xDC00, 0xD83D, 0xDE00];
    let mut b = UnicodeBuffer::new();
    b.push_utf16(&text, 0, None);
    assert_eq!(
        contents(&b),
        [(0x61, 0), (0xFFFD, 1), (0x62, 2), (0xFFFD, 3), (0x1F600, 4)]
    );
    let mut b = UnicodeBuffer::new();
    b.push_utf16(&text, 5, None);
    assert_eq!(
        b.pre_context()[0],
        0xFFFD,
        "a low surrogate whose high one is outside"
    );
    assert_eq!(contents(&b), [(0xFFFD, 5)]);
}

#[test]
fn utf32_and_codepoints() {
    let text = [0x61, 0xD800, 0x110000, 0x10FFFF];
    let mut b = UnicodeBuffer::new();
    b.push_utf32(&text, 0, None);
    assert_eq!(
        b.codepoints().collect::<Vec<_>>(),
        [0x61, 0xFFFD, 0xFFFD, 0x10FFFF]
    );
    let mut b = UnicodeBuffer::new();
    b.push_codepoints(&text, 1, Some(2));
    assert_eq!(contents(&b), [(0xD800, 1), (0x110000, 2)]);
    assert_eq!(b.pre_context(), [0x61]);
    assert_eq!(b.post_context(), [0x10FFFF]);
}

#[test]
fn latin1_and_clamping() {
    let mut b = UnicodeBuffer::new();
    b.push_latin1(b"\xE9t\xE9", 0, None);
    assert_eq!(contents(&b), [(0xE9, 0), (0x74, 1), (0xE9, 2)]);
    // Offsets and lengths are clamped to the text.
    let mut b = UnicodeBuffer::new();
    b.push_latin1(b"abc", 7, Some(9));
    assert!(b.is_empty());
    let mut b = UnicodeBuffer::new();
    b.push_latin1(b"abc", 1, Some(9));
    assert_eq!(contents(&b), [(0x62, 1), (0x63, 2)]);
}

#[test]
fn post_context_is_five_characters() {
    let mut b = UnicodeBuffer::new();
    b.push_utf8(b"abcdefghij", 0, Some(1));
    assert_eq!(b.post_context(), [0x62, 0x63, 0x64, 0x65, 0x66]);
}

#[test]
fn settings_survive_clear() {
    let mut b = UnicodeBuffer::new();
    b.set_flags(BufferFlags::BOT.union(BufferFlags::EOT));
    b.set_cluster_level(ClusterLevel::Characters);
    b.set_invisible_glyph(3);
    b.set_not_found_glyph(4);
    b.set_not_found_variation_selector_glyph(Some(5));
    b.set_random_state(7);
    b.push_utf8(b"a", 0, None);
    b.clear();
    assert!(b.is_empty());
    assert_eq!(b.flags(), BufferFlags(3));
    assert_eq!(b.cluster_level(), ClusterLevel::Characters);
    assert_eq!(b.invisible_glyph(), 3);
    assert_eq!(b.not_found_glyph(), 4);
    assert_eq!(b.not_found_variation_selector_glyph(), Some(5));
    assert_eq!(
        b.random_state(),
        1,
        "hb_buffer_clear_contents resets the random state"
    );
}

#[test]
fn limits_are_harfbuzz_constants() {
    assert_eq!(sofel_shape::MAX_LEN_FACTOR, 256);
    assert_eq!(sofel_shape::MAX_LEN_MIN, 65536);
    assert_eq!(sofel_shape::MAX_LEN_DEFAULT, 0x3FFF_FFFF);
    assert_eq!(sofel_shape::MAX_OPS_FACTOR, 4096);
    assert_eq!(sofel_shape::MAX_OPS_MIN, 65536);
    assert_eq!(sofel_shape::MAX_OPS_DEFAULT, 0x1FFF_FFFF);
}

fn script(t: &[u8; 4]) -> Script {
    Script::from_iso15924(t).unwrap()
}

#[test]
fn guess_script_and_direction() {
    let guess = |text: &str| {
        let mut b = UnicodeBuffer::new();
        b.push_utf8(text.as_bytes(), 0, None);
        SegmentProperties::default().guess(&b, None)
    };
    // Common and Inherited characters are passed over.
    let p = guess("12 \u{0301}\u{05D0}a");
    assert_eq!(p.script, Some(script(b"Hebr")));
    assert_eq!(p.direction, Direction::Rtl);
    assert_eq!(p.language, None);
    // No script: invalid, left to right.
    let p = guess("123 !");
    assert_eq!(p.script, None);
    assert_eq!(p.direction, Direction::Ltr);
    let p = guess("");
    assert_eq!((p.script, p.direction), (None, Direction::Ltr));
    // A script written both ways: left to right.
    let p = guess("\u{16A0}");
    assert_eq!(p.script, Some(script(b"Runr")));
    assert_eq!(p.direction, Direction::Ltr);
}

#[test]
fn guess_keeps_given_properties_and_uses_default_language() {
    let mut b = UnicodeBuffer::new();
    b.push_utf8("\u{05D0}".as_bytes(), 0, None);
    let given = SegmentProperties {
        direction: Some(Direction::Ttb),
        script: Some(script(b"Latn")),
        language: None,
    };
    let p = given.guess(&b, Language::new(b"ja_JP.UTF-8"));
    assert_eq!(p.direction, Direction::Ttb);
    assert_eq!(p.script, Some(script(b"Latn")));
    assert_eq!(p.language.unwrap().as_str(), "ja-jp");
    let given = SegmentProperties {
        language: Language::new(b"fr"),
        ..SegmentProperties::default()
    };
    let p = given.guess(&b, Language::new(b"de"));
    assert_eq!(p.language.unwrap().as_str(), "fr");
}

#[test]
fn horizontal_directions() {
    assert_eq!(
        script_horizontal_direction(Some(script(b"Arab"))),
        Some(Direction::Rtl)
    );
    assert_eq!(
        script_horizontal_direction(Some(script(b"Sidt"))),
        Some(Direction::Rtl)
    );
    assert_eq!(script_horizontal_direction(Some(script(b"Tfng"))), None);
    assert_eq!(
        script_horizontal_direction(Some(script(b"Latn"))),
        Some(Direction::Ltr)
    );
    assert_eq!(
        script_horizontal_direction(Some(script(b"Xyzw"))),
        Some(Direction::Ltr)
    );
    assert_eq!(script_horizontal_direction(None), Some(Direction::Ltr));
}

fn glyphs(items: &[(u32, u32, u32)], pos: &[i32]) -> GlyphBuffer {
    GlyphBuffer::from_parts(
        Direction::Ltr,
        items
            .iter()
            .map(|&(glyph, cluster, flags)| GlyphInfo {
                glyph,
                cluster,
                flags: GlyphFlags(flags),
            })
            .collect(),
        pos.iter()
            .map(|&x| GlyphPosition {
                x_advance: x,
                ..GlyphPosition::default()
            })
            .collect(),
    )
}

#[test]
fn diff_flags() {
    let a = glyphs(&[(1, 0, 0), (2, 1, 0)], &[100, 100]);
    assert_eq!(diff(&a, &a, u32::MAX, 0), DiffFlags::EQUAL);
    let b = glyphs(&[(1, 0, 0)], &[100]);
    assert_eq!(diff(&a, &b, u32::MAX, 0), DiffFlags::LENGTH_MISMATCH);
    let c = glyphs(&[(0, 0, 0), (7, 1, 0), (3, 2, 0)], &[0, 0, 0]);
    assert_eq!(
        diff(&a, &c, 7, 0),
        DiffFlags::LENGTH_MISMATCH
            .union(DiffFlags::NOTDEF_PRESENT)
            .union(DiffFlags::DOTTED_CIRCLE_PRESENT)
    );
    let d = glyphs(&[(1, 0, 1), (5, 2, 0)], &[100, 103]);
    assert_eq!(
        diff(&a, &d, u32::MAX, 0),
        DiffFlags::CODEPOINT_MISMATCH
            .union(DiffFlags::CLUSTER_MISMATCH)
            .union(DiffFlags::GLYPH_FLAGS_MISMATCH)
            .union(DiffFlags::POSITION_MISMATCH)
    );
    let e = glyphs(&[(1, 0, 8), (2, 1, 0)], &[100, 103]);
    assert_eq!(
        diff(&a, &e, u32::MAX, 3),
        DiffFlags::EQUAL,
        "fuzz and undefined flags"
    );
    let mut u = UnicodeBuffer::new();
    u.push_utf8(b"ab", 0, None);
    let mut v = UnicodeBuffer::new();
    v.push_utf8(b"ac", 0, None);
    assert_eq!(
        diff_unicode(&u, &v, u32::MAX),
        DiffFlags::CODEPOINT_MISMATCH
    );
}

#[test]
fn normalize_glyphs_moves_advances() {
    let mut g = glyphs(&[(5, 0, 0), (3, 0, 0), (9, 1, 0)], &[100, 50, 70]);
    g.normalize_glyphs();
    let infos: Vec<u32> = g.glyph_infos().iter().map(|i| i.glyph).collect();
    // The cluster's advance goes to its first glyph; the others follow by
    // offset, sorted by glyph id, descending.
    assert_eq!(infos, [5, 3, 9]);
    let p = g.glyph_positions();
    assert_eq!(
        (p[0].x_advance, p[1].x_advance, p[1].x_offset),
        (150, 0, -50)
    );
}

#[test]
fn types_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<UnicodeBuffer>();
    check::<GlyphBuffer>();
    check::<ShapePlan<'static>>();
    check::<ResolvedProperties>();
}
