//! The shaping steps with hand-built fonts (ADR 0020, テスト設計 手順の各段):
//! dotted circles, clusters, normalization, fractions, default
//! ignorables, mark widths, fallback positioning and spaces, the
//! unsafe-to-break flags, GSUB and GPOS, scaling and Device tables.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::bool_assert_comparison
)]

mod support;

use sofel_font::read::GlyphId;
use sofel_font::{Font, GlyphExtents, HbScale};
use sofel_shape::{
    BufferFlags, ClusterLevel, Direction, Feature, GlyphBuffer, GlyphFlags, Language,
    ResolvedProperties, Scale, Script, ShapePlan, UnicodeBuffer, shape, shape_fallback,
    shape_with_extents,
};
use support::{
    gdef, ligature_subst, lookup, mark_base, multiple_subst, pair_pos, simple_layout, single_pos,
    single_pos_device, single_subst,
};

/// The glyphs of the test fonts.
const NOTDEF: u32 = 0;
const A: u32 = 1;
const B: u32 = 2;
const F: u32 = 3;
const I: u32 = 4;
const FI: u32 = 5;
const ACUTE: u32 = 6;
const A_ACUTE: u32 = 7;
const CIRCLE: u32 = 8;
const SPACE: u32 = 9;
const ONE: u32 = 10;
const TWO: u32 = 11;
const SLASH: u32 = 12;
const PAREN_L: u32 = 13;
const PAREN_R: u32 = 14;
const COMMA: u32 = 15;
const VCOMMA: u32 = 16;

const ADVANCES: [u16; 17] = [
    500, 510, 520, 300, 250, 550, 0, 510, 600, 200, 400, 400, 100, 330, 330, 210, 1000,
];

fn cmap(with_composite: bool) -> Vec<(u32, u16)> {
    let mut m = vec![
        (0x61, A as u16),
        (0x62, B as u16),
        (0x66, F as u16),
        (0x69, I as u16),
        (0x0301, ACUTE as u16),
        (0x25CC, CIRCLE as u16),
        (0x20, SPACE as u16),
        (0x31, ONE as u16),
        (0x32, TWO as u16),
        (0x2044, SLASH as u16),
        (0x28, PAREN_L as u16),
        (0x29, PAREN_R as u16),
        (0x3001, COMMA as u16),
        (0xFE11, VCOMMA as u16),
    ];
    if with_composite {
        m.push((0xE1, A_ACUTE as u16));
    }
    m
}

fn make_font(with_composite: bool, extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    support::font(17, &cmap(with_composite), &ADVANCES, extra)
}

fn props(direction: Direction) -> ResolvedProperties {
    ResolvedProperties {
        direction,
        script: Script::from_iso15924(b"Latn"),
        language: None,
    }
}

struct Run {
    flags: BufferFlags,
    level: ClusterLevel,
    direction: Direction,
    features: Vec<Feature>,
    scale: Option<Scale>,
    extents: Option<fn(GlyphId) -> Option<GlyphExtents>>,
}

impl Default for Run {
    fn default() -> Run {
        Run {
            flags: BufferFlags::DEFAULT,
            level: ClusterLevel::MonotoneGraphemes,
            direction: Direction::Ltr,
            features: Vec::new(),
            scale: None,
            extents: None,
        }
    }
}

fn run(data: &[u8], text: &str, r: Run) -> GlyphBuffer {
    let font = Font::new(data, 0).unwrap();
    let mut b = UnicodeBuffer::new();
    b.push_utf8(text.as_bytes(), 0, None);
    b.set_flags(r.flags);
    b.set_cluster_level(r.level);
    let plan = ShapePlan::new(font.default_instance(), props(r.direction), &r.features);
    let scale = r.scale.unwrap_or_else(|| Scale::unscaled(&font));
    match r.extents {
        Some(e) => shape_with_extents(font.default_instance(), &scale, &plan, b, &e),
        None => shape(font.default_instance(), &scale, &plan, b),
    }
}

fn glyphs(g: &GlyphBuffer) -> Vec<(u32, u32)> {
    g.glyph_infos()
        .iter()
        .map(|i| (i.glyph, i.cluster))
        .collect()
}

fn advances(g: &GlyphBuffer) -> Vec<i32> {
    g.glyph_positions().iter().map(|p| p.x_advance).collect()
}

#[test]
fn cmap_and_advances() {
    let data = make_font(true, vec![]);
    let g = run(&data, "ab", Run::default());
    assert_eq!(glyphs(&g), [(A, 0), (B, 1)]);
    assert_eq!(advances(&g), [510, 520]);
    // A missing character is glyph 0, or the not-found glyph.
    let g = run(&data, "z", Run::default());
    assert_eq!(glyphs(&g), [(NOTDEF, 0)]);
}

#[test]
fn dotted_circle() {
    let data = make_font(false, vec![]);
    let bot = Run {
        flags: BufferFlags::BOT,
        ..Run::default()
    };
    let g = run(&data, "\u{0301}a", bot);
    assert_eq!(glyphs(&g), [(CIRCLE, 0), (ACUTE, 0), (A, 2)]);
    // Not without BOT, nor when asked not to.
    let g = run(&data, "\u{0301}a", Run::default());
    assert_eq!(glyphs(&g), [(ACUTE, 0), (A, 2)]);
    let no = Run {
        flags: BufferFlags::BOT.union(BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE),
        ..Run::default()
    };
    let g = run(&data, "\u{0301}a", no);
    assert_eq!(glyphs(&g), [(ACUTE, 0), (A, 2)]);
}

#[test]
fn cluster_levels() {
    let data = make_font(false, vec![]);
    let level = |level| Run {
        level,
        ..Run::default()
    };
    // a + combining acute, without a composite glyph.
    let g = run(&data, "a\u{0301}b", level(ClusterLevel::MonotoneGraphemes));
    assert_eq!(glyphs(&g), [(A, 0), (ACUTE, 0), (B, 3)]);
    let g = run(&data, "a\u{0301}b", level(ClusterLevel::Graphemes));
    assert_eq!(glyphs(&g), [(A, 0), (ACUTE, 0), (B, 3)]);
    let g = run(&data, "a\u{0301}b", level(ClusterLevel::MonotoneCharacters));
    assert_eq!(glyphs(&g), [(A, 0), (ACUTE, 1), (B, 3)]);
    let g = run(&data, "a\u{0301}b", level(ClusterLevel::Characters));
    assert_eq!(glyphs(&g), [(A, 0), (ACUTE, 1), (B, 3)]);
    // Characters marks the mark's cluster unsafe to break.
    assert!(
        g.glyph_infos()[1]
            .flags
            .contains(GlyphFlags::UNSAFE_TO_BREAK)
    );
}

#[test]
fn normalization() {
    // Composed when the font has the composite.
    let data = make_font(true, vec![]);
    let g = run(&data, "a\u{0301}", Run::default());
    assert_eq!(glyphs(&g), [(A_ACUTE, 0)]);
    let g = run(&data, "\u{e1}", Run::default());
    assert_eq!(glyphs(&g), [(A_ACUTE, 0)]);
    // Decomposed when it does not.
    let data = make_font(false, vec![]);
    let g = run(&data, "\u{e1}", Run::default());
    assert_eq!(glyphs(&g), [(A, 0), (ACUTE, 0)]);
}

#[test]
fn ligatures_and_multiple() {
    let gsub = simple_layout(
        &[b"liga", b"ccmp"],
        vec![
            lookup(
                4,
                0,
                vec![ligature_subst(F as u16, &[(&[I as u16], FI as u16)])],
            ),
            lookup(2, 0, vec![multiple_subst(B as u16, &[A as u16, A as u16])]),
        ],
    );
    let data = make_font(false, vec![(b"GSUB", gsub)]);
    let g = run(&data, "fib", Run::default());
    assert_eq!(glyphs(&g), [(FI, 0), (A, 2), (A, 2)]);
    // A feature turned off for a range.
    let off = Run {
        features: vec![Feature::parse(b"-liga[0:1]").unwrap()],
        ..Run::default()
    };
    let g = run(&data, "fifi", off);
    assert_eq!(glyphs(&g), [(F, 0), (I, 1), (FI, 2)]);
}

#[test]
fn adr0033_i4_ligature_candidate_order_and_unreadable_candidates() {
    let shape_ligs = |subtable: Vec<u8>, text: &str| {
        let gsub = simple_layout(&[b"liga"], vec![lookup(4, 0, vec![subtable])]);
        let data = make_font(false, vec![(b"GSUB", gsub)]);
        run(
            &data,
            text,
            Run {
                flags: BufferFlags::PRODUCE_UNSAFE_TO_CONCAT,
                ..Run::default()
            },
        )
    };
    // Empty, one valid, and multiple valid candidates. The first match
    // wins even when a later candidate has the same components.
    let empty = shape_ligs(ligature_subst(F as u16, &[]), "fi");
    assert_eq!(glyphs(&empty), [(F, 0), (I, 1)]);
    let single = ligature_subst(F as u16, &[(&[I as u16], FI as u16)]);
    let expected = shape_ligs(single.clone(), "fi");
    assert_eq!(glyphs(&expected), [(FI, 0)]);
    let many = ligature_subst(
        F as u16,
        &[
            (&[B as u16], B as u16),
            (&[I as u16], FI as u16),
            (&[I as u16], A as u16),
        ],
    );
    assert_eq!(glyphs(&shape_ligs(many.clone(), "fi")), [(FI, 0)]);
    assert_eq!(glyphs(&shape_ligs(many, "f")), [(F, 0)]);

    // More than one offset does not mean more than one valid candidate.
    // Null and out-of-bounds offsets must leave the single-candidate
    // path's glyphs, positions and unsafe flags unchanged.
    for bad in [0u16, u16::MAX] {
        for position in [0usize, 1] {
            let mut subtable = ligature_subst(
                F as u16,
                &[(&[I as u16], FI as u16), (&[I as u16], FI as u16)],
            );
            let set = u16::from_be_bytes([subtable[6], subtable[7]]) as usize;
            let at = set + 2 + position * 2;
            subtable[at..at + 2].copy_from_slice(&bad.to_be_bytes());
            let actual = shape_ligs(subtable, "fi");
            assert_eq!(actual.glyph_infos(), expected.glyph_infos());
            assert_eq!(actual.glyph_positions(), expected.glyph_positions());
        }
    }
}

#[test]
fn fractions() {
    let gsub = simple_layout(
        &[b"numr", b"dnom"],
        vec![
            lookup(1, 0, vec![single_subst(&[(ONE as u16, A as u16)])]),
            lookup(1, 0, vec![single_subst(&[(TWO as u16, B as u16)])]),
        ],
    );
    let data = make_font(false, vec![(b"GSUB", gsub)]);
    let g = run(&data, "1\u{2044}2", Run::default());
    assert_eq!(glyphs(&g), [(A, 0), (SLASH, 1), (B, 4)]);
    // Without digits on both sides nothing happens.
    let g = run(&data, "1\u{2044}", Run::default());
    assert_eq!(glyphs(&g), [(ONE, 0), (SLASH, 1)]);
}

#[test]
fn default_ignorables() {
    let data = make_font(false, vec![]);
    // Hidden as the space glyph, zero advance; ZWJ continues the
    // grapheme.
    let g = run(&data, "a\u{200D}b", Run::default());
    assert_eq!(glyphs(&g), [(A, 0), (SPACE, 0), (B, 4)]);
    assert_eq!(advances(&g), [510, 0, 520]);
    let removed = Run {
        flags: BufferFlags::REMOVE_DEFAULT_IGNORABLES,
        ..Run::default()
    };
    let g = run(&data, "a\u{200D}b", removed);
    assert_eq!(glyphs(&g), [(A, 0), (B, 4)]);
    let kept = Run {
        flags: BufferFlags::PRESERVE_DEFAULT_IGNORABLES,
        ..Run::default()
    };
    let g = run(&data, "a\u{200D}b", kept);
    assert_eq!(glyphs(&g), [(A, 0), (NOTDEF, 0), (B, 4)]);
    assert_eq!(advances(&g), [510, 500, 520]);
}

#[test]
fn mark_widths_are_zeroed() {
    // With GDEF marks and GPOS: zeroed late, offsets kept.
    let gpos = simple_layout(
        &[b"kern"],
        vec![lookup(1, 0, vec![single_pos(&[B as u16], 0, 0)])],
    );
    let data = make_font(
        false,
        vec![(b"GDEF", gdef(&[(ACUTE as u16, 3)])), (b"GPOS", gpos)],
    );
    let g = run(&data, "a\u{0301}", Run::default());
    assert_eq!(advances(&g), [510, 0]);
    assert_eq!(g.glyph_positions()[1].x_offset, 0);
}

fn boxes(g: GlyphId) -> Option<GlyphExtents> {
    // Bases are 0..advance wide and 0..500 tall; the mark is 100 wide.
    let w = i32::from(*ADVANCES.get(g.0 as usize)?);
    Some(if g.0 == ACUTE {
        GlyphExtents {
            x_bearing: -150,
            y_bearing: 700,
            width: 100,
            height: -100,
        }
    } else {
        GlyphExtents {
            x_bearing: 0,
            y_bearing: 500,
            width: w,
            height: -500,
        }
    })
}

#[test]
fn fallback_mark_positioning() {
    // No GPOS: the mark is placed over the base from the boxes.
    let data = make_font(false, vec![]);
    let r = Run {
        extents: Some(boxes),
        ..Run::default()
    };
    let g = run(&data, "a\u{0301}", r);
    let p = g.glyph_positions();
    assert_eq!(p[1].x_advance, 0);
    // Centered: (510 - 100) / 2 - (-150) = 355, minus the base advance.
    assert_eq!(p[1].x_offset, 355 - 510);
    // Above: base top 500 plus the gap 1000 / 16 = 62, minus the mark's
    // top 700 minus its height -100, gives -38; it points down while the
    // gap points up, so half of it is taken back.
    assert_eq!(p[1].y_offset, -38 + 19);
    // Without boxes the mark advance is zeroed and shifted back.
    let g = run(&data, "a\u{0301}", Run::default());
    assert_eq!(g.glyph_positions()[1].x_advance, 0);
    assert_eq!(g.glyph_positions()[1].x_offset, 0);
}

#[test]
fn fallback_spaces() {
    let data = make_font(false, vec![]);
    // EM SPACE and THIN SPACE are not in the font: the space glyph with
    // a fallback width.
    let g = run(&data, "\u{2003}\u{2009}\u{202F}", Run::default());
    assert_eq!(glyphs(&g), [(SPACE, 0), (SPACE, 3), (SPACE, 6)]);
    assert_eq!(advances(&g), [1000, 200, 100]);
}

#[test]
fn gpos_and_unsafe_to_break() {
    let gpos = simple_layout(
        &[b"kern"],
        vec![lookup(2, 0, vec![pair_pos(A as u16, B as u16, -50)])],
    );
    let data = make_font(false, vec![(b"GPOS", gpos)]);
    let g = run(&data, "abab", Run::default());
    assert_eq!(advances(&g), [460, 520, 460, 520]);
    let flags: Vec<u32> = g.glyph_infos().iter().map(|i| i.flags.0).collect();
    // The second glyph of each pair is unsafe to break before.
    assert_eq!(flags, [0, 1, 0, 1]);
    // Scaled to twice the size.
    let font = Font::new(&data, 0).unwrap();
    let r = Run {
        scale: Some(Scale::new(HbScale::for_font(&font, 2000, 2000), 0, 0)),
        ..Run::default()
    };
    let g = run(&data, "ab", r);
    assert_eq!(advances(&g), [920, 1040]);
}

#[test]
fn device_tables_need_a_ppem() {
    let gpos = simple_layout(
        &[b"kern"],
        vec![lookup(
            1,
            0,
            vec![single_pos_device(&[A as u16], 10, 12, &[2, 3])],
        )],
    );
    let data = make_font(false, vec![(b"GPOS", gpos)]);
    let font = Font::new(&data, 0).unwrap();
    let at = |ppem| Run {
        scale: Some(Scale::new(HbScale::unscaled(&font), ppem, ppem)),
        ..Run::default()
    };
    assert_eq!(advances(&run(&data, "a", at(0))), [520]);
    // 3 pixels at 13 ppem: 3 × 1000 / 13 = 230.
    assert_eq!(advances(&run(&data, "a", at(13))), [520 + 230]);
    assert_eq!(advances(&run(&data, "a", at(14))), [520]);
}

#[test]
fn mark_to_base() {
    let gpos = simple_layout(
        &[b"mark"],
        vec![lookup(
            4,
            0,
            vec![mark_base(&[(ACUTE as u16, 50, 0)], &[(A as u16, 250, 600)])],
        )],
    );
    let data = make_font(
        false,
        vec![
            (b"GDEF", gdef(&[(A as u16, 1), (ACUTE as u16, 3)])),
            (b"GPOS", gpos),
        ],
    );
    let g = run(&data, "a\u{0301}", Run::default());
    let p = g.glyph_positions();
    assert_eq!(
        (p[1].x_offset, p[1].y_offset, p[1].x_advance),
        (250 - 50 - 510, 600, 0)
    );
}

#[test]
fn right_to_left_mirrors_and_reverses() {
    let data = make_font(false, vec![]);
    let r = Run {
        direction: Direction::Rtl,
        ..Run::default()
    };
    let g = run(&data, "(ab", r);
    assert_eq!(glyphs(&g), [(B, 2), (A, 1), (PAREN_R, 0)]);
}

#[test]
fn vertical_forms_without_vert() {
    let data = make_font(false, vec![]);
    let r = Run {
        direction: Direction::Ttb,
        ..Run::default()
    };
    let g = run(&data, "\u{3001}", r);
    assert_eq!(glyphs(&g), [(VCOMMA, 0)]);
    // Vertical advances come from the font extents without vmtx.
    assert!(g.glyph_positions()[0].y_advance < 0);
}

#[test]
fn rand_uses_harfbuzz_generator() {
    let gsub = simple_layout(
        &[b"rand"],
        vec![lookup(
            3,
            0,
            vec![support::alternate_subst(
                A as u16,
                &[B as u16, F as u16, I as u16],
            )],
        )],
    );
    let data = make_font(false, vec![(b"GSUB", gsub)]);
    let g = run(&data, "aaaa", Run::default());
    // HarfBuzz's `state * 48271 % 2147483647` with the product wrapped
    // to 32 bits; each value mod 3 plus 1 picks the alternate.
    let mut state = 1u32;
    let want: Vec<u32> = (0..4)
        .map(|_| {
            state = state.wrapping_mul(48271) % 2_147_483_647;
            [B, F, I][(state % 3) as usize]
        })
        .collect();
    let got: Vec<u32> = g.glyph_infos().iter().map(|i| i.glyph).collect();
    assert_eq!(got, want);
}

#[test]
fn fallback_shaper() {
    let data = make_font(false, vec![]);
    let font = Font::new(&data, 0).unwrap();
    let mut b = UnicodeBuffer::new();
    b.push_utf8("ab\u{200D}".as_bytes(), 0, None);
    let g = shape_fallback(
        font.default_instance(),
        &Scale::unscaled(&font),
        &props(Direction::Rtl),
        b,
    );
    assert_eq!(glyphs(&g), [(SPACE, 2), (B, 1), (A, 0)]);
    assert_eq!(advances(&g), [0, 520, 510]);
}

#[test]
fn buffers_are_reused() {
    let data = make_font(false, vec![]);
    let font = Font::new(&data, 0).unwrap();
    let plan = ShapePlan::new(font.default_instance(), props(Direction::Ltr), &[]);
    let mut b = UnicodeBuffer::new();
    b.set_flags(BufferFlags::BOT);
    b.push_utf8(b"ab", 0, None);
    let g = shape(font.default_instance(), &Scale::unscaled(&font), &plan, b);
    let mut b = g.into_unicode();
    assert!(b.is_empty());
    assert_eq!(b.flags(), BufferFlags::BOT);
    b.push_utf8(b"ba", 0, None);
    let g = shape(font.default_instance(), &Scale::unscaled(&font), &plan, b);
    assert_eq!(glyphs(&g), [(B, 0), (A, 1)]);
    let _ = Language::new(b"en");
}

#[test]
fn adr0027_i_gpos_variation_index_applies_without_ppem() {
    use sofel_font::var::{Location, Variation};

    let mut sub = single_pos_device(&[A as u16], 10, 12, &[2]);
    let device_offset = u16::from_be_bytes([sub[8], sub[9]]) as usize;
    sub[device_offset..device_offset + 6].copy_from_slice(&[0, 0, 0, 0, 0x80, 0]);
    let gpos = simple_layout(&[b"kern"], vec![lookup(1, 0, vec![sub])]);
    let store = [
        0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, // region-list offset, one data offset
        0, 1, 0, 1, 0, 0, 0x40, 0, 0x40, 0, // one axis and one full region
        0, 1, 0, 1, 0, 1, 0, 0, 0, 21, // one row with delta 21
    ];
    let mut gdef = vec![0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18];
    gdef.extend_from_slice(&store);
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let data = make_font(
        false,
        vec![(b"GPOS", gpos), (b"GDEF", gdef), (b"fvar", fvar)],
    );
    let font = Font::new(&data, 0).unwrap();
    let mut b = UnicodeBuffer::new();
    b.push_utf8(b"a", 0, None);
    let scale = Scale::unscaled(&font);
    let loc = Location::new(&font, None, &[Variation::parse("wght=1").unwrap()]);
    let plan = ShapePlan::new(font.instance(&loc), props(Direction::Ltr), &[]);
    let glyphs = shape(font.instance(&loc), &scale, &plan, b);
    assert_eq!(advances(&glyphs), [541]);
}

#[test]
fn adr0049_i4_variable_shape_matches_uncached_advance_and_default_fields() {
    use sofel_font::var::Location;

    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let store = [
        0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, 0, 1, 0, 1, 0, 0, 0x40, 0, 0x40, 0, 0, 2, 0, 1, 0, 1,
        0, 0, 0, 21, 0xff, 0xe1,
    ];
    let mut hvar = vec![0, 1, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    hvar.extend_from_slice(&store);
    let gsub = simple_layout(
        &[b"liga"],
        vec![lookup(
            4,
            0,
            vec![ligature_subst(F as u16, &[(&[I as u16], FI as u16)])],
        )],
    );
    let bytes = make_font(
        false,
        vec![(b"fvar", fvar), (b"HVAR", hvar), (b"GSUB", gsub)],
    );
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[1.0]);
    let scale = Scale::unscaled(&font);
    for text in ["", "a", "aaaaaaaaaaaaaaaa", "fi", "afia"] {
        let run = |instance| {
            let plan = ShapePlan::new(instance, props(Direction::Ltr), &[]);
            let mut buffer = UnicodeBuffer::new();
            buffer.push_utf8(text.as_bytes(), 0, None);
            shape(instance, &scale, &plan, buffer)
        };
        let default = run(font.default_instance());
        let instance = font.instance(&location);
        let varied = run(instance);
        assert_eq!(varied.glyph_infos(), default.glyph_infos(), "{text}");
        for (info, position) in varied.glyph_infos().iter().zip(varied.glyph_positions()) {
            assert_eq!(
                position.x_advance,
                instance.advance(GlyphId(info.glyph)).cast_signed(),
                "{text}"
            );
            assert_eq!(
                (position.y_advance, position.x_offset, position.y_offset),
                (0, 0, 0),
                "{text}"
            );
        }
    }
}
