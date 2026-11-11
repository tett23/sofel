//! The Hebrew shaper (ADR 0024, hebrew): composition into the
//! presentation forms for fonts without `GPOS` mark positioning, and the
//! patah/qamats reordering.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-hebrew.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_font::read::Tag;

use crate::buffer::Buffer;
use crate::normalize::NormalizeContext;
use crate::plan::{NormalizationMode, ShapePlan, ShaperFuncs, ZeroWidthMarks};
use crate::unicode::mcc;

/// The presentation forms with dagesh of U+05D0..=U+05EA; 0 where none
/// is encoded.
const DAGESH_FORMS: [u32; 0x05EA - 0x05D0 + 1] = [
    0xFB30, // ALEF
    0xFB31, // BET
    0xFB32, // GIMEL
    0xFB33, // DALET
    0xFB34, // HE
    0xFB35, // VAV
    0xFB36, // ZAYIN
    0x0000, // HET
    0xFB38, // TET
    0xFB39, // YOD
    0xFB3A, // FINAL KAF
    0xFB3B, // KAF
    0xFB3C, // LAMED
    0x0000, // FINAL MEM
    0xFB3E, // MEM
    0x0000, // FINAL NUN
    0xFB40, // NUN
    0xFB41, // SAMEKH
    0x0000, // AYIN
    0xFB43, // FINAL PE
    0xFB44, // PE
    0x0000, // FINAL TSADI
    0xFB46, // TSADI
    0xFB47, // QOF
    0xFB48, // RESH
    0xFB49, // SHIN
    0xFB4A, // TAV
];

/// The presentation forms that canonical composition excludes, wanted for
/// old fonts.
fn presentation_form(a: u32, b: u32) -> Option<u32> {
    let c = match (b, a) {
        (0x05B4, 0x05D9) => 0xFB1D, // HIRIQ: YOD
        (0x05B7, 0x05F2) => 0xFB1F, // PATAH: YIDDISH YOD YOD
        (0x05B7, 0x05D0) => 0xFB2E, // PATAH: ALEF
        (0x05B8, 0x05D0) => 0xFB2F, // QAMATS: ALEF
        (0x05B9, 0x05D5) => 0xFB4B, // HOLAM: VAV
        (0x05BC, 0x05D0..=0x05EA) => DAGESH_FORMS
            .get(usize::try_from(a.wrapping_sub(0x05D0)).ok()?)
            .copied()
            .filter(|&c| c != 0)?,
        (0x05BC, 0xFB2A) => 0xFB2C, // DAGESH: SHIN WITH SHIN DOT
        (0x05BC, 0xFB2B) => 0xFB2D, // DAGESH: SHIN WITH SIN DOT
        (0x05BF, 0x05D1) => 0xFB4C, // RAFE: BET
        (0x05BF, 0x05DB) => 0xFB4D, // RAFE: KAF
        (0x05BF, 0x05E4) => 0xFB4E, // RAFE: PE
        (0x05C1, 0x05E9) => 0xFB2A, // SHIN DOT: SHIN
        (0x05C1, 0xFB49) => 0xFB2C, // SHIN DOT: SHIN WITH DAGESH
        (0x05C2, 0x05E9) => 0xFB2B, // SIN DOT: SHIN
        (0x05C2, 0xFB49) => 0xFB2D, // SIN DOT: SHIN WITH DAGESH
        _ => return None,
    };
    Some(c)
}

/// `compose_hebrew`.
fn compose(c: &NormalizeContext<'_, '_, '_, '_, '_>, a: u32, b: u32) -> Option<u32> {
    sofel_unicode::compose(a, b).or_else(|| {
        if c.plan.has_gpos_mark {
            None
        } else {
            presentation_form(a, b)
        }
    })
}

/// `reorder_marks_hebrew`: patah or qamats, then sheva or hiriq, then
/// meteg or a below mark: the last two swap.
fn reorder_marks(_: &ShapePlan<'_>, buffer: &mut Buffer, start: usize, end: usize) {
    let mut i = start.wrapping_add(2);
    while i < end {
        let c0 = buffer.info[i.wrapping_sub(2)].modified_combining_class();
        let c1 = buffer.info[i.wrapping_sub(1)].modified_combining_class();
        let c2 = buffer.info[i].modified_combining_class();
        if (c0 == mcc::CCC17 || c0 == mcc::CCC18)
            && (c1 == mcc::CCC10 || c1 == mcc::CCC14)
            && (c2 == mcc::CCC22 || c2 == mcc::BELOW)
        {
            buffer.merge_clusters(i.wrapping_sub(1), i.wrapping_add(1));
            let a = buffer.info[i.wrapping_sub(1)];
            buffer.info[i.wrapping_sub(1)] = buffer.info[i];
            buffer.info[i] = a;
            break;
        }
        i = i.wrapping_add(1);
    }
}

/// `_hb_ot_shaper_hebrew`.
pub(crate) static HEBREW: ShaperFuncs = ShaperFuncs {
    collect_features: None,
    override_features: None,
    data_create: None,
    preprocess_text: None,
    postprocess_glyphs: None,
    decompose: None,
    compose: Some(compose),
    setup_masks: None,
    reorder_marks: Some(reorder_marks),
    gpos_tag: Some(Tag(*b"hebr")),
    normalization_preference: NormalizationMode::Auto,
    zero_width_marks: ZeroWidthMarks::ByGdefLate,
    fallback_position: true,
};
