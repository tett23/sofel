//! The shifted outlines against HarfBuzz's glyph extents (ADR 0018):
//! for every simple glyph of HarfBuzz's test fonts with glyf, the
//! smallest x of the unscaled outline equals the `x_bearing` HarfBuzz
//! gives without variations (`GlyphHeader::get_extents_without_var_scaled`,
//! the left side bearing read by `hmtx` `accelerator_t`).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use sofel_conformance::outline::has_glyf;
use sofel_conformance::{fonts, paths};
use sofel_font::read::glyf::{self, Glyph};
use sofel_font::read::hhea::{self, Hhea};
use sofel_font::read::loca::{self, Loca};
use sofel_font::read::{CountWidth, GlyphId, hmtx};
use sofel_font::{Font, FontError};
use sofel_outline::{Scale, Scratch, load};

/// HarfBuzz's `get_leading_bearing_without_var_unscaled`, with the
/// tables `sofel-font` picks for the advances (ADR 0010).
fn harfbuzz_lsb(font: &Font<'_>, glyph: u32) -> i32 {
    let wide = font.table(hhea::TAG_WIDE).is_some() && font.table(hmtx::TAG_WIDE).is_some();
    let (hhea_tag, hmtx_tag, width) = if wide {
        (hhea::TAG_WIDE, hmtx::TAG_WIDE, CountWidth::U24)
    } else {
        (hhea::TAG, hmtx::TAG, CountWidth::U16)
    };
    let long = font
        .table(hhea_tag)
        .and_then(|d| Hhea::new(d, width).ok())
        .map_or(0, |h| h.number_of_h_metrics());
    let data = font.table(hmtx_tag).unwrap_or_default();
    let mut len = (data.len() as u32) & !1;
    let nlm = long.min(len / 4);
    len -= nlm * 4;
    let mut nb = font.num_glyphs().max(nlm);
    if (nb - nlm) * 2 > len {
        nb = nlm + len / 2;
    }
    if nlm == 0 {
        return 0;
    }
    let at = |pos: u32| {
        i32::from(i16::from_be_bytes([
            data[pos as usize],
            data[pos as usize + 1],
        ]))
    };
    if glyph < nlm {
        at(glyph * 4 + 2)
    } else if glyph >= nb {
        0
    } else {
        at(nlm * 4 + (glyph - nlm) * 2)
    }
}

/// The header `xMin` of a simple glyph with well-formed loca entries.
fn simple_x_min(font: &Font<'_>, glyph: u32) -> Option<i16> {
    let extended = font.table(glyf::TAG_WIDE).is_some();
    let (glyf_tag, loca_tag) = if extended {
        (glyf::TAG_WIDE, loca::TAG_WIDE)
    } else {
        (glyf::TAG, loca::TAG)
    };
    let loca = Loca::new(font.table(loca_tag)?, font.head().index_to_loc_format()).ok()?;
    let (start, end) = (
        loca.offset(glyph as usize)? as usize,
        loca.offset(glyph as usize + 1)? as usize,
    );
    let data = font.table(glyf_tag)?.get(start..end)?;
    let g = Glyph::new(data).ok()?;
    (g.number_of_contours() > 0).then(|| g.x_min())
}

#[test]
fn shifted_outlines_start_at_harfbuzz_x_bearing() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut scratch = Scratch::default();
    let (mut checked, mut skipped, mut failed) = (0usize, 0usize, Vec::new());
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for face in 0.. {
            let font = match Font::new(&bytes, face) {
                Ok(f) => f,
                Err(FontError::IndexOutOfRange) => break,
                Err(_) if face == 0 => break,
                Err(_) => continue,
            };
            if !has_glyf(&font) {
                continue;
            }
            for glyph in 0..font.num_glyphs() {
                let Some(x_min) = simple_x_min(&font, glyph) else {
                    continue;
                };
                let Ok(info) = load(
                    font.default_instance(),
                    GlyphId(glyph),
                    Scale::Unscaled,
                    &mut scratch,
                ) else {
                    continue;
                };
                // HarfBuzz trusts the header box; FreeType's outline is
                // where the points are. Compare where the two agree.
                let points_x_min = info.cbox.x_min
                    + (i32::from(x_min)
                        - i32::from(font.default_instance().ft_left_side_bearing(GlyphId(glyph))));
                if points_x_min != i32::from(x_min) {
                    skipped += 1;
                    continue;
                }
                checked += 1;
                let hb = harfbuzz_lsb(&font, glyph);
                if info.cbox.x_min != hb {
                    failed.push(format!(
                        "{id}#{face}:{glyph}: sofel {}, HarfBuzz {hb}",
                        info.cbox.x_min
                    ));
                }
            }
        }
    }
    eprintln!(
        "{checked} simple glyphs, {} differ; {skipped} left out (header xMin is not the points' smallest x)",
        failed.len()
    );
    assert!(checked > 0);
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
