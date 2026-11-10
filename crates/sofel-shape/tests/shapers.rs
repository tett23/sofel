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

// Indic.

/// A font for the Indic shaper: glyph `i + 1` is `chars[i]` (the forms
/// are private-use characters), and a `GSUB` with the script tag `ot`
/// whose features are ligatures `(feature, components, form)`, one
/// lookup per feature.
fn indic_font(
    ot: &[u8; 4],
    chars: &[u32],
    ligatures: &[(&'static [u8; 4], &[u32], u32)],
) -> Vec<u8> {
    let mut tags: Vec<&'static [u8; 4]> = Vec::new();
    for (t, _, _) in ligatures {
        if !tags.contains(t) {
            tags.push(t);
        }
    }
    let gl = |c: u32| g(chars, c) as u16;
    let lookups: Vec<support::LookupSpec> = tags
        .iter()
        .map(|t| {
            let subtables = ligatures
                .iter()
                .filter(|(f, _, _)| f == t)
                .map(|(_, comps, form)| {
                    let rest: Vec<u16> = comps[1..].iter().map(|&c| gl(c)).collect();
                    support::ligature_subst(gl(comps[0]), &[(&rest, gl(*form))])
                })
                .collect();
            lookup(4, 0, subtables)
        })
        .collect();
    let features: Vec<(&[u8; 4], Vec<u16>)> = tags
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, vec![i as u16]))
        .collect();
    let idx: Vec<u16> = (0..tags.len() as u16).collect();
    let gsub = support::layout(
        &[(ot, vec![support::lang(None, &idx)])],
        &features,
        &lookups,
    );
    font_for(chars, 500, vec![(b"GSUB", gsub)])
}

/// Shapes `text` left to right and gives the characters of the glyphs
/// (0 for `.notdef`).
fn indic_shape(data: &[u8], chars: &[u32], script: &[u8; 4], text: &[u32]) -> Vec<u32> {
    let out = shape_cps(data, script, Direction::Ltr, text);
    glyphs(&out)
        .iter()
        .map(|&gl| if gl == 0 { 0 } else { chars[gl as usize - 1] })
        .collect()
}

const DC: u32 = 0x25CC;
const ZWJ: u32 = 0x200D;
const FORM1: u32 = 0xE001;
const FORM2: u32 = 0xE002;
const FORM3: u32 = 0xE003;
const REPH: u32 = 0xE000;

mod deva {
    pub const KA: u32 = 0x0915;
    pub const RA: u32 = 0x0930;
    pub const SSA: u32 = 0x0937;
    pub const VIRAMA: u32 = 0x094D;
    pub const I: u32 = 0x093F;
    pub const A: u32 = 0x0905;
    pub const AA: u32 = 0x093E;
}

/// Devanagari: reph from Ra, H; half forms; Ra below the base with the
/// old (Ra, H) and new (H, Ra) orders of `blwf`.
fn deva_font(ot: &[u8; 4]) -> (Vec<u8>, Vec<u32>) {
    use deva::*;
    let chars = vec![KA, RA, SSA, VIRAMA, I, A, AA, DC, REPH, FORM1, FORM2, FORM3];
    let data = indic_font(
        ot,
        &chars,
        &[
            (b"rphf", &[RA, VIRAMA], REPH),
            (b"blwf", &[RA, VIRAMA], FORM2),
            (b"blwf", &[VIRAMA, RA], FORM3),
            (b"half", &[KA, VIRAMA], FORM1),
        ],
    );
    (data, chars)
}

/// The other scripts: Ra, the virama, a consonant, and the characters of
/// each script's rule.
mod script {
    pub const BENG: [u32; 5] = [0x0995, 0x09B0, 0x09CD, 0x09AF, 0x09BF]; // KA RA VIRAMA YA I
    pub const GURU: [u32; 4] = [0x0A15, 0x0A30, 0x0A4D, 0x0A39]; // KA RA VIRAMA HA
    pub const GUJR: [u32; 4] = [0x0A95, 0x0AB0, 0x0ACD, 0x0ABF]; // KA RA VIRAMA I
    pub const ORYA: [u32; 4] = [0x0B15, 0x0B30, 0x0B4D, 0x0B3E]; // KA RA VIRAMA AA
    pub const TAML: [u32; 5] = [0x0B95, 0x0BB0, 0x0BCD, 0x0BB7, 0x0BC6]; // KA RA VIRAMA SSA E
    pub const TELU: [u32; 3] = [0x0C15, 0x0C30, 0x0C4D]; // KA RA VIRAMA
    pub const KNDA: [u32; 3] = [0x0C95, 0x0CB0, 0x0CCD]; // KA RA VIRAMA
    pub const MLYM: [u32; 4] = [0x0D15, 0x0D30, 0x0D4D, 0x0D4E]; // KA RA VIRAMA DOT REPH
}

/// A font for `base` (KA, RA, VIRAMA, ...) with the reph `rphf` of Ra,
/// virama and `more` ligatures.
fn script_font(
    ot: &[u8; 4],
    base: &[u32],
    more: &[(&'static [u8; 4], &[u32], u32)],
) -> (Vec<u8>, Vec<u32>) {
    let mut chars = base.to_vec();
    chars.extend([ZWJ, DC, REPH, FORM1, FORM2, FORM3]);
    let mut ligs: Vec<(&'static [u8; 4], &[u32], u32)> = Vec::new();
    let rv = [base[1], base[2]];
    ligs.push((b"rphf", &rv, REPH));
    ligs.extend_from_slice(more);
    (indic_font(ot, &chars, &ligs), chars)
}

fn indic_clusters(data: &[u8], script: &[u8; 4], text: &[u32]) -> Vec<u32> {
    clusters(&shape_cps(data, script, Direction::Ltr, text))
}

#[test]
fn indic_devanagari_new_spec() {
    use deva::*;
    let (data, chars) = deva_font(b"dev2");
    assert_eq!(plan_shaper(&data, b"Deva"), Shaper::Indic);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Deva", text);
    // RA VIRAMA KA VIRAMA SSA I: Ra, virama forms the reph, which moves
    // to the end (before post-base forms: there are none); KA, virama
    // forms the half form; SSA is the base; the pre-base matra I stays
    // first (no halant is left after the half form), all in one cluster.
    assert_eq!(s(&[RA, VIRAMA, KA, VIRAMA, SSA, I]), [I, FORM1, SSA, REPH]);
    assert_eq!(
        indic_clusters(&data, b"Deva", &[RA, VIRAMA, KA, VIRAMA, SSA, I]),
        [0, 0, 0, 0]
    );
    // KA VIRAMA RA: `blwf` would form RA below, so KA is the base and the
    // new order (virama, RA) forms the below-base RA.
    assert_eq!(s(&[KA, VIRAMA, RA]), [KA, FORM3]);
    // A Ra, virama with no other consonant: Ra is the base, no reph.
    assert_eq!(s(&[RA, VIRAMA]), [RA, VIRAMA]);
    // RA VIRAMA KA I: the matra moves before the base, the reph after it.
    assert_eq!(s(&[RA, VIRAMA, KA, I]), [I, KA, REPH]);
}

#[test]
fn indic_devanagari_old_spec() {
    use deva::*;
    let (data, chars) = deva_font(b"deva");
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Deva", text);
    // The old specification moves the post-base virama after the last
    // consonant: RA, virama forms the below-base RA.
    assert_eq!(s(&[KA, VIRAMA, RA]), [KA, FORM2]);
    // KA VIRAMA RA VIRAMA SSA: SSA is the base; the half KA and the Ra,
    // virama before it are not reordered, and the eyelash rule gives Ra,
    // virama `blwf` too.
    assert_eq!(s(&[KA, VIRAMA, RA, VIRAMA, SSA]), [FORM1, FORM2, SSA]);
    assert_eq!(
        indic_clusters(&data, b"Deva", &[KA, VIRAMA, RA, VIRAMA, SSA]),
        [0, 2, 4]
    );
}

#[test]
fn indic_dotted_circle_and_vowel_constraints() {
    use deva::*;
    let (data, chars) = deva_font(b"dev2");
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Deva", text);
    // A matra alone is a broken cluster: a dotted circle is its base,
    // and the pre-base matra goes before it.
    assert_eq!(s(&[I]), [I, DC]);
    // A, AA looks like AA: a dotted circle is put between them.
    assert_eq!(s(&[A, AA]), [A, DC, AA]);

    // Neither with DO_NOT_INSERT_DOTTED_CIRCLE: A AA I is one vowel
    // syllable, whose pre-base matra goes first.
    let font = Font::new(&data, 0).unwrap();
    let props = ResolvedProperties {
        direction: Direction::Ltr,
        script: Script::from_iso15924(b"Deva"),
        language: None,
    };
    let plan = ShapePlan::new(&font, props, &[]);
    let mut b = UnicodeBuffer::new();
    b.set_flags(sofel_shape::BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE);
    b.push_codepoints(&[A, AA, I], 0, None);
    let out = shape(&font, &Scale::unscaled(&font), &plan, b);
    assert_eq!(glyphs(&out), [g(&chars, I), g(&chars, A), g(&chars, AA)]);
}

#[test]
fn indic_bengali_reph_after_below_base_forms() {
    use script::BENG as b;
    let (data, chars) = script_font(b"bng2", &b, &[(b"blwf", &[b[2], b[3]], FORM1)]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Beng", text);
    // RA VIRAMA KA VIRAMA YA: YA forms below the base KA; the reph goes
    // after the below-base form.
    assert_eq!(s(&[b[1], b[2], b[0], b[2], b[3]]), [b[0], FORM1, REPH]);
    // KA VIRAMA YA I: the pre-base matra goes before the base.
    assert_eq!(s(&[b[0], b[2], b[3], b[4]]), [b[4], b[0], FORM1]);
}

#[test]
fn indic_gurmukhi_reph_before_below_base_forms() {
    use script::GURU as b;
    let (data, chars) = script_font(b"gur2", &b, &[(b"blwf", &[b[2], b[3]], FORM1)]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Guru", text);
    // RA VIRAMA KA VIRAMA HA: the reph has no halant to follow; it ends
    // the syllable, after the below-base HA.
    assert_eq!(s(&[b[1], b[2], b[0], b[2], b[3]]), [b[0], FORM1, REPH]);
}

#[test]
fn indic_gujarati_reph_and_matra() {
    use script::GUJR as b;
    let (data, chars) = script_font(b"gjr2", &b, &[]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Gujr", text);
    assert_eq!(s(&[b[1], b[2], b[0], b[3]]), [b[3], b[0], REPH]);
    // RA VIRAMA KA KA: two syllables.
    assert_eq!(s(&[b[1], b[2], b[0], b[0]]), [b[0], REPH, b[0]]);
    assert_eq!(
        indic_clusters(&data, b"Gujr", &[b[1], b[2], b[0], b[0]]),
        [0, 0, 3]
    );
}

#[test]
fn indic_oriya_reph_after_main() {
    use script::ORYA as b;
    let (data, chars) = script_font(b"ory2", &b, &[]);
    // RA VIRAMA KA AA: the reph goes right after the main consonant,
    // before the post-base matra.
    assert_eq!(
        indic_shape(&data, &chars, b"Orya", &[b[1], b[2], b[0], b[3]]),
        [b[0], REPH, b[3]]
    );
}

#[test]
fn indic_tamil_matra_after_half_forms() {
    use script::TAML as b;
    let (data, chars) = script_font(b"tml2", &b, &[(b"half", &[b[0], b[2]], FORM1)]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Taml", text);
    // KA VIRAMA SSA E: the pre-base matra goes right before the base,
    // after the ligated KA, virama (Tamil looks for no halant).
    assert_eq!(s(&[b[0], b[2], b[3], b[4]]), [FORM1, b[4], b[3]]);
    assert_eq!(
        indic_clusters(&data, b"Taml", &[b[0], b[2], b[3], b[4]]),
        [0, 2, 2]
    );
    // RA VIRAMA KA E: the reph goes after the post-base forms, to the end.
    assert_eq!(s(&[b[1], b[2], b[0], b[4]]), [b[4], b[0], REPH]);
}

#[test]
fn indic_telugu_explicit_reph_and_below_base() {
    use script::TELU as b;
    let (data, chars) = script_font(b"tel2", &b, &[(b"blwf", &[b[2], b[0]], FORM1)]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Telu", text);
    // The reph is explicit: RA VIRAMA ZWJ KA forms it ...
    assert_eq!(s(&[b[1], b[2], ZWJ, b[0]]), [b[0], REPH]);
    // ... RA VIRAMA KA does not: KA forms below RA, the base.
    assert_eq!(s(&[b[1], b[2], b[0]]), [b[1], FORM1]);
    assert_eq!(s(&[b[0], b[2], b[0]]), [b[0], FORM1]);
}

#[test]
fn indic_kannada_ra_virama_zwj() {
    use script::KNDA as b;
    let (data, chars) = script_font(b"knd2", &b, &[]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Knda", text);
    // RA VIRAMA ZWJ behaves as RA ZWJ VIRAMA: no reph, and the ZWJ is
    // gone (default ignorable).
    assert_eq!(s(&[b[1], b[2], ZWJ, b[0]]), [b[1], b[2], b[0]]);
    assert_eq!(s(&[b[1], b[2], b[0]]), [b[0], REPH]);
}

#[test]
fn indic_malayalam_dot_reph_and_pref() {
    use script::MLYM as b;
    let (data, chars) = script_font(b"mlm2", &b, &[(b"pref", &[b[2], b[1]], FORM1)]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Mlym", text);
    // The encoded DOT REPH moves after the main consonant.
    assert_eq!(s(&[b[3], b[0]]), [b[0], b[3]]);
    // KA VIRAMA RA: `pref` forms RA, which moves before the base.
    assert_eq!(s(&[b[0], b[2], b[1]]), [FORM1, b[0]]);
    // DOT REPH alone: the dotted circle goes after it, and it moves after
    // the dotted circle.
    assert_eq!(s(&[b[3]]), [DC, b[3]]);
    assert_eq!(s(&[b[0], b[2], b[1], b[3]]), [FORM1, b[0], DC, b[3]]);
}

// USE.

/// A substitution of a [`use_font`]: a ligature of characters, or a
/// single substitution of one character.
enum Subst {
    Lig(&'static [u32], u32),
    Single(u32, u32),
}

/// A font for the USE shaper: glyph `i + 1` is `chars[i]`, and a `GSUB`
/// with the script tag `ot` whose features `(feature, substitution)`
/// have a lookup each.
fn use_font(ot: &[u8; 4], chars: &[u32], substs: &[(&'static [u8; 4], Subst)]) -> Vec<u8> {
    let gl = |c: u32| g(chars, c) as u16;
    let lookups: Vec<support::LookupSpec> = substs
        .iter()
        .map(|(_, s)| match s {
            Subst::Lig(comps, form) => {
                let rest: Vec<u16> = comps[1..].iter().map(|&c| gl(c)).collect();
                lookup(
                    4,
                    0,
                    vec![support::ligature_subst(gl(comps[0]), &[(&rest, gl(*form))])],
                )
            }
            Subst::Single(from, to) => lookup(1, 0, vec![single_subst(&[(gl(*from), gl(*to))])]),
        })
        .collect();
    let mut tags: Vec<&'static [u8; 4]> = Vec::new();
    for (t, _) in substs {
        if !tags.contains(t) {
            tags.push(t);
        }
    }
    let features: Vec<(&[u8; 4], Vec<u16>)> = tags
        .iter()
        .map(|t| {
            let ls = substs
                .iter()
                .enumerate()
                .filter(|(_, (f, _))| f == t)
                .map(|(i, _)| i as u16)
                .collect();
            (*t, ls)
        })
        .collect();
    let idx: Vec<u16> = (0..tags.len() as u16).collect();
    let gsub = support::layout(
        &[(ot, vec![support::lang(None, &idx)])],
        &features,
        &lookups,
    );
    font_for(chars, 500, vec![(b"GSUB", gsub)])
}

/// Balinese: KA and YA (base consonants), RA, ADEG ADEG (halant), TALING
/// (a pre-base vowel) and TEDUNG (a post-base vowel).
mod bali {
    pub const KA: u32 = 0x1B13;
    pub const YA: u32 = 0x1B2C;
    pub const RA: u32 = 0x1B2D;
    pub const ADEG: u32 = 0x1B44;
    pub const TALING: u32 = 0x1B3E;
    pub const TEDUNG: u32 = 0x1B35;
}

fn bali_font(substs: &[(&'static [u8; 4], Subst)]) -> (Vec<u8>, Vec<u32>) {
    use bali::*;
    let chars = vec![
        KA, YA, RA, ADEG, TALING, TEDUNG, DC, REPH, FORM1, FORM2, FORM3,
    ];
    (use_font(b"bali", &chars, substs), chars)
}

#[test]
fn use_reorders_repha_and_pre_base_vowels() {
    use bali::*;
    let (data, chars) = bali_font(&[
        (b"rphf", Subst::Lig(&[RA, ADEG], REPH)),
        (b"pref", Subst::Lig(&[ADEG, YA], FORM1)),
    ]);
    assert_eq!(plan_shaper(&data, b"Bali"), Shaper::Use);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Bali", text);
    // RA ADEG KA TALING: `rphf` forms the repha, which moves before the
    // first post-base glyph (the vowel); the pre-base vowel moves to the
    // start of the syllable. One cluster.
    assert_eq!(s(&[RA, ADEG, KA, TALING]), [TALING, KA, REPH]);
    assert_eq!(
        indic_clusters(&data, b"Bali", &[RA, ADEG, KA, TALING]),
        [0, 0, 0]
    );
    // RA ADEG KA TEDUNG: the repha goes before the post-base vowel.
    assert_eq!(s(&[RA, ADEG, KA, TEDUNG]), [KA, REPH, TEDUNG]);
    // RA ADEG KA: no post-base glyph, the repha goes to the end.
    assert_eq!(s(&[RA, ADEG, KA]), [KA, REPH]);
    // KA ADEG YA: `pref` forms a pre-base form, which moves like a
    // pre-base vowel.
    assert_eq!(s(&[KA, ADEG, YA]), [FORM1, KA]);
    // KA ADEG KA TALING: the vowel stays after the halant.
    assert_eq!(s(&[KA, ADEG, KA, TALING]), [KA, ADEG, TALING, KA]);
}

#[test]
fn use_dotted_circle() {
    use bali::*;
    let (data, chars) = bali_font(&[]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Bali", text);
    // A vowel alone is a broken cluster: a dotted circle is its base, and
    // the pre-base vowel goes before it.
    assert_eq!(s(&[TALING]), [TALING, DC]);
    assert_eq!(s(&[TEDUNG]), [DC, TEDUNG]);
    // Not with DO_NOT_INSERT_DOTTED_CIRCLE.
    let font = Font::new(&data, 0).unwrap();
    let props = ResolvedProperties {
        direction: Direction::Ltr,
        script: Script::from_iso15924(b"Bali"),
        language: None,
    };
    let plan = ShapePlan::new(&font, props, &[]);
    let mut b = UnicodeBuffer::new();
    b.set_flags(sofel_shape::BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE);
    b.push_codepoints(&[TALING], 0, None);
    let out = shape(&font, &Scale::unscaled(&font), &plan, b);
    assert_eq!(glyphs(&out), [g(&chars, TALING)]);
}

#[test]
fn use_topographical_features_by_cluster() {
    use bali::*;
    let (data, chars) = bali_font(&[
        (b"isol", Subst::Single(KA, FORM1)),
        (b"init", Subst::Single(KA, FORM2)),
        (b"medi", Subst::Single(KA, FORM3)),
        (b"fina", Subst::Single(KA, REPH)),
    ]);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Bali", text);
    assert_eq!(s(&[KA]), [FORM1]);
    assert_eq!(s(&[KA, KA]), [FORM2, REPH]);
    assert_eq!(s(&[KA, KA, KA]), [FORM2, FORM3, REPH]);
    // A cluster of several characters takes one form.
    assert_eq!(s(&[KA, TEDUNG, KA]), [FORM2, TEDUNG, REPH]);
}

#[test]
fn use_mongolian_free_variation_selectors_join() {
    // MONGOLIAN LETTER A, FVS1, A: Mongolian joins as Arabic; the free
    // variation selector takes the initial form of the letter before it,
    // so `init` substitutes it too.
    const A: u32 = 0x1820;
    const FVS1: u32 = 0x180B;
    let chars = vec![A, FVS1, FORM1, FORM2, FORM3];
    let data = use_font(
        b"mong",
        &chars,
        &[
            (b"init", Subst::Single(A, FORM1)),
            (b"init", Subst::Single(FVS1, FORM2)),
            (b"fina", Subst::Single(A, FORM3)),
        ],
    );
    assert_eq!(plan_shaper(&data, b"Mong"), Shaper::Use);
    let s = |text: &[u32]| indic_shape(&data, &chars, b"Mong", text);
    assert_eq!(s(&[A, FVS1, A]), [FORM1, FORM2, FORM3]);
    assert_eq!(s(&[A, A]), [FORM1, FORM3]);
}
