//! The script-specific shapers with hand-built fonts (ADR 0024, テスト設計
//! 各シェーパーの規則).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

mod support;

use sofel_font::Font;
use sofel_shape::{
    Direction, GlyphBuffer, ResolvedProperties, Scale, Script, ShapePlan, Shaper, UnicodeBuffer,
    shape,
};
use support::{lookup, mark_base, simple_layout, single_subst};

/// A font whose glyph `i + 1` is the character `chars[i]`, every glyph
/// `advance` wide, with extra tables.
fn font_for(chars: &[u32], advance: u16, extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    let map: Vec<(u32, u16)> = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, i as u16 + 1))
        .collect();
    support::font(
        chars.len() as u16 + 1,
        &map,
        &vec![advance; chars.len() + 1],
        extra,
    )
}

/// The glyph of `c` in a [`font_for`] font.
fn g(chars: &[u32], c: u32) -> u32 {
    chars.iter().position(|&x| x == c).unwrap() as u32 + 1
}

fn shape_cps(data: &[u8], script: &[u8; 4], direction: Direction, text: &[u32]) -> GlyphBuffer {
    let font = Font::new(data, 0).unwrap();
    let props = ResolvedProperties {
        direction,
        script: Script::from_iso15924(script),
        language: None,
    };
    let plan = ShapePlan::new(&font, props, &[]);
    let mut b = UnicodeBuffer::new();
    b.push_codepoints(text, 0, None);
    shape(&font, &Scale::unscaled(&font), &plan, b)
}

fn glyphs(g: &GlyphBuffer) -> Vec<u32> {
    g.glyph_infos().iter().map(|i| i.glyph).collect()
}

fn clusters(g: &GlyphBuffer) -> Vec<u32> {
    g.glyph_infos().iter().map(|i| i.cluster).collect()
}

fn plan_shaper(data: &[u8], script: &[u8; 4]) -> Shaper {
    let font = Font::new(data, 0).unwrap();
    let props = ResolvedProperties {
        direction: Direction::Ltr,
        script: Script::from_iso15924(script),
        language: None,
    };
    ShapePlan::new(&font, props, &[]).shaper()
}

// Hebrew.

const HEBREW: [u32; 7] = [0x05D1, 0x05BC, 0xFB31, 0x05B7, 0x05B0, 0x05BD, 0x05D0];

#[test]
fn hebrew_composes_presentation_forms_without_gpos_marks() {
    let data = font_for(&HEBREW, 500, vec![]);
    assert_eq!(plan_shaper(&data, b"Hebr"), Shaper::Hebrew);
    // BET + DAGESH is excluded from canonical composition; the shaper
    // composes it into U+FB31 for a font without mark positioning.
    let out = shape_cps(&data, b"Hebr", Direction::Rtl, &[0x05D1, 0x05BC]);
    assert_eq!(glyphs(&out), [g(&HEBREW, 0xFB31)]);

    // With a GPOS `mark` feature it stays decomposed.
    let gpos = simple_layout(
        &[b"mark"],
        vec![lookup(4, 0, vec![mark_base(&[(2, 0, 0)], &[(1, 0, 0)])])],
    );
    let data = font_for(&HEBREW, 500, vec![(b"GPOS", gpos)]);
    let out = shape_cps(&data, b"Hebr", Direction::Rtl, &[0x05D1, 0x05BC]);
    assert_eq!(glyphs(&out).len(), 2);
}

#[test]
fn hebrew_moves_meteg_before_sheva_after_patah() {
    let data = font_for(&HEBREW, 500, vec![]);
    // ALEF PATAH SHEVA METEG: METEG and SHEVA swap and merge clusters.
    // Right to left, so the glyphs come out reversed.
    let out = shape_cps(
        &data,
        b"Hebr",
        Direction::Rtl,
        &[0x05D0, 0x05B7, 0x05B0, 0x05BD],
    );
    assert_eq!(
        glyphs(&out),
        [
            g(&HEBREW, 0x05B0),
            g(&HEBREW, 0x05BD),
            g(&HEBREW, 0x05B7),
            g(&HEBREW, 0x05D0)
        ]
    );
    assert_eq!(clusters(&out), [0, 0, 0, 0]);
}

// Hangul.

const HANGUL: [u32; 6] = [0x1100, 0x1161, 0x11A8, 0xAC00, 0x302E, 0x25CC];

#[test]
fn hangul_composes_jamo_into_syllables() {
    let data = font_for(&HANGUL, 1000, vec![]);
    assert_eq!(plan_shaper(&data, b"Hang"), Shaper::Hangul);
    // <L,V> composes into U+AC00.
    let out = shape_cps(&data, b"Hang", Direction::Ltr, &[0x1100, 0x1161]);
    assert_eq!(glyphs(&out), [g(&HANGUL, 0xAC00)]);
    // <L,V,T>: U+AC01 is missing, so the jamo stay, in one cluster.
    let out = shape_cps(&data, b"Hang", Direction::Ltr, &[0x1100, 0x1161, 0x11A8]);
    assert_eq!(
        glyphs(&out),
        [g(&HANGUL, 0x1100), g(&HANGUL, 0x1161), g(&HANGUL, 0x11A8)]
    );
    assert_eq!(clusters(&out), [0, 0, 0]);
    // <LV,T> with U+AC01 missing decomposes the LV.
    let out = shape_cps(&data, b"Hang", Direction::Ltr, &[0xAC00, 0x11A8]);
    assert_eq!(
        glyphs(&out),
        [g(&HANGUL, 0x1100), g(&HANGUL, 0x1161), g(&HANGUL, 0x11A8)]
    );
}

#[test]
fn hangul_masks_jamo_features_and_moves_tone_marks() {
    // ljmo, vjmo and tjmo each replace their jamo with the next glyph id
    // past the cmap (7, 8, 9).
    let (l, v, t) = (
        g(&HANGUL, 0x1100) as u16,
        g(&HANGUL, 0x1161) as u16,
        g(&HANGUL, 0x11A8) as u16,
    );
    let gsub = simple_layout(
        &[b"ljmo", b"vjmo", b"tjmo"],
        vec![
            lookup(1, 0, vec![single_subst(&[(l, 7)])]),
            lookup(1, 0, vec![single_subst(&[(v, 8)])]),
            lookup(1, 0, vec![single_subst(&[(t, 9)])]),
        ],
    );
    let mut chars = HANGUL.to_vec();
    chars.extend([0xE000, 0xE001, 0xE002]);
    let data = font_for(&chars, 1000, vec![(b"GSUB", gsub)]);
    let out = shape_cps(&data, b"Hang", Direction::Ltr, &[0x1100, 0x1161, 0x11A8]);
    assert_eq!(glyphs(&out), [7, 8, 9]);

    // A tone mark after a syllable moves before it.
    let out = shape_cps(&data, b"Hang", Direction::Ltr, &[0xAC00, 0x302E]);
    assert_eq!(glyphs(&out), [g(&HANGUL, 0x302E), g(&HANGUL, 0xAC00)]);
    // Without a syllable, a dotted circle comes after it.
    let out = shape_cps(&data, b"Hang", Direction::Ltr, &[0x302E]);
    assert_eq!(glyphs(&out), [g(&HANGUL, 0x302E), g(&HANGUL, 0x25CC)]);
}

// Thai.

const THAI: [u32; 8] = [
    0x0E14, 0x0E4B, 0x0E33, 0x0E4D, 0x0E32, 0x0E1B, 0x0E48, 0xF705,
];

#[test]
fn thai_decomposes_sara_am() {
    let data = font_for(&THAI, 500, vec![]);
    assert_eq!(plan_shaper(&data, b"Thai"), Shaper::Thai);
    // <0E14, 0E4B, 0E33> -> <0E14, 0E4D, 0E4B, 0E32>, one cluster.
    let out = shape_cps(&data, b"Thai", Direction::Ltr, &[0x0E14, 0x0E4B, 0x0E33]);
    assert_eq!(
        glyphs(&out),
        [
            g(&THAI, 0x0E14),
            g(&THAI, 0x0E4D),
            g(&THAI, 0x0E4B),
            g(&THAI, 0x0E32)
        ]
    );
    assert_eq!(clusters(&out), [0, 0, 0, 0]);
}

#[test]
fn thai_uses_private_use_glyphs_without_thai_gsub() {
    // PO PLA (an ascending consonant) + MAI EK: shift down-left, the
    // Windows PUA form U+F705.
    let data = font_for(&THAI, 500, vec![]);
    let out = shape_cps(&data, b"Thai", Direction::Ltr, &[0x0E1B, 0x0E48]);
    assert_eq!(glyphs(&out), [g(&THAI, 0x0E1B), g(&THAI, 0xF705)]);

    // A Thai GSUB script turns it off.
    let gsub = support::layout(&[(b"thai", vec![support::lang(None, &[])])], &[], &[]);
    let data = font_for(&THAI, 500, vec![(b"GSUB", gsub)]);
    let out = shape_cps(&data, b"Thai", Direction::Ltr, &[0x0E1B, 0x0E48]);
    assert_eq!(glyphs(&out), [g(&THAI, 0x0E1B), g(&THAI, 0x0E48)]);
}

// Arabic.

const ARABIC: [u32; 16] = [
    0x0628, 0xFE8F, 0xFE90, 0xFE91, 0xFE92, 0x0644, 0x0627, 0xFEDF, 0xFE8E, 0xFEFB, 0xFEDD, 0xFE8D,
    0x0650, 0x0655, 0xE000, 0xE001,
];

fn shape_arabic(data: &[u8], pre: usize, text: &[u32]) -> GlyphBuffer {
    let font = Font::new(data, 0).unwrap();
    let props = ResolvedProperties {
        direction: Direction::Rtl,
        script: Script::from_iso15924(b"Arab"),
        language: None,
    };
    let plan = ShapePlan::new(&font, props, &[]);
    assert_eq!(plan.shaper(), Shaper::Arabic);
    let mut b = UnicodeBuffer::new();
    b.push_codepoints(text, pre, Some(text.len() - pre));
    shape(&font, &Scale::unscaled(&font), &plan, b)
}

#[test]
fn arabic_fallback_forms_follow_joining() {
    // No GSUB: the fallback lookups built from the presentation forms in
    // the cmap give the joining forms. Right to left, so reversed.
    let data = font_for(&ARABIC, 300, vec![]);
    let a = |c| g(&ARABIC, c);
    let out = shape_arabic(&data, 0, &[0x0628, 0x0628, 0x0628]);
    assert_eq!(glyphs(&out), [a(0xFE90), a(0xFE92), a(0xFE91)]);
    let out = shape_arabic(&data, 0, &[0x0628]);
    assert_eq!(glyphs(&out), [a(0xFE8F)]);
    // A joining character before the text (the buffer's context) makes
    // the first one final.
    let out = shape_arabic(&data, 1, &[0x0628, 0x0628]);
    assert_eq!(glyphs(&out), [a(0xFE90)]);
    // LAM ALEF: init and fina forms, then the rlig ligature.
    let out = shape_arabic(&data, 0, &[0x0644, 0x0627]);
    assert_eq!(glyphs(&out), [a(0xFEFB)]);
}

#[test]
fn arabic_moves_modifier_combining_marks_first() {
    // BEH KASRA HAMZA-BELOW: the hamza below (220, a UAX #53 modifier)
    // moves before the kasra.
    let data = font_for(&ARABIC, 300, vec![]);
    let a = |c| g(&ARABIC, c);
    let out = shape_arabic(&data, 0, &[0x0628, 0x0650, 0x0655]);
    assert_eq!(glyphs(&out), [a(0x0650), a(0x0655), a(0xFE8F)]);
}

#[test]
fn arabic_stch_repeats_tiles_to_fill_the_word() {
    // stch turns ALEF into fixed, repeating, fixed tiles (E000, E001,
    // E000), 100 wide; the three BEH after it (to its left) are 300 each,
    // so the repeating tile is repeated to fill 900 - 200: seven times.
    let (alef, f, r) = (
        g(&ARABIC, 0x0627) as u16,
        g(&ARABIC, 0xE000) as u16,
        g(&ARABIC, 0xE001) as u16,
    );
    let gsub = simple_layout(
        &[b"stch"],
        vec![lookup(
            2,
            0,
            vec![support::multiple_subst(alef, &[f, r, f])],
        )],
    );
    let mut adv = vec![300u16; ARABIC.len() + 1];
    adv[f as usize] = 100;
    adv[r as usize] = 100;
    let map: Vec<(u32, u16)> = ARABIC
        .iter()
        .enumerate()
        .map(|(i, &c)| (c, i as u16 + 1))
        .collect();
    let data = support::font(ARABIC.len() as u16 + 1, &map, &adv, vec![(b"GSUB", gsub)]);
    let out = shape_arabic(&data, 0, &[0x0627, 0x0628, 0x0628, 0x0628]);
    let gl = glyphs(&out);
    assert_eq!(gl.len(), 3 + 2 + 7);
    assert_eq!(gl.iter().filter(|&&x| x == u32::from(r)).count(), 7);
    assert_eq!(gl.iter().filter(|&&x| x == u32::from(f)).count(), 2);
    // The tiles take no advance.
    let tiles: i32 = out
        .glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .filter(|(i, _)| i.glyph == u32::from(f) || i.glyph == u32::from(r))
        .map(|(_, p)| p.x_advance)
        .sum();
    assert_eq!(tiles, 0);
}
