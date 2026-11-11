//! The Universal Shaping Engine (ADR 0024, USE): the syllables of
//! [`crate::tables::USE_MACHINE`] over the USE categories, the repha and
//! pre-base reordering, the basic, topographical and other feature
//! stages, and the joining of the joining scripts through the Arabic
//! shaper's functions.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-use.cc` and the
//! `find_syllables_use` of `src/hb-ot-shaper-use-machine.rl`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;

use super::arabic::{ArabicPlan, plan_data as arabic_plan_data, setup_masks_plan};
use super::syllabic::{clear_syllables, for_each_syllable, insert_dotted_circles};
use super::vowel_constraints;
use crate::buffer::{Buffer, Mask, scratch};
use crate::font::ShapeFont;
use crate::layout::map::flags;
use crate::normalize::NormalizeContext;
use crate::plan::{NormalizationMode, Planner, ShapePlan, ShaperData, ShaperFuncs, ZeroWidthMarks};
use crate::tables::USE_MACHINE;
use crate::tables::arabic::has_arabic_joining;
use crate::tables::use_::use_category;

/// The categories of the USE machine (`use_syllable_machine_ex_*`,
/// `USE(...)`), in `shaper_category`.
#[allow(dead_code)] // Not every category is named in the code.
pub(crate) mod cat {
    pub(crate) const O: u8 = 0;
    pub(crate) const B: u8 = 1;
    pub(crate) const N: u8 = 4;
    pub(crate) const GB: u8 = 5;
    pub(crate) const CGJ: u8 = 6;
    pub(crate) const SUB: u8 = 11;
    pub(crate) const H: u8 = 12;
    pub(crate) const HN: u8 = 13;
    pub(crate) const ZWNJ: u8 = 14;
    pub(crate) const WJ: u8 = 16;
    pub(crate) const R: u8 = 18;
    pub(crate) const VPRE: u8 = 22;
    pub(crate) const VMPRE: u8 = 23;
    pub(crate) const FABV: u8 = 24;
    pub(crate) const FBLW: u8 = 25;
    pub(crate) const FPST: u8 = 26;
    pub(crate) const MABV: u8 = 27;
    pub(crate) const MBLW: u8 = 28;
    pub(crate) const MPST: u8 = 29;
    pub(crate) const MPRE: u8 = 30;
    pub(crate) const CMABV: u8 = 31;
    pub(crate) const CMBLW: u8 = 32;
    pub(crate) const VABV: u8 = 33;
    pub(crate) const VBLW: u8 = 34;
    pub(crate) const VPST: u8 = 35;
    pub(crate) const VMABV: u8 = 37;
    pub(crate) const VMBLW: u8 = 38;
    pub(crate) const VMPST: u8 = 39;
    pub(crate) const SMABV: u8 = 41;
    pub(crate) const SMBLW: u8 = 42;
    pub(crate) const CS: u8 = 43;
    pub(crate) const IS: u8 = 44;
    pub(crate) const FMABV: u8 = 45;
    pub(crate) const FMBLW: u8 = 46;
    pub(crate) const FMPST: u8 = 47;
    pub(crate) const SK: u8 = 48;
    pub(crate) const G: u8 = 49;
    pub(crate) const J: u8 = 50;
    pub(crate) const SB: u8 = 51;
    pub(crate) const SE: u8 = 52;
    pub(crate) const HVM: u8 = 53;
    pub(crate) const HM: u8 = 54;
    pub(crate) const HR: u8 = 55;
    pub(crate) const RK: u8 = 56;
}

/// The syllable types of the USE machine (`use_syllable_type_t`).
mod syllable {
    pub(crate) const VIRAMA_TERMINATED: u8 = 0;
    pub(crate) const SAKOT_TERMINATED: u8 = 1;
    pub(crate) const STANDARD: u8 = 2;
    pub(crate) const NUMBER_JOINER_TERMINATED: u8 = 3;
    pub(crate) const NUMERAL: u8 = 4;
    pub(crate) const SYMBOL: u8 = 5;
    pub(crate) const HIEROGLYPH: u8 = 6;
    pub(crate) const BROKEN: u8 = 7;
    pub(crate) const NON_CLUSTER: u8 = 8;
}

/// `use_basic_features`: applied together before the reordering, per
/// syllable.
const BASIC_FEATURES: [[u8; 4]; 7] = [
    *b"rkrf", *b"abvf", *b"blwf", *b"half", *b"pstf", *b"vatu", *b"cjct",
];

/// `use_topographical_features`, in the order of `joining_form_t`.
const TOPOGRAPHICAL_FEATURES: [[u8; 4]; 4] = [*b"isol", *b"init", *b"medi", *b"fina"];

/// `joining_form_t`: indices into [`TOPOGRAPHICAL_FEATURES`].
mod form {
    pub(crate) const ISOL: usize = 0;
    pub(crate) const INIT: usize = 1;
    pub(crate) const MEDI: usize = 2;
    pub(crate) const FINA: usize = 3;
}

/// `use_other_features`: applied together after the reordering.
const OTHER_FEATURES: [[u8; 4]; 5] = [*b"abvs", *b"blws", *b"haln", *b"pres", *b"psts"];

/// `use_shape_plan_t`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsePlan {
    rphf_mask: Mask,
    /// The Arabic plan of a joining script.
    arabic_plan: Option<ArabicPlan>,
}

/// `collect_features_use`.
fn collect_features(planner: &mut Planner<'_, '_>) {
    let map = &mut planner.map;
    // Before any lookup is applied.
    map.add_gsub_pause(Some(setup_syllables));

    // The default glyph pre-processing group.
    map.enable_feature(Tag(*b"locl"), flags::PER_SYLLABLE, 1);
    map.enable_feature(Tag(*b"ccmp"), flags::PER_SYLLABLE, 1);
    map.enable_feature(Tag(*b"nukt"), flags::PER_SYLLABLE, 1);
    map.enable_feature(Tag(*b"akhn"), flags::MANUAL_ZWJ | flags::PER_SYLLABLE, 1);

    // The reordering group.
    map.add_gsub_pause(Some(clear_substitution_flags));
    map.add_feature(Tag(*b"rphf"), flags::MANUAL_ZWJ | flags::PER_SYLLABLE, 1);
    map.add_gsub_pause(Some(record_rphf));
    map.add_gsub_pause(Some(clear_substitution_flags));
    map.enable_feature(Tag(*b"pref"), flags::MANUAL_ZWJ | flags::PER_SYLLABLE, 1);
    map.add_gsub_pause(Some(record_pref));

    // The orthographic unit shaping group.
    for tag in BASIC_FEATURES {
        map.enable_feature(Tag(tag), flags::MANUAL_ZWJ | flags::PER_SYLLABLE, 1);
    }
    map.add_gsub_pause(Some(reorder));
    map.add_gsub_pause(Some(clear_syllables));

    // The topographical features.
    for tag in TOPOGRAPHICAL_FEATURES {
        map.add_feature(Tag(tag), flags::NONE, 1);
    }
    map.add_gsub_pause(None);

    // The standard typographic presentation.
    for tag in OTHER_FEATURES {
        map.enable_feature(Tag(tag), flags::MANUAL_ZWJ, 1);
    }
}

/// `data_create_use`, with the Arabic plan of a joining script.
fn data_create(plan: &ShapePlan<'_>, font: &Font<'_>) -> ShaperData {
    let joining = plan
        .props
        .script
        .is_some_and(|s| has_arabic_joining(s.tag()));
    ShaperData::Use(UsePlan {
        rphf_mask: plan.map.one_mask(Tag(*b"rphf")),
        arabic_plan: joining.then(|| arabic_plan_data(plan, font)),
    })
}

fn use_plan<'p>(plan: &'p ShapePlan<'_>) -> Option<&'p UsePlan> {
    match &plan.data {
        ShaperData::Use(up) => Some(up),
        _ => None,
    }
}

/// `setup_masks_use`: the joining of a joining script, then the USE
/// category of each character. The other masks are set up in the
/// [`setup_syllables`] pause.
fn setup_masks(plan: &ShapePlan<'_>, buffer: &mut Buffer, _: &ShapeFont<'_, '_, '_, '_>) {
    if let Some(ap) = use_plan(plan).and_then(|up| up.arabic_plan.as_ref()) {
        setup_masks_plan(ap, buffer);
    }
    for info in buffer.infos_mut() {
        info.shaper_category = use_category(info.codepoint);
    }
}

/// `setup_rphf_mask`: `rphf` applies to an encoded repha, or to the
/// first three items of a syllable.
fn setup_rphf_mask(up: &UsePlan, buffer: &mut Buffer) {
    let mask = up.rphf_mask;
    if mask == 0 {
        return;
    }
    for_each_syllable(buffer, |b, start, end| {
        let limit = if b.info[start].shaper_category == cat::R {
            1
        } else {
            end.wrapping_sub(start).min(3)
        };
        for i in start..start.wrapping_add(limit) {
            b.info[i].mask |= mask;
        }
    });
}

/// `setup_topographical_masks`: the joining form of each cluster of a
/// script without Arabic joining.
fn setup_topographical_masks(plan: &ShapePlan<'_>, up: &UsePlan, buffer: &mut Buffer) {
    if up.arabic_plan.is_some() {
        return;
    }
    let masks = TOPOGRAPHICAL_FEATURES.map(|tag| {
        let m = plan.map.one_mask(Tag(tag));
        if m == plan.map.global_mask { 0 } else { m }
    });
    let all_masks = masks.iter().fold(0, |a, m| a | m);
    if all_masks == 0 {
        return;
    }
    let other_masks = !all_masks;
    let form_mask = |f: usize| masks.get(f).copied().unwrap_or(0);

    let mut last_start = 0usize;
    let mut last_form: Option<usize> = None;
    for_each_syllable(buffer, |b, start, end| {
        match b.info[start].syllable & 0x0F {
            syllable::HIEROGLYPH | syllable::NON_CLUSTER => {
                // These do not join.
                last_form = None;
            }
            syllable::VIRAMA_TERMINATED
            | syllable::SAKOT_TERMINATED
            | syllable::STANDARD
            | syllable::NUMBER_JOINER_TERMINATED
            | syllable::NUMERAL
            | syllable::SYMBOL
            | syllable::BROKEN => {
                let join = matches!(last_form, Some(form::FINA | form::ISOL));
                if join {
                    // Fix up the previous syllable's form.
                    let f = if last_form == Some(form::FINA) {
                        form::MEDI
                    } else {
                        form::INIT
                    };
                    for i in last_start..start {
                        b.info[i].mask = (b.info[i].mask & other_masks) | form_mask(f);
                    }
                }
                // The form of this syllable.
                let f = if join { form::FINA } else { form::ISOL };
                last_form = Some(f);
                for i in start..end {
                    b.info[i].mask = (b.info[i].mask & other_masks) | form_mask(f);
                }
            }
            _ => {}
        }
        last_start = start;
    });
}

/// `find_syllables_use`: the machine skips CGJ, and a ZWNJ before a
/// Unicode mark (CGJ between them aside). Sets `HAS_BROKEN_SYLLABLE` for
/// a broken cluster.
fn find_syllables(buffer: &mut Buffer) {
    let categories: Vec<u8> = buffer.infos().iter().map(|i| i.shaper_category).collect();
    let marks: Vec<bool> = buffer
        .infos()
        .iter()
        .map(|i| crate::unicode::is_mark_category(i.general_category()))
        .collect();
    let mut kept: Vec<usize> = Vec::new();
    let mut broken = false;
    USE_MACHINE.syllables_use(
        categories.len(),
        |i| categories.get(i).copied().unwrap_or(cat::O),
        |i| marks.get(i).copied().unwrap_or(false),
        &mut kept,
        |i, s| {
            buffer.info[i].syllable = s;
            broken |= s & 0x0F == syllable::BROKEN;
        },
    );
    if broken {
        buffer.scratch_flags |= scratch::HAS_BROKEN_SYLLABLE;
    }
}

/// `setup_syllables_use`.
fn setup_syllables(
    plan: &ShapePlan<'_>,
    _: &ShapeFont<'_, '_, '_, '_>,
    buffer: &mut Buffer,
) -> bool {
    find_syllables(buffer);
    for_each_syllable(buffer, |b, start, end| b.unsafe_to_break(start, end));
    if let Some(up) = use_plan(plan) {
        setup_rphf_mask(up, buffer);
        setup_topographical_masks(plan, up, buffer);
    }
    false
}

/// `_hb_clear_substitution_flags`.
fn clear_substitution_flags(
    _: &ShapePlan<'_>,
    _: &ShapeFont<'_, '_, '_, '_>,
    buffer: &mut Buffer,
) -> bool {
    for info in buffer.infos_mut() {
        info.clear_substituted();
    }
    false
}

/// `record_rphf_use`: a repha that `rphf` substituted becomes `R`.
fn record_rphf(plan: &ShapePlan<'_>, _: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    let Some(up) = use_plan(plan) else {
        return false;
    };
    let mask = up.rphf_mask;
    if mask == 0 {
        return false;
    }
    for_each_syllable(buffer, |b, start, end| {
        if let Some(i) = (start..end)
            .take_while(|&i| b.info[i].mask & mask != 0)
            .find(|&i| b.info[i].substituted())
        {
            b.info[i].shaper_category = cat::R;
        }
    });
    false
}

/// `record_pref_use`: a glyph that `pref` substituted becomes `VPre`,
/// which behaves the same.
fn record_pref(_: &ShapePlan<'_>, _: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    for_each_syllable(buffer, |b, start, end| {
        if let Some(i) = (start..end).find(|&i| b.info[i].substituted()) {
            b.info[i].shaper_category = cat::VPRE;
        }
    });
    false
}

/// `FLAG64_UNSAFE`: the bit of a value below 64, none otherwise.
fn flag64(v: u8) -> u64 {
    1u64.checked_shl(u32::from(v)).unwrap_or(0)
}

/// `POST_BASE_FLAGS64`.
const POST_BASE_FLAGS: u64 = (1 << cat::FABV)
    | (1 << cat::FBLW)
    | (1 << cat::FPST)
    | (1 << cat::FMABV)
    | (1 << cat::FMBLW)
    | (1 << cat::FMPST)
    | (1 << cat::MABV)
    | (1 << cat::MBLW)
    | (1 << cat::MPST)
    | (1 << cat::MPRE)
    | (1 << cat::VABV)
    | (1 << cat::VBLW)
    | (1 << cat::VPST)
    | (1 << cat::VPRE)
    | (1 << cat::VMABV)
    | (1 << cat::VMBLW)
    | (1 << cat::VMPST)
    | (1 << cat::VMPRE);

/// `is_halant_use`: never for a glyph that ligated.
fn is_halant(info: &crate::buffer::Info) -> bool {
    matches!(info.shaper_category, cat::H | cat::HVM | cat::IS) && !info.ligated()
}

/// `reorder_syllable_use`.
fn reorder_syllable(buffer: &mut Buffer, start: usize, end: usize) {
    // Only a few syllable types need reordering.
    if !matches!(
        buffer.info[start].syllable & 0x0F,
        syllable::VIRAMA_TERMINATED
            | syllable::SAKOT_TERMINATED
            | syllable::STANDARD
            | syllable::SYMBOL
            | syllable::BROKEN
    ) {
        return;
    }

    // Move a repha forward, towards the end but before the first
    // post-base glyph.
    if buffer.info[start].shaper_category == cat::R && end.wrapping_sub(start) > 1 {
        for i in start.wrapping_add(1)..end {
            let info = &buffer.info[i];
            let is_post_base_glyph =
                flag64(info.shaper_category) & POST_BASE_FLAGS != 0 || is_halant(info);
            if is_post_base_glyph || i == end.wrapping_sub(1) {
                // Before a post-base glyph; otherwise to the end.
                let to = if is_post_base_glyph {
                    i.wrapping_sub(1)
                } else {
                    i
                };
                buffer.merge_clusters(start, to.wrapping_add(1));
                buffer
                    .info
                    .range_mut(start, to.wrapping_add(1))
                    .rotate_left(1);
                break;
            }
        }
    }

    // Move pre-base vowels back: to the start, or after the last halant.
    let mut j = start;
    for i in start..end {
        let info = buffer.info[i];
        if is_halant(&info) {
            j = i.wrapping_add(1);
        } else if matches!(info.shaper_category, cat::VPRE | cat::VMPRE)
            // Only the first component of a multiple substitution.
            && info.lig_comp() == 0
            && j < i
        {
            buffer.merge_clusters(j, i.wrapping_add(1));
            buffer.info.range_mut(j, i.wrapping_add(1)).rotate_right(1);
        }
    }
}

/// `reorder_use`: the dotted circles of broken clusters, then the
/// reordering of each syllable.
fn reorder(_: &ShapePlan<'_>, font: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    let ret = insert_dotted_circles(font, buffer, syllable::BROKEN, cat::B, Some(cat::R), None);
    for_each_syllable(buffer, reorder_syllable);
    ret
}

/// `compose_use`: split matras are not recomposed.
fn compose(_: &NormalizeContext<'_, '_, '_, '_, '_>, a: u32, b: u32) -> Option<u32> {
    if crate::unicode::is_mark_category(sofel_unicode::general_category(a)) {
        return None;
    }
    sofel_unicode::compose(a, b)
}

/// `_hb_ot_shaper_use`.
pub(crate) static USE: ShaperFuncs = ShaperFuncs {
    collect_features: Some(collect_features),
    override_features: None,
    data_create: Some(data_create),
    preprocess_text: Some(vowel_constraints::preprocess_text),
    postprocess_glyphs: None,
    decompose: None,
    compose: Some(compose),
    setup_masks: Some(setup_masks),
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::ComposedDiacriticsNoShortCircuit,
    zero_width_marks: ZeroWidthMarks::ByGdefEarly,
    fallback_position: false,
};
