//! The Myanmar shapers (ADR 0024, myanmar): the syllables of
//! [`crate::tables::MYANMAR_MACHINE`], the initial reordering (kinzi,
//! medial Ra, pre-base vowels, asat) before the basic features, and the
//! Zawgyi shaper that does none of it.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-myanmar.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::Tag;

use super::indic::{flag, pos};
use super::syllabic::{clear_syllables, for_each_syllable, insert_dotted_circles};
use crate::buffer::{Buffer, Info, scratch};
use crate::font::ShapeFont;
use crate::layout::map::flags;
use crate::plan::{NormalizationMode, Planner, ShapePlan, ShaperFuncs, ZeroWidthMarks};
use crate::tables::MYANMAR_MACHINE;
use crate::tables::indic::indic_categories;

/// The categories of the Myanmar machine (`myanmar_syllable_machine_ex_*`)
/// that the shaper names, in `shaper_category`.
mod cat {
    pub(crate) const X: u8 = 0;
    pub(crate) const C: u8 = 1;
    pub(crate) const IV: u8 = 2;
    pub(crate) const H: u8 = 4;
    pub(crate) const A: u8 = 9;
    pub(crate) const GB: u8 = 10;
    pub(crate) const DOTTED_CIRCLE: u8 = 11;
    pub(crate) const RA: u8 = 15;
    pub(crate) const CS: u8 = 18;
    pub(crate) const VBLW: u8 = 21;
    pub(crate) const VPRE: u8 = 22;
    pub(crate) const AS: u8 = 32;
    pub(crate) const MR: u8 = 36;
    pub(crate) const VS: u8 = 40;
}

/// The syllable types of the Myanmar machine (`myanmar_syllable_type_t`).
mod syllable {
    pub(crate) const CONSONANT: u8 = 0;
    pub(crate) const BROKEN: u8 = 1;
}

/// `CONSONANT_FLAGS_MYANMAR`: independent vowels, placeholders and the
/// dotted circle count as consonants.
const CONSONANT_FLAGS: u32 = (1 << cat::C)
    | (1 << cat::CS)
    | (1 << cat::RA)
    | (1 << cat::IV)
    | (1 << cat::GB)
    | (1 << cat::DOTTED_CIRCLE);

/// `is_consonant_myanmar`: never for a glyph that ligated.
fn is_consonant(info: &Info) -> bool {
    !info.ligated() && flag(info.shaper_category) & CONSONANT_FLAGS != 0
}

/// `myanmar_basic_features`, applied one at a time after the reordering.
const BASIC_FEATURES: [[u8; 4]; 4] = [*b"rphf", *b"pref", *b"blwf", *b"pstf"];
/// `myanmar_other_features`, applied together after the syllables are
/// cleared.
const OTHER_FEATURES: [[u8; 4]; 4] = [*b"pres", *b"abvs", *b"blws", *b"psts"];

/// `collect_features_myanmar`.
fn collect_features(planner: &mut Planner<'_, '_>) {
    let map = &mut planner.map;
    map.add_gsub_pause(Some(setup_syllables));
    map.enable_feature(Tag(*b"locl"), flags::PER_SYLLABLE, 1);
    // The specifications do not ask for `ccmp`, but it is applied here
    // since its uses are typically at the start.
    map.enable_feature(Tag(*b"ccmp"), flags::PER_SYLLABLE, 1);
    map.add_gsub_pause(Some(reorder));
    for tag in BASIC_FEATURES {
        map.enable_feature(Tag(tag), flags::MANUAL_ZWJ | flags::PER_SYLLABLE, 1);
        map.add_gsub_pause(None);
    }
    map.add_gsub_pause(Some(clear_syllables));
    for tag in OTHER_FEATURES {
        map.enable_feature(Tag(tag), flags::MANUAL_ZWJ, 1);
    }
}

/// `setup_masks_myanmar`: the category of each character, no masks.
fn setup_masks(_: &ShapePlan<'_>, buffer: &mut Buffer, _: &ShapeFont<'_, '_, '_, '_>) {
    for info in buffer.infos_mut() {
        let [category, _] = indic_categories(info.codepoint).to_le_bytes();
        info.shaper_category = category;
    }
}

/// `find_syllables_myanmar`, setting `HAS_BROKEN_SYLLABLE` for a broken
/// cluster.
fn find_syllables(buffer: &mut Buffer) {
    let categories: Vec<u8> = buffer.infos().iter().map(|i| i.shaper_category).collect();
    let mut broken = false;
    MYANMAR_MACHINE.syllables(
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

/// `setup_syllables_myanmar`.
fn setup_syllables(_: &ShapePlan<'_>, _: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    find_syllables(buffer);
    for_each_syllable(buffer, |b, start, end| b.unsafe_to_break(start, end));
    false
}

/// The positions of `start..end` (in `shaper_position`, HarfBuzz's
/// `myanmar_position`): a kinzi (Ra, asat, halant) after the main
/// consonant, what precedes the base before it, medial Ra and pre-base
/// vowels before it, and the below-base vowels and what follows them.
fn set_positions(buffer: &mut Buffer, start: usize, end: usize) {
    let has_reph = start.wrapping_add(3) <= end
        && buffer.info[start].shaper_category == cat::RA
        && buffer.info[start.wrapping_add(1)].shaper_category == cat::AS
        && buffer.info[start.wrapping_add(2)].shaper_category == cat::H;
    let limit = if has_reph {
        start.wrapping_add(3)
    } else {
        start
    };
    let base = (limit..end)
        .find(|&i| is_consonant(&buffer.info[i]))
        .unwrap_or(start);

    let mut i = start;
    while i < limit {
        buffer.info[i].shaper_position = pos::AFTER_MAIN;
        i = i.wrapping_add(1);
    }
    while i < base {
        buffer.info[i].shaper_position = pos::PRE_C;
        i = i.wrapping_add(1);
    }
    if i < end {
        buffer.info[i].shaper_position = pos::BASE_C;
        i = i.wrapping_add(1);
    }
    let mut p = pos::AFTER_MAIN;
    while i < end {
        let category = buffer.info[i].shaper_category;
        let position = if category == cat::MR {
            pos::PRE_C
        } else if category == cat::VPRE {
            pos::PRE_M
        } else if category == cat::VS {
            buffer.info[i.wrapping_sub(1)].shaper_position
        } else if p == pos::AFTER_MAIN && category == cat::VBLW {
            p = pos::BELOW_C;
            p
        } else if p == pos::BELOW_C && category == cat::A {
            pos::BEFORE_SUB
        } else if p == pos::BELOW_C && category == cat::VBLW {
            p
        } else if p == pos::BELOW_C {
            p = pos::AFTER_SUB;
            p
        } else {
            p
        };
        buffer.info[i].shaper_position = position;
        i = i.wrapping_add(1);
    }
}

/// `initial_reordering_consonant_syllable`: the items sorted by
/// position (merging the clusters they move across), then the pre-base
/// vowels put back in their order (HarfBuzz issue 3863).
fn initial_reordering_consonant_syllable(buffer: &mut Buffer, start: usize, end: usize) {
    set_positions(buffer, start, end);
    buffer.sort(start, end, |a, b| {
        i32::from(a.shaper_position).wrapping_sub(i32::from(b.shaper_position))
    });

    let mut left = (start..end).filter(|&i| buffer.info[i].shaper_position == pos::PRE_M);
    let first_left_matra = left.clone().next().unwrap_or(end);
    let last_left_matra = left.next_back().unwrap_or(end);
    if first_left_matra < last_left_matra {
        buffer.reverse_range(first_left_matra, last_left_matra.wrapping_add(1));
        // Reverse back the variation selectors and the like.
        let mut i = first_left_matra;
        for j in first_left_matra..=last_left_matra {
            if buffer.info[j].shaper_category == cat::VPRE {
                buffer.reverse_range(i, j.wrapping_add(1));
                i = j.wrapping_add(1);
            }
        }
    }
}

/// `reorder_myanmar`: the dotted circles, then each consonant syllable
/// and broken cluster reordered.
fn reorder(_: &ShapePlan<'_>, font: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    let ret = insert_dotted_circles(
        font,
        buffer,
        syllable::BROKEN,
        cat::DOTTED_CIRCLE,
        None,
        None,
    );
    for_each_syllable(buffer, |b, start, end| {
        if matches!(
            b.info[start].syllable & 0x0F,
            syllable::CONSONANT | syllable::BROKEN
        ) {
            initial_reordering_consonant_syllable(b, start, end);
        }
    });
    ret
}

/// `_hb_ot_shaper_myanmar`.
pub(crate) static MYANMAR: ShaperFuncs = ShaperFuncs {
    collect_features: Some(collect_features),
    override_features: None,
    data_create: None,
    preprocess_text: None,
    postprocess_glyphs: None,
    decompose: None,
    compose: None,
    setup_masks: Some(setup_masks),
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::ComposedDiacriticsNoShortCircuit,
    zero_width_marks: ZeroWidthMarks::ByGdefEarly,
    fallback_position: false,
};

/// `_hb_ot_shaper_myanmar_zawgyi`: the Zawgyi encoding, with every
/// automatic step off (HarfBuzz issue 1162).
pub(crate) static MYANMAR_ZAWGYI: ShaperFuncs = ShaperFuncs {
    collect_features: None,
    override_features: None,
    data_create: None,
    preprocess_text: None,
    postprocess_glyphs: None,
    decompose: None,
    compose: None,
    setup_masks: None,
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::None,
    zero_width_marks: ZeroWidthMarks::None,
    fallback_position: false,
};
