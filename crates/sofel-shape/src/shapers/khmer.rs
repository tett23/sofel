//! The Khmer shaper (ADR 0024, khmer): the syllables of
//! [`crate::tables::KHMER_MACHINE`], the Coeng, Ro reordering with
//! `pref` and `cfar`, the pre-base vowels moved to the start of the
//! syllable, the feature masks, and the split vowels' decomposition.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-khmer.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;

use super::syllabic::{clear_syllables, for_each_syllable, insert_dotted_circles};
use crate::buffer::{Buffer, Mask, scratch};
use crate::font::ShapeFont;
use crate::layout::map::flags;
use crate::normalize::NormalizeContext;
use crate::plan::{NormalizationMode, Planner, ShapePlan, ShaperData, ShaperFuncs, ZeroWidthMarks};
use crate::tables::KHMER_MACHINE;
use crate::tables::indic::indic_categories;

/// The categories of the Khmer machine (`khmer_syllable_machine_ex_*`)
/// that the shaper names, in `shaper_category`.
mod cat {
    pub(crate) const X: u8 = 0;
    pub(crate) const H: u8 = 4;
    pub(crate) const DOTTED_CIRCLE: u8 = 11;
    pub(crate) const RA: u8 = 15;
    pub(crate) const VPRE: u8 = 22;
}

/// The syllable types of the Khmer machine (`khmer_syllable_type_t`).
mod syllable {
    pub(crate) const CONSONANT: u8 = 0;
    pub(crate) const BROKEN: u8 = 1;
}

/// `khmer_features`: the basic features, applied together before the
/// syllables are cleared, then the others.
const FEATURES: [([u8; 4], u32); 9] = {
    const M: u32 = flags::MANUAL_JOINERS | flags::PER_SYLLABLE;
    const G: u32 = flags::GLOBAL_MANUAL_JOINERS;
    [
        (*b"pref", M),
        (*b"blwf", M),
        (*b"abvf", M),
        (*b"pstf", M),
        (*b"cfar", M),
        (*b"pres", G),
        (*b"abvs", G),
        (*b"blws", G),
        (*b"psts", G),
    ]
};

/// Indices into [`FEATURES`] and [`KhmerPlan::mask_array`].
const PREF: usize = 0;
const BLWF: usize = 1;
const ABVF: usize = 2;
const PSTF: usize = 3;
const CFAR: usize = 4;
/// `KHMER_BASIC_FEATURES`.
const BASIC_FEATURES: usize = 5;

/// `khmer_shape_plan_t`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KhmerPlan {
    /// The masks of [`FEATURES`]; 0 for the global ones.
    mask_array: [Mask; 9],
}

impl KhmerPlan {
    fn mask(&self, i: usize) -> Mask {
        self.mask_array.get(i).copied().unwrap_or(0)
    }
}

/// `collect_features_khmer`. Uniscribe does not pause between the basic
/// features (HarfBuzz issue 974).
fn collect_features(planner: &mut Planner<'_, '_>) {
    let map = &mut planner.map;
    map.add_gsub_pause(Some(setup_syllables));
    map.add_gsub_pause(Some(reorder));
    map.enable_feature(Tag(*b"locl"), flags::PER_SYLLABLE, 1);
    map.enable_feature(Tag(*b"ccmp"), flags::PER_SYLLABLE, 1);
    for (tag, f) in FEATURES.iter().take(BASIC_FEATURES) {
        map.add_feature(Tag(*tag), *f, 1);
    }
    map.add_gsub_pause(Some(clear_syllables));
    for (tag, f) in FEATURES.iter().skip(BASIC_FEATURES) {
        map.add_feature(Tag(*tag), *f, 1);
    }
}

/// `override_features_khmer`: `clig` is one of Khmer's required
/// features; `liga` is off.
fn override_features(planner: &mut Planner<'_, '_>) {
    planner.map.enable_feature(Tag(*b"clig"), flags::NONE, 1);
    planner.map.disable_feature(Tag(*b"liga"));
}

/// `data_create_khmer`.
fn data_create(plan: &ShapePlan<'_>, _: &Font<'_>) -> ShaperData {
    let mut mask_array = [0; 9];
    for (m, (tag, f)) in mask_array.iter_mut().zip(FEATURES) {
        *m = if f & flags::GLOBAL != 0 {
            0
        } else {
            plan.map.one_mask(Tag(tag))
        };
    }
    ShaperData::Khmer(KhmerPlan { mask_array })
}

fn khmer_plan<'p>(plan: &'p ShapePlan<'_>) -> Option<&'p KhmerPlan> {
    match &plan.data {
        ShaperData::Khmer(kp) => Some(kp),
        _ => None,
    }
}

/// `setup_masks_khmer`: the category of each character; the masks are
/// set when reordering.
fn setup_masks(_: &ShapePlan<'_>, buffer: &mut Buffer, _: &ShapeFont<'_, '_>) {
    for info in buffer.infos_mut() {
        let [category, _] = indic_categories(info.codepoint).to_le_bytes();
        info.shaper_category = category;
    }
}

/// `find_syllables_khmer`, setting `HAS_BROKEN_SYLLABLE` for a broken
/// cluster.
fn find_syllables(buffer: &mut Buffer) {
    let categories: Vec<u8> = buffer.infos().iter().map(|i| i.shaper_category).collect();
    let mut broken = false;
    KHMER_MACHINE.syllables(
        categories.len(),
        |i| categories.get(i).copied().unwrap_or(cat::X),
        |i, s| {
            buffer.info[i].syllable = s;
            broken |= s & 0x0F == syllable::BROKEN;
        },
    );
    if broken {
        buffer.scratch_flags |= scratch::HAS_BROKEN_SYLLABLE;
    }
}

/// `setup_syllables_khmer`.
fn setup_syllables(_: &ShapePlan<'_>, _: &ShapeFont<'_, '_>, buffer: &mut Buffer) -> bool {
    find_syllables(buffer);
    for_each_syllable(buffer, |b, start, end| b.unsafe_to_break(start, end));
    false
}

/// `reorder_consonant_syllable`: the post-base masks; a Coeng, Ro moves
/// to the start of the syllable with `pref` (what follows it gets
/// `cfar`); a pre-base vowel moves to the start.
fn reorder_consonant_syllable(kp: &KhmerPlan, buffer: &mut Buffer, start: usize, end: usize) {
    let post = kp.mask(BLWF) | kp.mask(ABVF) | kp.mask(PSTF);
    for i in start.wrapping_add(1)..end {
        buffer.info[i].mask |= post;
    }

    let mut num_coengs = 0u32;
    for i in start.wrapping_add(1)..end {
        let category = buffer.info[i].shaper_category;
        if category == cat::H && num_coengs <= 2 && i.wrapping_add(1) < end {
            num_coengs = num_coengs.wrapping_add(1);
            if buffer.info[i.wrapping_add(1)].shaper_category == cat::RA {
                buffer.info[i].mask |= kp.mask(PREF);
                buffer.info[i.wrapping_add(1)].mask |= kp.mask(PREF);
                // Move the Coeng, Ro to the start.
                buffer.merge_clusters(start, i.wrapping_add(2));
                buffer
                    .info
                    .range_mut(start, i.wrapping_add(2))
                    .rotate_right(2);
                // `cfar` tells U+1784 U+17D2 U+179A U+17D2 U+1782 from
                // U+1784 U+17D2 U+1782 U+17D2 U+179A with MS Khmer fonts.
                let cfar = kp.mask(CFAR);
                if cfar != 0 {
                    for j in i.wrapping_add(2)..end {
                        buffer.info[j].mask |= cfar;
                    }
                }
                num_coengs = 2;
            }
        } else if category == cat::VPRE {
            buffer.merge_clusters(start, i.wrapping_add(1));
            buffer
                .info
                .range_mut(start, i.wrapping_add(1))
                .rotate_right(1);
        }
    }
}

/// `reorder_khmer`: the dotted circles, then each consonant syllable
/// and broken cluster reordered.
fn reorder(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_>, buffer: &mut Buffer) -> bool {
    let ret = insert_dotted_circles(
        font,
        buffer,
        syllable::BROKEN,
        cat::DOTTED_CIRCLE,
        None,
        None,
    );
    if let Some(kp) = khmer_plan(plan) {
        for_each_syllable(buffer, |b, start, end| {
            if matches!(
                b.info[start].syllable & 0x0F,
                syllable::CONSONANT | syllable::BROKEN
            ) {
                reorder_consonant_syllable(kp, b, start, end);
            }
        });
    }
    ret
}

/// `decompose_khmer`: the split vowels without a Unicode decomposition
/// start with U+17C1.
fn decompose(_: &NormalizeContext<'_, '_, '_>, ab: u32) -> Option<(u32, u32)> {
    match ab {
        0x17BE | 0x17BF | 0x17C0 | 0x17C4 | 0x17C5 => Some((0x17C1, ab)),
        _ => sofel_unicode::decompose(ab),
    }
}

/// `compose_khmer`: split vowels are not recomposed.
fn compose(_: &NormalizeContext<'_, '_, '_>, a: u32, b: u32) -> Option<u32> {
    if crate::unicode::is_mark_category(sofel_unicode::general_category(a)) {
        return None;
    }
    sofel_unicode::compose(a, b)
}

/// `_hb_ot_shaper_khmer`.
pub(crate) static KHMER: ShaperFuncs = ShaperFuncs {
    collect_features: Some(collect_features),
    override_features: Some(override_features),
    data_create: Some(data_create),
    preprocess_text: None,
    postprocess_glyphs: None,
    decompose: Some(decompose),
    compose: Some(compose),
    setup_masks: Some(setup_masks),
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::ComposedDiacriticsNoShortCircuit,
    zero_width_marks: ZeroWidthMarks::None,
    fallback_position: false,
};
