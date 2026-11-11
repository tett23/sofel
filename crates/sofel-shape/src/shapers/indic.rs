//! The Indic shaper (ADR 0024, indic): the nine scripts' settings, the
//! old (`deva`) and new (`dev2`) specifications, the syllables of
//! [`crate::tables::INDIC_MACHINE`], the initial reordering (base
//! consonant, reph, pre-base-reordering consonants, matras) before the
//! basic features and the final reordering after them, the feature
//! masks, and the decomposition and composition rules.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-indic.cc` and
//! `src/hb-ot-shaper-indic.hh`, MIT; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::gsub::SubstSubtable;

use super::syllabic::{clear_syllables, for_each_syllable, insert_dotted_circles};
use super::vowel_constraints;
use crate::buffer::{Buffer, Info, Mask, scratch};
use crate::font::ShapeFont;
use crate::layout::map::{Map, flags};
use crate::layout::would::{WouldApply, lookup_would_substitute};
use crate::layout::{GSUB, Table};
use crate::normalize::NormalizeContext;
use crate::plan::{NormalizationMode, Planner, ShapePlan, ShaperData, ShaperFuncs, ZeroWidthMarks};
use crate::tables::INDIC_MACHINE;
use crate::tables::indic::indic_categories;

/// The categories of the Indic machine (`indic_syllable_machine_ex_*`,
/// `I_Cat`), in `shaper_category`.
#[allow(dead_code)] // Not every category is named in the code.
pub(crate) mod cat {
    pub(crate) const X: u8 = 0;
    pub(crate) const C: u8 = 1;
    pub(crate) const V: u8 = 2;
    pub(crate) const N: u8 = 3;
    pub(crate) const H: u8 = 4;
    pub(crate) const ZWNJ: u8 = 5;
    pub(crate) const ZWJ: u8 = 6;
    pub(crate) const M: u8 = 7;
    pub(crate) const SM: u8 = 8;
    pub(crate) const A: u8 = 9;
    pub(crate) const VD: u8 = 9;
    pub(crate) const PLACEHOLDER: u8 = 10;
    pub(crate) const DOTTED_CIRCLE: u8 = 11;
    pub(crate) const RS: u8 = 12;
    pub(crate) const MPST: u8 = 13;
    pub(crate) const REPHA: u8 = 14;
    pub(crate) const RA: u8 = 15;
    pub(crate) const CM: u8 = 16;
    pub(crate) const SYMBOL: u8 = 17;
    pub(crate) const CS: u8 = 18;
    pub(crate) const SMPST: u8 = 57;
}

/// The visual positions in a syllable, left to right (`ot_position_t`),
/// in `shaper_position`.
#[allow(dead_code)] // Not every position is named in the code.
pub(crate) mod pos {
    pub(crate) const START: u8 = 0;
    pub(crate) const RA_TO_BECOME_REPH: u8 = 1;
    pub(crate) const PRE_M: u8 = 2;
    pub(crate) const PRE_C: u8 = 3;
    pub(crate) const BASE_C: u8 = 4;
    pub(crate) const AFTER_MAIN: u8 = 5;
    pub(crate) const ABOVE_C: u8 = 6;
    pub(crate) const BEFORE_SUB: u8 = 7;
    pub(crate) const BELOW_C: u8 = 8;
    pub(crate) const AFTER_SUB: u8 = 9;
    pub(crate) const BEFORE_POST: u8 = 10;
    pub(crate) const POST_C: u8 = 11;
    pub(crate) const AFTER_POST: u8 = 12;
    pub(crate) const SMVD: u8 = 13;
    pub(crate) const END: u8 = 14;
}

/// The syllable types of the Indic machine (`indic_syllable_type_t`).
mod syllable {
    pub(crate) const CONSONANT: u8 = 0;
    pub(crate) const VOWEL: u8 = 1;
    pub(crate) const STANDALONE: u8 = 2;
    pub(crate) const BROKEN: u8 = 4;
}

/// `FLAG_UNSAFE`: the bit of a value below 32, none otherwise.
pub(crate) fn flag(v: u8) -> u32 {
    1u32.checked_shl(u32::from(v)).unwrap_or(0)
}

/// `CONSONANT_FLAGS_INDIC`: vowels and placeholders count as consonants.
const CONSONANT_FLAGS: u32 = (1 << cat::C)
    | (1 << cat::CS)
    | (1 << cat::RA)
    | (1 << cat::CM)
    | (1 << cat::V)
    | (1 << cat::PLACEHOLDER)
    | (1 << cat::DOTTED_CIRCLE);
/// `JOINER_FLAGS`.
const JOINER_FLAGS: u32 = (1 << cat::ZWJ) | (1 << cat::ZWNJ);
const MATRA_FLAGS: u32 = (1 << cat::M) | (1 << cat::MPST);

/// `is_one_of`: never for a glyph that ligated.
fn is_one_of(info: &Info, flags: u32) -> bool {
    !info.ligated() && flag(info.shaper_category) & flags != 0
}
fn is_consonant(info: &Info) -> bool {
    is_one_of(info, CONSONANT_FLAGS)
}
fn is_joiner(info: &Info) -> bool {
    is_one_of(info, JOINER_FLAGS)
}
fn is_halant(info: &Info) -> bool {
    is_one_of(info, flag(cat::H))
}

/// `reph_mode_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RephMode {
    /// Reph formed out of an initial Ra, H.
    Implicit,
    /// Reph formed out of an initial Ra, H, ZWJ.
    Explicit,
    /// An encoded repha character, to be reordered.
    LogRepha,
}

/// `blwf_mode_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlwfMode {
    /// `blwf` applies to pre-base and post-base consonants.
    PreAndPost,
    /// `blwf` applies to post-base consonants only.
    PostOnly,
}

/// `indic_config_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct IndicConfig {
    script: [u8; 4],
    has_old_spec: bool,
    virama: u32,
    /// `reph_position_t`, a [`pos`] value.
    reph_pos: u8,
    reph_mode: RephMode,
    blwf_mode: BlwfMode,
}

/// The first of `indic_configs`, for the other scripts.
const DEFAULT_CONFIG: IndicConfig = IndicConfig {
    script: [0; 4],
    has_old_spec: false,
    virama: 0,
    reph_pos: pos::BEFORE_POST,
    reph_mode: RephMode::Implicit,
    blwf_mode: BlwfMode::PreAndPost,
};

/// The rest of `indic_configs`.
const CONFIGS: [IndicConfig; 9] = {
    const fn c(
        script: &[u8; 4],
        has_old_spec: bool,
        virama: u32,
        reph_pos: u8,
        reph_mode: RephMode,
        blwf_mode: BlwfMode,
    ) -> IndicConfig {
        IndicConfig {
            script: *script,
            has_old_spec,
            virama,
            reph_pos,
            reph_mode,
            blwf_mode,
        }
    }
    use BlwfMode::{PostOnly, PreAndPost};
    use RephMode::{Explicit, Implicit, LogRepha};
    [
        c(
            b"Deva",
            true,
            0x094D,
            pos::BEFORE_POST,
            Implicit,
            PreAndPost,
        ),
        c(b"Beng", true, 0x09CD, pos::AFTER_SUB, Implicit, PreAndPost),
        c(b"Guru", true, 0x0A4D, pos::BEFORE_SUB, Implicit, PreAndPost),
        c(
            b"Gujr",
            true,
            0x0ACD,
            pos::BEFORE_POST,
            Implicit,
            PreAndPost,
        ),
        c(b"Orya", true, 0x0B4D, pos::AFTER_MAIN, Implicit, PreAndPost),
        c(b"Taml", true, 0x0BCD, pos::AFTER_POST, Implicit, PreAndPost),
        c(b"Telu", true, 0x0C4D, pos::AFTER_POST, Explicit, PostOnly),
        c(b"Knda", true, 0x0CCD, pos::AFTER_POST, Implicit, PostOnly),
        c(b"Mlym", true, 0x0D4D, pos::AFTER_MAIN, LogRepha, PreAndPost),
    ]
};

/// `indic_features`: the basic features, applied one at a time after
/// the initial reordering, then the others, applied together after the
/// final reordering; all per syllable.
const FEATURES: [([u8; 4], u32); 17] = {
    const G: u32 = flags::GLOBAL_MANUAL_JOINERS | flags::PER_SYLLABLE;
    const M: u32 = flags::MANUAL_JOINERS | flags::PER_SYLLABLE;
    [
        (*b"nukt", G),
        (*b"akhn", G),
        (*b"rphf", M),
        (*b"rkrf", G),
        (*b"pref", M),
        (*b"blwf", M),
        (*b"abvf", M),
        (*b"half", M),
        (*b"pstf", M),
        (*b"vatu", G),
        (*b"cjct", G),
        (*b"init", M),
        (*b"pres", G),
        (*b"abvs", G),
        (*b"blws", G),
        (*b"psts", G),
        (*b"haln", G),
    ]
};

/// Indices into [`FEATURES`] and [`IndicPlan::mask_array`].
const RPHF: usize = 2;
const PREF: usize = 4;
const BLWF: usize = 5;
const ABVF: usize = 6;
const HALF: usize = 7;
const PSTF: usize = 8;
const INIT: usize = 11;
/// `INDIC_BASIC_FEATURES`.
const BASIC_FEATURES: usize = INIT;

/// `hb_indic_would_substitute_feature_t`: the lookups of a feature's
/// `GSUB` stage, asked whether they would substitute a glyph sequence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WouldSubstituteFeature {
    lookups: Vec<u16>,
    zero_context: bool,
}

impl WouldSubstituteFeature {
    /// `init`: the lookups of the stage of `tag`.
    pub(crate) fn new(map: &Map, tag: Tag, zero_context: bool) -> WouldSubstituteFeature {
        WouldSubstituteFeature {
            lookups: map
                .stage_lookups(GSUB, map.feature_stage(GSUB, tag))
                .iter()
                .map(|l| l.index)
                .collect(),
            zero_context,
        }
    }

    /// `would_substitute`, the work charged to `buffer`.
    pub(crate) fn would_substitute(
        &self,
        gsub: &Table<'_, SubstSubtable<'_>>,
        glyphs: &[u32],
        buffer: &mut Buffer,
    ) -> bool {
        if !buffer.successful {
            return false;
        }
        self.lookups.iter().any(|&l| {
            let mut c = WouldApply {
                glyphs,
                zero_context: self.zero_context,
                buffer: Some(&mut *buffer),
            };
            lookup_would_substitute(gsub, l, &mut c)
        })
    }
}

/// `indic_shape_plan_t`, with the virama glyph found when the plan is
/// made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndicPlan {
    config: IndicConfig,
    is_old_spec: bool,
    /// The glyph of the script's virama; 0 for none.
    virama_glyph: u32,
    rphf: WouldSubstituteFeature,
    pref: WouldSubstituteFeature,
    blwf: WouldSubstituteFeature,
    pstf: WouldSubstituteFeature,
    vatu: WouldSubstituteFeature,
    /// The masks of [`FEATURES`]; 0 for the global ones.
    mask_array: [Mask; 17],
}

impl IndicPlan {
    fn mask(&self, i: usize) -> Mask {
        self.mask_array.get(i).copied().unwrap_or(0)
    }
}

/// `collect_features_indic`.
fn collect_features(planner: &mut Planner<'_, '_>) {
    let map = &mut planner.map;
    map.add_gsub_pause(Some(setup_syllables));
    map.enable_feature(Tag(*b"locl"), flags::PER_SYLLABLE, 1);
    map.enable_feature(Tag(*b"ccmp"), flags::PER_SYLLABLE, 1);
    map.add_gsub_pause(Some(initial_reordering));
    for (tag, f) in FEATURES.iter().take(BASIC_FEATURES) {
        map.add_feature(Tag(*tag), *f, 1);
        map.add_gsub_pause(None);
    }
    map.add_gsub_pause(Some(final_reordering));
    for (tag, f) in FEATURES.iter().skip(BASIC_FEATURES) {
        map.add_feature(Tag(*tag), *f, 1);
    }
}

/// `override_features_indic`.
fn override_features(planner: &mut Planner<'_, '_>) {
    planner.map.disable_feature(Tag(*b"liga"));
    planner.map.add_gsub_pause(Some(clear_syllables));
}

/// `data_create_indic`, with the virama glyph looked up in `font`.
pub(crate) fn plan_data(plan: &ShapePlan<'_>, font: &Font<'_>) -> IndicPlan {
    let script = plan.props.script.map(|s| s.tag());
    let config = CONFIGS
        .iter()
        .find(|c| Some(c.script) == script)
        .copied()
        .unwrap_or(DEFAULT_CONFIG);
    let chosen = plan.map.chosen_script[GSUB].map_or(0, Tag::to_u32);
    let is_old_spec = config.has_old_spec && chosen & 0xFF != u32::from(b'2');
    // Zero-context matching for the new specifications, except
    // Malayalam's (HarfBuzz's heuristic after Windows).
    let zero_context = !is_old_spec && script != Some(*b"Mlym");
    let feature = |tag: &[u8; 4]| WouldSubstituteFeature::new(&plan.map, Tag(*tag), zero_context);
    let mut mask_array = [0; 17];
    for (m, (tag, f)) in mask_array.iter_mut().zip(FEATURES) {
        *m = if f & flags::GLOBAL != 0 {
            0
        } else {
            plan.map.one_mask(Tag(tag))
        };
    }
    let virama_glyph = if config.virama == 0 {
        0
    } else {
        font.nominal_glyph_raw(config.virama).map_or(0, |g| g.0)
    };
    IndicPlan {
        config,
        is_old_spec,
        virama_glyph,
        rphf: feature(b"rphf"),
        pref: feature(b"pref"),
        blwf: feature(b"blwf"),
        pstf: feature(b"pstf"),
        vatu: feature(b"vatu"),
        mask_array,
    }
}

fn data_create(plan: &ShapePlan<'_>, font: &Font<'_>) -> ShaperData {
    ShaperData::Indic(plan_data(plan, font))
}

fn indic_plan<'p>(plan: &'p ShapePlan<'_>) -> Option<&'p IndicPlan> {
    match &plan.data {
        ShaperData::Indic(ip) => Some(ip),
        _ => None,
    }
}

fn script_is(plan: &ShapePlan<'_>, tag: &[u8; 4]) -> bool {
    plan.props.script.is_some_and(|s| s.tag() == *tag)
}

/// `consonant_position_from_face`: below-base, post-base or base, by
/// what the font's `blwf`, `vatu`, `pstf` and `pref` would form from
/// the consonant with a virama, in either order.
fn consonant_position_from_face(
    ip: &IndicPlan,
    gsub: &Table<'_, SubstSubtable<'_>>,
    consonant: u32,
    virama: u32,
    buffer: &mut Buffer,
) -> u8 {
    let glyphs = [virama, consonant, virama];
    let (a, b) = (
        glyphs.get(..2).unwrap_or(&[]),
        glyphs.get(1..).unwrap_or(&[]),
    );
    let mut either = |f: &WouldSubstituteFeature| {
        f.would_substitute(gsub, a, buffer) || f.would_substitute(gsub, b, buffer)
    };
    if either(&ip.blwf) || either(&ip.vatu) {
        pos::BELOW_C
    } else if either(&ip.pstf) || either(&ip.pref) {
        pos::POST_C
    } else {
        pos::BASE_C
    }
}

/// `setup_masks_indic`: the category and position of each character.
fn setup_masks(_: &ShapePlan<'_>, buffer: &mut Buffer, _: &ShapeFont<'_, '_, '_, '_>) {
    for info in buffer.infos_mut() {
        let [category, position] = indic_categories(info.codepoint).to_le_bytes();
        info.shaper_category = category;
        info.shaper_position = position;
    }
}

/// `find_syllables_indic`, setting `HAS_BROKEN_SYLLABLE` for a broken
/// cluster.
pub(crate) fn find_syllables(buffer: &mut Buffer) {
    let categories: Vec<u8> = buffer.infos().iter().map(|i| i.shaper_category).collect();
    let mut broken = false;
    INDIC_MACHINE.syllables(
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

/// `setup_syllables_indic`.
fn setup_syllables(_: &ShapePlan<'_>, _: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    find_syllables(buffer);
    for_each_syllable(buffer, |b, start, end| b.unsafe_to_break(start, end));
    false
}

/// `update_consonant_positions_indic`.
fn update_consonant_positions(plan: &ShapePlan<'_>, ip: &IndicPlan, buffer: &mut Buffer) {
    let virama = ip.virama_glyph;
    if virama == 0 {
        return;
    }
    for i in 0..buffer.len {
        if buffer.info[i].shaper_position == pos::BASE_C {
            let consonant = buffer.info[i].codepoint;
            let p = consonant_position_from_face(ip, &plan.layout.gsub, consonant, virama, buffer);
            buffer.info[i].shaper_position = p;
        }
    }
}

/// Moves item `from` to `to`, shifting the items between by one
/// (HarfBuzz's `memmove` with a saved item).
fn move_item(buffer: &mut Buffer, from: usize, to: usize) {
    if from < to {
        buffer
            .info
            .range_mut(from, to.wrapping_add(1))
            .rotate_left(1);
    } else if to < from {
        buffer
            .info
            .range_mut(to, from.wrapping_add(1))
            .rotate_right(1);
    }
}

/// `initial_reordering_consonant_syllable`: also for vowel syllables,
/// standalone clusters and broken clusters with their dotted circle.
fn initial_reordering_consonant_syllable(
    plan: &ShapePlan<'_>,
    ip: &IndicPlan,
    buffer: &mut Buffer,
    start: usize,
    end: usize,
) {
    let gsub = &plan.layout.gsub;
    let category = |b: &Buffer, i: usize| b.info[i].shaper_category;

    // For compatibility with legacy usage in Kannada, Ra, H, ZWJ must
    // behave like Ra, ZWJ, H.
    if script_is(plan, b"Knda")
        && start.wrapping_add(3) <= end
        && is_one_of(&buffer.info[start], flag(cat::RA))
        && is_one_of(&buffer.info[start.wrapping_add(1)], flag(cat::H))
        && is_one_of(&buffer.info[start.wrapping_add(2)], flag(cat::ZWJ))
    {
        buffer.merge_clusters(start.wrapping_add(1), start.wrapping_add(3));
        let a = buffer.info[start.wrapping_add(1)];
        buffer.info[start.wrapping_add(1)] = buffer.info[start.wrapping_add(2)];
        buffer.info[start.wrapping_add(2)] = a;
    }

    // 1. Find the base consonant.
    let mut base = end;
    let mut has_reph = false;
    let mut limit = start;
    let mode = ip.config.reph_mode;
    if ip.mask(RPHF) != 0
        && start.wrapping_add(3) <= end
        && ((mode == RephMode::Implicit && !is_joiner(&buffer.info[start.wrapping_add(2)]))
            || (mode == RephMode::Explicit && category(buffer, start.wrapping_add(2)) == cat::ZWJ))
    {
        // See whether it matches the `rphf` feature.
        let glyphs = [
            buffer.info[start].codepoint,
            buffer.info[start.wrapping_add(1)].codepoint,
            if mode == RephMode::Explicit {
                buffer.info[start.wrapping_add(2)].codepoint
            } else {
                0
            },
        ];
        if ip
            .rphf
            .would_substitute(gsub, glyphs.get(..2).unwrap_or(&[]), buffer)
            || (mode == RephMode::Explicit && ip.rphf.would_substitute(gsub, &glyphs, buffer))
        {
            limit = limit.wrapping_add(2);
            while limit < end && is_joiner(&buffer.info[limit]) {
                limit = limit.wrapping_add(1);
            }
            base = start;
            has_reph = true;
        }
    } else if mode == RephMode::LogRepha && category(buffer, start) == cat::REPHA {
        limit = limit.wrapping_add(1);
        while limit < end && is_joiner(&buffer.info[limit]) {
            limit = limit.wrapping_add(1);
        }
        base = start;
        has_reph = true;
    }

    {
        // Starting from the end of the syllable, move backwards until a
        // consonant without a below-base or post-base form (post-base
        // forms have to follow below-base forms) is found, or the first
        // consonant.
        let mut i = end;
        let mut seen_below = false;
        loop {
            i = i.wrapping_sub(1);
            let info = buffer.info[i];
            if is_consonant(&info) {
                if info.shaper_position != pos::BELOW_C
                    && (info.shaper_position != pos::POST_C || seen_below)
                {
                    base = i;
                    break;
                }
                if info.shaper_position == pos::BELOW_C {
                    seen_below = true;
                }
                base = i;
            } else if start < i
                && info.shaper_category == cat::ZWJ
                && category(buffer, i.wrapping_sub(1)) == cat::H
            {
                // A ZWJ after a halant stops the search and asks for an
                // explicit half form.
                break;
            }
            if i <= limit {
                break;
            }
        }
    }

    // A Ra, H with no other consonant does not form a reph: the Ra is the
    // base.
    if has_reph && base == start && limit.wrapping_sub(base) <= 2 {
        has_reph = false;
    }

    // Reorder characters.
    for i in start..base {
        let p = &mut buffer.info[i].shaper_position;
        *p = (*p).min(pos::PRE_C);
    }
    if base < end {
        buffer.info[base].shaper_position = pos::BASE_C;
    }
    if has_reph {
        buffer.info[start].shaper_position = pos::RA_TO_BECOME_REPH;
    }

    // For the old specifications, move the first post-base halant after
    // the last consonant (not in Kannada when a halant follows the last
    // consonant already).
    if ip.is_old_spec {
        let disallow_double_halants = script_is(plan, b"Knda");
        if let Some(i) = (base.wrapping_add(1)..end).find(|&i| category(buffer, i) == cat::H) {
            let mut j = end.wrapping_sub(1);
            while j > i {
                if is_consonant(&buffer.info[j])
                    || (disallow_double_halants && category(buffer, j) == cat::H)
                {
                    break;
                }
                j = j.wrapping_sub(1);
            }
            if category(buffer, j) != cat::H && j > i {
                move_item(buffer, i, j);
            }
        }
    }

    // Attach misc marks to the previous character to move with them.
    {
        let mut last_pos = pos::START;
        for i in start..end {
            let c = category(buffer, i);
            if flag(c)
                & (JOINER_FLAGS | flag(cat::N) | flag(cat::RS) | flag(cat::CM) | flag(cat::H))
                != 0
            {
                buffer.info[i].shaper_position = last_pos;
                if c == cat::H && buffer.info[i].shaper_position == pos::PRE_M {
                    // Uniscribe doesn't move the halant with a left matra.
                    if let Some(j) = (start.wrapping_add(1)..=i)
                        .rev()
                        .find(|&j| buffer.info[j.wrapping_sub(1)].shaper_position != pos::PRE_M)
                    {
                        buffer.info[i].shaper_position =
                            buffer.info[j.wrapping_sub(1)].shaper_position;
                    }
                }
            } else if buffer.info[i].shaper_position != pos::SMVD {
                if c == cat::MPST && i > start && category(buffer, i.wrapping_sub(1)) == cat::SM {
                    buffer.info[i.wrapping_sub(1)].shaper_position = buffer.info[i].shaper_position;
                }
                last_pos = buffer.info[i].shaper_position;
            }
        }
    }
    // Post-base consonants own anything before them since the last
    // consonant or matra.
    {
        let mut last = base;
        for i in base.wrapping_add(1)..end {
            if is_consonant(&buffer.info[i]) {
                let p = buffer.info[i].shaper_position;
                for j in last.wrapping_add(1)..i {
                    if buffer.info[j].shaper_position < pos::SMVD {
                        buffer.info[j].shaper_position = p;
                    }
                }
                last = i;
            } else if flag(category(buffer, i)) & MATRA_FLAGS != 0 {
                last = i;
            }
        }
    }

    {
        // Use `syllable` for the sort accounting for a while.
        let syllable = buffer.info[start].syllable;
        for i in start..end {
            buffer.info[i].syllable = u8::try_from(i.wrapping_sub(start) & 0xFF).unwrap_or(0);
        }
        buffer
            .info
            .range_mut(start, end)
            .sort_by_key(|i| i.shaper_position);

        // Find the base again; also flip the left matra sequence.
        let mut first_left_matra = end;
        let mut last_left_matra = end;
        base = end;
        for i in start..end {
            let p = buffer.info[i].shaper_position;
            if p == pos::BASE_C {
                base = i;
                break;
            } else if p == pos::PRE_M {
                if first_left_matra == end {
                    first_left_matra = i;
                }
                last_left_matra = i;
            }
        }
        if first_left_matra < last_left_matra {
            buffer.reverse_range(first_left_matra, last_left_matra.wrapping_add(1));
            // Reverse back nuktas, etc.
            let mut i = first_left_matra;
            for j in first_left_matra..=last_left_matra {
                if flag(category(buffer, j)) & MATRA_FLAGS != 0 {
                    buffer.reverse_range(i, j.wrapping_add(1));
                    i = j.wrapping_add(1);
                }
            }
        }

        // Merge the clusters of what moved after the base; everything
        // after it with the old specifications.
        if ip.is_old_spec || end.wrapping_sub(start) > 127 {
            buffer.merge_clusters(base, end);
        } else {
            for i in base..end {
                if buffer.info[i].syllable == 255 {
                    continue;
                }
                let mut min = i;
                let mut max = i;
                let mut j = start.wrapping_add(usize::from(buffer.info[i].syllable));
                let mut steps = end.wrapping_sub(start);
                while j != i && steps > 0 {
                    min = min.min(j);
                    max = max.max(j);
                    let next = start.wrapping_add(usize::from(buffer.info[j].syllable));
                    buffer.info[j].syllable = 255;
                    j = next;
                    steps = steps.wrapping_sub(1);
                }
                buffer.merge_clusters(base.max(min), max.wrapping_add(1));
            }
        }

        for i in start..end {
            buffer.info[i].syllable = syllable;
        }
    }

    // Set up the masks.
    {
        let mut i = start;
        while i < end && buffer.info[i].shaper_position == pos::RA_TO_BECOME_REPH {
            buffer.info[i].mask |= ip.mask(RPHF);
            i = i.wrapping_add(1);
        }
        let mut pre = ip.mask(HALF);
        if !ip.is_old_spec && ip.config.blwf_mode == BlwfMode::PreAndPost {
            pre |= ip.mask(BLWF);
        }
        for i in start..base {
            buffer.info[i].mask |= pre;
        }
        let post = ip.mask(BLWF) | ip.mask(ABVF) | ip.mask(PSTF);
        for i in base.wrapping_add(1)..end {
            buffer.info[i].mask |= post;
        }
    }

    if ip.is_old_spec && script_is(plan, b"Deva") {
        // The old specification's eyelash Ra: `blwf` also applies to a
        // Ra, H before the base unless a ZWJ follows (Ra, H, ZWJ asks for
        // the eyelash form).
        let mut i = start;
        while i.wrapping_add(1) < base {
            if category(buffer, i) == cat::RA
                && category(buffer, i.wrapping_add(1)) == cat::H
                && (i.wrapping_add(2) == base || category(buffer, i.wrapping_add(2)) != cat::ZWJ)
            {
                buffer.info[i].mask |= ip.mask(BLWF);
                buffer.info[i.wrapping_add(1)].mask |= ip.mask(BLWF);
            }
            i = i.wrapping_add(1);
        }
    }

    // Find a halant, Ra sequence and mark it for pre-base reordering.
    let pref_len = 2usize;
    if ip.mask(PREF) != 0 && base.wrapping_add(pref_len) < end {
        let mut i = base.wrapping_add(1);
        while i.wrapping_add(pref_len).wrapping_sub(1) < end {
            let glyphs = [
                buffer.info[i].codepoint,
                buffer.info[i.wrapping_add(1)].codepoint,
            ];
            if ip.pref.would_substitute(gsub, &glyphs, buffer) {
                buffer.info[i].mask |= ip.mask(PREF);
                buffer.info[i.wrapping_add(1)].mask |= ip.mask(PREF);
                break;
            }
            i = i.wrapping_add(1);
        }
    }

    // Apply the ZWJ and ZWNJ effects: a ZWNJ disables `half`.
    for i in start.wrapping_add(1)..end {
        if is_joiner(&buffer.info[i]) {
            let non_joiner = category(buffer, i) == cat::ZWNJ;
            let mut j = i;
            loop {
                j = j.wrapping_sub(1);
                if non_joiner {
                    buffer.info[j].mask &= !ip.mask(HALF);
                }
                if !(j > start && !is_consonant(&buffer.info[j])) {
                    break;
                }
            }
        }
    }
}

/// `initial_reordering_syllable_indic`.
fn initial_reordering_syllable(
    plan: &ShapePlan<'_>,
    ip: &IndicPlan,
    buffer: &mut Buffer,
    start: usize,
    end: usize,
) {
    match buffer.info[start].syllable & 0x0F {
        syllable::CONSONANT | syllable::VOWEL | syllable::STANDALONE | syllable::BROKEN => {
            initial_reordering_consonant_syllable(plan, ip, buffer, start, end);
        }
        _ => {}
    }
}

/// `initial_reordering_indic`.
fn initial_reordering(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    let Some(ip) = indic_plan(plan) else {
        return false;
    };
    update_consonant_positions(plan, ip, buffer);
    let ret = insert_dotted_circles(
        font,
        buffer,
        syllable::BROKEN,
        cat::DOTTED_CIRCLE,
        Some(cat::REPHA),
        Some(pos::END),
    );
    for_each_syllable(buffer, |b, start, end| {
        initial_reordering_syllable(plan, ip, b, start, end);
    });
    ret
}

/// The step 2 of the reph positioning, also its step 5: after the first
/// explicit halant between the first post-reph consonant and the base,
/// and after a joiner that follows it.
fn reph_after_halant(buffer: &Buffer, start: usize, base: usize) -> Option<usize> {
    let mut p = start.wrapping_add(1);
    while p < base && !is_halant(&buffer.info[p]) {
        p = p.wrapping_add(1);
    }
    if p < base && is_halant(&buffer.info[p]) {
        if p.wrapping_add(1) < base && is_joiner(&buffer.info[p.wrapping_add(1)]) {
            p = p.wrapping_add(1);
        }
        Some(p)
    } else {
        None
    }
}

/// Where the reph moves to (steps 1 to 6 of the specification).
fn new_reph_position(
    ip: &IndicPlan,
    buffer: &Buffer,
    start: usize,
    end: usize,
    base: usize,
) -> usize {
    let reph_pos = ip.config.reph_pos;
    if reph_pos != pos::AFTER_POST {
        if let Some(p) = reph_after_halant(buffer, start, base) {
            return p;
        }
        // 3. After the main consonant.
        if reph_pos == pos::AFTER_MAIN {
            let mut p = base;
            while p.wrapping_add(1) < end
                && buffer.info[p.wrapping_add(1)].shaper_position <= pos::AFTER_MAIN
            {
                p = p.wrapping_add(1);
            }
            if p < end {
                return p;
            }
        }
        // 4. Before the post-base consonants.
        if reph_pos == pos::AFTER_SUB {
            let mut p = base;
            while p.wrapping_add(1) < end
                && flag(buffer.info[p.wrapping_add(1)].shaper_position)
                    & (flag(pos::POST_C) | flag(pos::AFTER_POST) | flag(pos::SMVD))
                    == 0
            {
                p = p.wrapping_add(1);
            }
            if p < end {
                return p;
            }
        }
    }
    // 5. As step 2.
    if let Some(p) = reph_after_halant(buffer, start, base) {
        return p;
    }
    // 6. The end of the syllable, before the syllable modifiers and
    // Vedic signs, and before the halant of a matra, halant.
    let mut p = end.wrapping_sub(1);
    while p > start && buffer.info[p].shaper_position == pos::SMVD {
        p = p.wrapping_sub(1);
    }
    if is_halant(&buffer.info[p]) {
        let mut i = base.wrapping_add(1);
        while i < p {
            if flag(buffer.info[i].shaper_category) & MATRA_FLAGS != 0 {
                p = p.wrapping_sub(1);
            }
            i = i.wrapping_add(1);
        }
    }
    p
}

/// `final_reordering_syllable_indic`.
fn final_reordering_syllable(
    plan: &ShapePlan<'_>,
    ip: &IndicPlan,
    buffer: &mut Buffer,
    start: usize,
    end: usize,
) {
    let not_mlym_or_taml = !script_is(plan, b"Mlym") && !script_is(plan, b"Taml");

    // Recover the halant category of a virama glyph that a ligature or a
    // multiple substitution lost.
    let virama = ip.virama_glyph;
    if virama != 0 {
        for i in start..end {
            let info = &mut buffer.info[i];
            if info.codepoint == virama && info.ligated() && info.multiplied() {
                info.shaper_category = cat::H;
                info.clear_ligated_and_multiplied();
            }
        }
    }

    let mut try_pref = ip.mask(PREF) != 0;

    // Find the base again.
    let mut base = start;
    while base < end {
        if buffer.info[base].shaper_position >= pos::BASE_C {
            if try_pref && base.wrapping_add(1) < end {
                if let Some(i) =
                    (base.wrapping_add(1)..end).find(|&i| buffer.info[i].mask & ip.mask(PREF) != 0)
                {
                    let info = buffer.info[i];
                    if !(info.substituted() && info.ligated_and_didnt_multiply()) {
                        // A `pref` candidate that formed nothing: the base
                        // is around here.
                        base = i;
                        while base < end && is_halant(&buffer.info[base]) {
                            base = base.wrapping_add(1);
                        }
                        if base < end {
                            buffer.info[base].shaper_position = pos::BASE_C;
                        }
                        try_pref = false;
                    }
                }
                if base == end {
                    break;
                }
            }
            // For Malayalam, skip over unformed below- (but not post-)
            // forms.
            if script_is(plan, b"Mlym") {
                let mut i = base.wrapping_add(1);
                while i < end {
                    while i < end && is_joiner(&buffer.info[i]) {
                        i = i.wrapping_add(1);
                    }
                    if i == end || !is_halant(&buffer.info[i]) {
                        break;
                    }
                    i = i.wrapping_add(1);
                    while i < end && is_joiner(&buffer.info[i]) {
                        i = i.wrapping_add(1);
                    }
                    if i < end
                        && is_consonant(&buffer.info[i])
                        && buffer.info[i].shaper_position == pos::BELOW_C
                    {
                        base = i;
                        buffer.info[base].shaper_position = pos::BASE_C;
                    }
                    i = i.wrapping_add(1);
                }
            }
            if start < base && buffer.info[base].shaper_position > pos::BASE_C {
                base = base.wrapping_sub(1);
            }
            break;
        }
        base = base.wrapping_add(1);
    }
    if base == end && start < base && is_one_of(&buffer.info[base.wrapping_sub(1)], flag(cat::ZWJ))
    {
        base = base.wrapping_sub(1);
    }
    if base < end {
        while start < base && is_one_of(&buffer.info[base], flag(cat::N) | flag(cat::H)) {
            base = base.wrapping_sub(1);
        }
    }

    // Reorder the matras: a pre-base matra moves after the last standalone
    // halant (not one followed by a ZWJ) before the main consonant.
    if start.wrapping_add(1) < end && start < base {
        // If the base was lost, before the last thing.
        let mut new_pos = if base == end {
            base.wrapping_sub(2)
        } else {
            base.wrapping_sub(1)
        };
        // Malayalam and Tamil have no half forms or explicit viramas;
        // the glyphs `half` forms are chillus or ligated explicit
        // viramas, and the matra goes after them.
        if not_mlym_or_taml {
            loop {
                while new_pos > start
                    && !is_one_of(&buffer.info[new_pos], MATRA_FLAGS | flag(cat::H))
                {
                    new_pos = new_pos.wrapping_sub(1);
                }
                // Only a halant that does not belong to the matra.
                if is_halant(&buffer.info[new_pos])
                    && buffer.info[new_pos].shaper_position != pos::PRE_M
                {
                    // A ZWJ after the halant: keep searching.
                    if new_pos.wrapping_add(1) < end
                        && buffer.info[new_pos.wrapping_add(1)].shaper_category == cat::ZWJ
                        && new_pos > start
                    {
                        new_pos = new_pos.wrapping_sub(1);
                        continue;
                    }
                } else {
                    new_pos = start;
                }
                break;
            }
        }

        if start < new_pos && buffer.info[new_pos].shaper_position != pos::PRE_M {
            // See whether there are matras to move.
            let mut i = new_pos;
            while i > start {
                if buffer.info[i.wrapping_sub(1)].shaper_position == pos::PRE_M {
                    let old_pos = i.wrapping_sub(1);
                    if old_pos < base && base <= new_pos {
                        base = base.wrapping_sub(1);
                    }
                    move_item(buffer, old_pos, new_pos);
                    // The merge is after the move on purpose.
                    buffer.merge_clusters(new_pos, end.min(base.wrapping_add(1)));
                    new_pos = new_pos.wrapping_sub(1);
                }
                i = i.wrapping_sub(1);
            }
        } else if let Some(i) =
            (start..base).find(|&i| buffer.info[i].shaper_position == pos::PRE_M)
        {
            buffer.merge_clusters(i, end.min(base.wrapping_add(1)));
        }
    }

    // Reorder the reph: a reph from Ra, H (or Ra, H, ZWJ) moves only if it
    // ligated, an encoded repha only if it did not.
    let first = buffer.info[start];
    if start.wrapping_add(1) < end
        && first.shaper_position == pos::RA_TO_BECOME_REPH
        && ((first.shaper_category == cat::REPHA) ^ first.ligated_and_didnt_multiply())
    {
        let new_reph_pos = new_reph_position(ip, buffer, start, end, base);
        buffer.merge_clusters(start, new_reph_pos.wrapping_add(1));
        move_item(buffer, start, new_reph_pos);
        if start < base && base <= new_reph_pos {
            base = base.wrapping_sub(1);
        }
    }

    // Reorder a pre-base-reordering consonant that `pref` formed: to the
    // position of a pre-base matra, or before the main consonant.
    if try_pref
        && base.wrapping_add(1) < end
        && let Some(i) =
            (base.wrapping_add(1)..end).find(|&i| buffer.info[i].mask & ip.mask(PREF) != 0)
        && buffer.info[i].ligated_and_didnt_multiply()
    {
        let mut new_pos = base;
        if not_mlym_or_taml {
            while new_pos > start
                && !is_one_of(
                    &buffer.info[new_pos.wrapping_sub(1)],
                    MATRA_FLAGS | flag(cat::H),
                )
            {
                new_pos = new_pos.wrapping_sub(1);
            }
        }
        if new_pos > start
            && is_halant(&buffer.info[new_pos.wrapping_sub(1)])
            && new_pos < end
            && is_joiner(&buffer.info[new_pos])
        {
            new_pos = new_pos.wrapping_add(1);
        }
        let old_pos = i;
        buffer.merge_clusters(new_pos, old_pos.wrapping_add(1));
        move_item(buffer, old_pos, new_pos);
        // HarfBuzz moves the base along here, but nothing after reads it.
    }

    // Apply `init` to a left matra at the start of a word.
    if buffer.info[start].shaper_position == pos::PRE_M {
        let word_start = start == 0
            || !(1..=12).contains(&(buffer.info[start.wrapping_sub(1)].general_category() as u8));
        if word_start {
            buffer.info[start].mask |= ip.mask(INIT);
        } else {
            buffer.unsafe_to_break(start.wrapping_sub(1), start.wrapping_add(1));
        }
    }
}

/// `final_reordering_indic`.
fn final_reordering(plan: &ShapePlan<'_>, _: &ShapeFont<'_, '_, '_, '_>, buffer: &mut Buffer) -> bool {
    if buffer.len == 0 {
        return false;
    }
    if let Some(ip) = indic_plan(plan) {
        for_each_syllable(buffer, |b, start, end| {
            final_reordering_syllable(plan, ip, b, start, end);
        });
    }
    false
}

/// `decompose_indic`: some letters stay whole.
fn decompose(_: &NormalizeContext<'_, '_, '_, '_, '_>, ab: u32) -> Option<(u32, u32)> {
    match ab {
        // DEVANAGARI LETTER RRA, BENGALI LETTER RRA and RHA, TAMIL
        // LETTER AU.
        0x0931 | 0x09DC | 0x09DD | 0x0B94 => None,
        _ => sofel_unicode::decompose(ab),
    }
}

/// `compose_indic`: split matras are not recomposed; BENGALI LETTER YYA
/// is, despite the composition exclusion.
fn compose(_: &NormalizeContext<'_, '_, '_, '_, '_>, a: u32, b: u32) -> Option<u32> {
    if crate::unicode::is_mark_category(sofel_unicode::general_category(a)) {
        return None;
    }
    if a == 0x09AF && b == 0x09BC {
        return Some(0x09DF);
    }
    sofel_unicode::compose(a, b)
}

/// `_hb_ot_shaper_indic`.
pub(crate) static INDIC: ShaperFuncs = ShaperFuncs {
    collect_features: Some(collect_features),
    override_features: Some(override_features),
    data_create: Some(data_create),
    preprocess_text: Some(vowel_constraints::preprocess_text),
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
