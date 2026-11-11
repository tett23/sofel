//! Public composite-glyph budget boundary (ADR 0030 I10).

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{
    Args, HeadSpec, Pt, USE_MY_METRICS, comp, composite_glyph, glyf_loca, head, hhea, hmtx, maxp05,
    sample_tables, sfnt, simple_glyph,
};
use sofel_font::Font;
use sofel_font::read::{Fixed, GlyphId};
use sofel_font::var::Location;

fn fvar_one_axis() -> Vec<u8> {
    let mut data = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    data.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    data.extend_from_slice(&[0, 0, 1, 0]);
    data
}

fn repeated_gvar() -> Vec<u8> {
    const TUPLES: u16 = 1000;
    let glyph_len = 4 + usize::from(TUPLES) * 8;
    let mut data = Vec::new();
    data.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    data.extend_from_slice(&1u16.to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(&30u32.to_be_bytes());
    data.extend_from_slice(&4u16.to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(&30u32.to_be_bytes());
    for offset in [0, 0, glyph_len / 2, glyph_len / 2, glyph_len / 2] {
        data.extend_from_slice(&u16::try_from(offset).unwrap().to_be_bytes());
    }
    data.extend_from_slice(&TUPLES.to_be_bytes());
    data.extend_from_slice(&(4 + TUPLES * 6).to_be_bytes());
    for _ in 0..TUPLES {
        data.extend_from_slice(&[0, 2, 0x80, 0, 0x40, 0]);
    }
    for _ in 0..TUPLES {
        data.extend_from_slice(&[0x86, 0x86]);
    }
    data
}

fn font_with_budget_exhausting_composite() -> Vec<u8> {
    let simple = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 0), Pt::On(0, 100)]],
        false,
        &[],
    );
    let mut components = vec![comp(1, Args::Xy(0, 0)); 5000];
    for component in &mut components {
        component.flags |= USE_MY_METRICS;
    }
    let mut last = comp(3, Args::Xy(0, 0));
    last.flags |= USE_MY_METRICS;
    components.push(last);
    let composite = composite_glyph(&components, [0, 0, 100, 200], false);
    let last_simple = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 0), Pt::On(0, 200)]],
        false,
        &[],
    );
    let (glyf, loca) = glyf_loca(&[Vec::new(), simple, composite, last_simple], false);
    let replacements = [
        (b"head", head(HeadSpec::default())),
        (b"maxp", maxp05(4)),
        (b"vhea", hhea(4)),
        (b"vmtx", hmtx(&[(1000, 50); 4], &[])),
        (b"glyf", glyf),
        (b"loca", loca),
        (b"fvar", fvar_one_axis()),
        (b"gvar", repeated_gvar()),
    ];
    let mut tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(tag, _)| {
            !replacements
                .iter()
                .any(|(replacement, _)| replacement == tag)
        })
        .collect();
    tables.extend(replacements);
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn adr0030_i10_public_composite_budget_does_not_return_partial_extents() {
    let bytes = font_with_budget_exhausting_composite();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_16_16(&font, &[Fixed(0x10000)]);
    let instance = font.instance(&location);
    // HarfBuzz 15.1.0: extents for glyph 2 fail; only the last
    // USE_MY_METRICS component supplies the vertical origin (250).
    assert!(instance.glyf_extents(GlyphId(1)).is_some());
    assert_eq!(instance.glyf_extents(GlyphId(2)), None);
    assert_eq!(instance.v_origin(GlyphId(2), |_| None), (251, 250));
}
