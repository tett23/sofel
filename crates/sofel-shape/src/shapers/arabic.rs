//! The Arabic shaper (ADR 0024, arabic): the joining forms chosen by the
//! joining types (with the buffer's context), the feature stages, `stch`,
//! the UAX #53 mark reordering, the Mongolian free variation selectors,
//! and the fallback forms of [`super::arabic_fallback`].
//!
//! The joining of the USE shaper's joining scripts calls
//! [`setup_masks_plan`] too, as HarfBuzz does.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-arabic.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_unicode::GeneralCategory;

use super::arabic_fallback::FallbackPlan;
use crate::buffer::{Buffer, Info, Mask, scratch};
use crate::direction::Direction;
use crate::font::ShapeFont;
use crate::layout::map::flags;
use crate::plan::{NormalizationMode, Planner, ShapePlan, ShaperData, ShaperFuncs, ZeroWidthMarks};
use crate::tables::arabic;
use crate::unicode::mcc;

/// `HB_BUFFER_SCRATCH_FLAG_ARABIC_HAS_STCH`.
const HAS_STCH: u32 = scratch::SHAPER0;

/// Joining types (`JOINING_TYPE_*` and `JOINING_GROUP_*`).
pub(crate) mod jt {
    pub(crate) const U: u8 = 0;
    pub(crate) const R: u8 = 2;
    pub(crate) const T: u8 = 6;
    pub(crate) const X: u8 = 7;
}

/// `get_joining_type`.
pub(crate) fn joining_type(u: u32, gc: GeneralCategory) -> u8 {
    let t = arabic::joining_type(u);
    if t != jt::X {
        return t;
    }
    match gc {
        GeneralCategory::NonspacingMark
        | GeneralCategory::EnclosingMark
        | GeneralCategory::Format => jt::T,
        _ => jt::U,
    }
}

/// `FEATURE_IS_SYRIAC`.
fn feature_is_syriac(tag: [u8; 4]) -> bool {
    matches!(tag[3], b'2' | b'3')
}

/// The form features, in the order of the actions.
const FEATURES: [[u8; 4]; 7] = [
    *b"isol", *b"fina", *b"fin2", *b"fin3", *b"medi", *b"med2", *b"init",
];

/// The joining actions, stored in `shaper_aux`.
mod action {
    pub(crate) const ISOL: u8 = 0;
    pub(crate) const FINA: u8 = 1;
    pub(crate) const FIN2: u8 = 2;
    pub(crate) const FIN3: u8 = 3;
    pub(crate) const MEDI: u8 = 4;
    pub(crate) const MED2: u8 = 5;
    pub(crate) const INIT: u8 = 6;
    pub(crate) const NONE: u8 = 7;
    /// Not a form: a `stch` tile.
    pub(crate) const STCH_FIXED: u8 = 8;
    pub(crate) const STCH_REPEATING: u8 = 9;
}

fn is_stch(a: u8) -> bool {
    a == action::STCH_FIXED || a == action::STCH_REPEATING
}

/// `arabic_state_table`: `(prev_action, curr_action, next_state)` by
/// state and joining type (U, L, R, D, ALAPH, DALATH_RISH).
const STATE_TABLE: [[(u8, u8, u8); 6]; 7] = {
    use action::*;
    [
        // State 0: prev was U, not willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (NONE, ISOL, 1),
            (NONE, ISOL, 2),
            (NONE, ISOL, 1),
            (NONE, ISOL, 6),
        ],
        // State 1: prev was R or ISOL/ALAPH, not willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (NONE, ISOL, 1),
            (NONE, ISOL, 2),
            (NONE, FIN2, 5),
            (NONE, ISOL, 6),
        ],
        // State 2: prev was D/L in ISOL form, willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (INIT, FINA, 1),
            (INIT, FINA, 3),
            (INIT, FINA, 4),
            (INIT, FINA, 6),
        ],
        // State 3: prev was D in FINA form, willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (MEDI, FINA, 1),
            (MEDI, FINA, 3),
            (MEDI, FINA, 4),
            (MEDI, FINA, 6),
        ],
        // State 4: prev was FINA ALAPH, not willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (MED2, ISOL, 1),
            (MED2, ISOL, 2),
            (MED2, FIN2, 5),
            (MED2, ISOL, 6),
        ],
        // State 5: prev was FIN2/FIN3 ALAPH, not willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (ISOL, ISOL, 1),
            (ISOL, ISOL, 2),
            (ISOL, FIN2, 5),
            (ISOL, ISOL, 6),
        ],
        // State 6: prev was DALATH/RISH, not willing to join.
        [
            (NONE, NONE, 0),
            (NONE, ISOL, 2),
            (NONE, ISOL, 1),
            (NONE, ISOL, 2),
            (NONE, FIN3, 5),
            (NONE, ISOL, 6),
        ],
    ]
};

fn entry(state: u8, t: u8) -> (u8, u8, u8) {
    STATE_TABLE
        .get(usize::from(state))
        .and_then(|r| r.get(usize::from(t)))
        .copied()
        .unwrap_or((action::NONE, action::NONE, 0))
}

/// `arabic_shape_plan_t`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArabicPlan {
    /// The masks of the actions; `NONE` has none.
    pub(crate) mask_array: [Mask; 8],
    pub(crate) do_fallback: bool,
    pub(crate) has_stch: bool,
    /// The fallback lookups, built with the plan (HarfBuzz builds them on
    /// first use).
    pub(crate) fallback: Option<FallbackPlan>,
}

/// `collect_features_arabic`.
fn collect_features(planner: &mut Planner<'_, '_>) {
    let is_arabic = planner.props.script.is_some_and(|s| s.tag() == *b"Arab");
    let map = &mut planner.map;
    map.enable_feature(Tag(*b"stch"), flags::NONE, 1);
    map.add_gsub_pause(Some(record_stch));
    map.enable_feature(Tag(*b"ccmp"), flags::MANUAL_ZWJ, 1);
    map.enable_feature(Tag(*b"locl"), flags::MANUAL_ZWJ, 1);
    map.add_gsub_pause(None);
    for f in FEATURES {
        let fallback = if is_arabic && !feature_is_syriac(f) {
            flags::HAS_FALLBACK
        } else {
            flags::NONE
        };
        map.add_feature(Tag(f), flags::MANUAL_ZWJ | fallback, 1);
        map.add_gsub_pause(None);
    }
    map.add_gsub_pause(Some(deallocate_buffer_var));
    map.enable_feature(Tag(*b"rlig"), flags::MANUAL_ZWJ | flags::HAS_FALLBACK, 1);
    if is_arabic {
        map.add_gsub_pause(Some(fallback_shape));
    }
    map.enable_feature(Tag(*b"calt"), flags::MANUAL_ZWJ, 1);
    if !map.has_feature(Tag(*b"rclt")) {
        map.add_gsub_pause(None);
    }
    map.enable_feature(Tag(*b"liga"), flags::MANUAL_ZWJ, 1);
    map.enable_feature(Tag(*b"clig"), flags::MANUAL_ZWJ, 1);
    map.enable_feature(Tag(*b"mset"), flags::MANUAL_ZWJ, 1);
}

/// `data_create_arabic`, with the fallback plan built here.
pub(crate) fn plan_data(plan: &ShapePlan<'_>, font: &Font<'_>) -> ArabicPlan {
    let mut do_fallback = plan.props.script.is_some_and(|s| s.tag() == *b"Arab");
    let mut mask_array = [0; 8];
    for (m, f) in mask_array.iter_mut().zip(FEATURES) {
        *m = plan.map.one_mask(Tag(f));
        do_fallback = do_fallback && (feature_is_syriac(f) || plan.map.needs_fallback(Tag(f)));
    }
    let fallback = do_fallback.then(|| FallbackPlan::new(plan, font));
    ArabicPlan {
        mask_array,
        do_fallback,
        has_stch: plan.map.one_mask(Tag(*b"stch")) != 0,
        fallback,
    }
}

fn data_create(plan: &ShapePlan<'_>, font: &Font<'_>) -> ShaperData {
    ShaperData::Arabic(plan_data(plan, font))
}

fn context_type(c: u32) -> u8 {
    joining_type(c, sofel_unicode::general_category(c))
}

/// `arabic_joining`.
fn joining(buffer: &mut Buffer) {
    let mut prev: Option<usize> = None;
    let mut state = 0u8;

    if let Some(&c) = buffer
        .context
        .first()
        .and_then(|c| c.get(..buffer.context_len.first().copied().unwrap_or(0)))
        .and_then(|c| c.iter().find(|&&c| context_type(c) != jt::T))
    {
        state = entry(state, context_type(c)).2;
    }

    for i in 0..buffer.len {
        let info = &buffer.info[i];
        let t = joining_type(info.codepoint, info.general_category());
        if t == jt::T {
            buffer.info[i].shaper_aux = action::NONE;
            continue;
        }
        let (prev_action, curr_action, next_state) = entry(state, t);
        match prev {
            Some(p) if prev_action != action::NONE => {
                buffer.info[p].shaper_aux = prev_action;
                buffer.safe_to_insert_tatweel(p, i.wrapping_add(1));
            }
            None => {
                if t >= jt::R {
                    buffer.unsafe_to_concat_from_outbuffer(0, i.wrapping_add(1));
                }
            }
            Some(p) => {
                if t >= jt::R || (2..=5).contains(&state) {
                    buffer.unsafe_to_concat(p, i.wrapping_add(1));
                }
            }
        }
        buffer.info[i].shaper_aux = curr_action;
        prev = Some(i);
        state = next_state;
    }

    let post = buffer.context.get(1).copied().unwrap_or_default();
    let post_len = buffer.context_len.get(1).copied().unwrap_or(0);
    if let Some(&c) = post
        .get(..post_len)
        .and_then(|c| c.iter().find(|&&c| context_type(c) != jt::T))
    {
        let (prev_action, _, _) = entry(state, context_type(c));
        let len = buffer.len;
        match prev {
            Some(p) if prev_action != action::NONE => {
                buffer.info[p].shaper_aux = prev_action;
                buffer.safe_to_insert_tatweel(p, len);
            }
            Some(p) if (2..=5).contains(&state) => buffer.unsafe_to_concat(p, len),
            _ => {}
        }
    }
}

/// `mongolian_variation_selectors`: the free variation selectors take
/// the action of the character before.
fn mongolian_variation_selectors(buffer: &mut Buffer) {
    for i in 1..buffer.len {
        let u = buffer.info[i].codepoint;
        if (0x180B..=0x180D).contains(&u) || u == 0x180F {
            buffer.info[i].shaper_aux = buffer.info[i.wrapping_sub(1)].shaper_aux;
        }
    }
}

/// `setup_masks_arabic_plan`.
pub(crate) fn setup_masks_plan(ap: &ArabicPlan, buffer: &mut Buffer) {
    joining(buffer);
    if buffer.script.is_some_and(|s| s.tag() == *b"Mong") {
        mongolian_variation_selectors(buffer);
    }
    for info in buffer.infos_mut() {
        info.mask |= ap
            .mask_array
            .get(usize::from(info.shaper_aux))
            .copied()
            .unwrap_or(0);
    }
}

/// `setup_masks_arabic`.
fn setup_masks(plan: &ShapePlan<'_>, buffer: &mut Buffer, _: &ShapeFont<'_, '_>) {
    if let ShaperData::Arabic(ap) = &plan.data {
        setup_masks_plan(ap, buffer);
    }
}

/// `deallocate_buffer_var`: nothing to do with named fields.
fn deallocate_buffer_var(_: &ShapePlan<'_>, _: &ShapeFont<'_, '_>, _: &mut Buffer) -> bool {
    false
}

/// `arabic_fallback_shape`.
fn fallback_shape(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_>, buffer: &mut Buffer) -> bool {
    let ShaperData::Arabic(ap) = &plan.data else {
        return false;
    };
    if !ap.do_fallback {
        return false;
    }
    if let Some(f) = &ap.fallback {
        f.shape(plan, font, buffer);
    }
    true
}

/// `record_stch`: after `stch`, the multiplied glyphs are tiles, odd
/// components repeating.
fn record_stch(plan: &ShapePlan<'_>, _: &ShapeFont<'_, '_>, buffer: &mut Buffer) -> bool {
    let ShaperData::Arabic(ap) = &plan.data else {
        return false;
    };
    if !ap.has_stch {
        return false;
    }
    let mut found = false;
    for info in buffer.infos_mut() {
        if info.multiplied() {
            info.shaper_aux = if info.lig_comp() % 2 == 1 {
                action::STCH_REPEATING
            } else {
                action::STCH_FIXED
            };
            found = true;
        }
    }
    if found {
        buffer.scratch_flags |= HAS_STCH;
    }
    false
}

/// `HB_ARABIC_GENERAL_CATEGORY_IS_WORD`.
fn is_word_category(gc: GeneralCategory) -> bool {
    use GeneralCategory::*;
    matches!(
        gc,
        Unassigned
            | PrivateUse
            | ModifierLetter
            | OtherLetter
            | SpacingMark
            | EnclosingMark
            | NonspacingMark
            | DecimalNumber
            | LetterNumber
            | OtherNumber
            | CurrencySymbol
            | ModifierSymbol
            | MathSymbol
            | OtherSymbol
    )
}

/// `STCH_MAX_GLYPHS`.
const STCH_MAX_GLYPHS: u32 = 256;

/// `apply_stch`: the tiles of each stretch repeated to fill the rest of
/// the word, in two passes (measure, then cut).
fn apply_stch(buffer: &mut Buffer, font: &ShapeFont<'_, '_>) {
    if buffer.scratch_flags & HAS_STCH == 0 {
        return;
    }
    let rtl = buffer.direction == Direction::Rtl;
    if !rtl {
        buffer.reverse();
    }
    let sign: i64 = if font.x_scale() < 0 { -1 } else { 1 };
    let mut extra_glyphs_needed: usize = 0;
    let count = buffer.len;
    // The tiles' advances, by position, from the measuring pass.
    let mut widths = vec![0i32; count];
    let is_stch_at = |b: &Buffer, i: usize| is_stch(b.info[i].shaper_aux);

    for cut in [false, true] {
        let new_len = count.saturating_add(extra_glyphs_needed);
        let mut j = new_len;
        let mut i = count;
        let mut overflow = false;
        while i > 0 {
            if !is_stch_at(buffer, i.wrapping_sub(1)) {
                if cut {
                    j = j.wrapping_sub(1);
                    buffer.info[j] = buffer.info[i.wrapping_sub(1)];
                    buffer.pos[j] = buffer.pos[i.wrapping_sub(1)];
                }
                i = i.wrapping_sub(1);
                continue;
            }

            let mut w_total: i32 = 0;
            let mut w_fixed: i32 = 0;
            let mut w_repeating: i32 = 0;
            let mut n_fixed: u32 = 0;
            let mut n_repeating: u32 = 0;

            let end = i;
            while i > 0 && is_stch_at(buffer, i.wrapping_sub(1)) {
                i = i.wrapping_sub(1);
                if !cut && let Some(w) = widths.get_mut(i) {
                    *w = font.h_advance(buffer.info[i].codepoint);
                }
                let width = widths.get(i).copied().unwrap_or(0);
                if buffer.info[i].shaper_aux == action::STCH_FIXED {
                    w_fixed = w_fixed.saturating_add(width);
                    n_fixed = n_fixed.saturating_add(1);
                } else {
                    w_repeating = w_repeating.saturating_add(width);
                    n_repeating = n_repeating.saturating_add(1);
                }
            }
            let start = i;
            let mut context = i;
            while context > 0 && {
                let ci = &buffer.info[context.wrapping_sub(1)];
                !is_stch(ci.shaper_aux)
                    && (ci.is_default_ignorable() || is_word_category(ci.general_category()))
            } {
                context = context.wrapping_sub(1);
                w_total = w_total.saturating_add(buffer.pos[context].x_advance);
            }
            // Leave i at start; the loop continues below it.

            let mut n_copies: u32 = 0;
            let mut w_remaining_signed = i64::from(w_total).wrapping_sub(i64::from(w_fixed));
            let mut w_repeating_signed = i64::from(w_repeating);
            if sign < 0 {
                w_remaining_signed = w_remaining_signed.wrapping_neg();
                w_repeating_signed = w_repeating_signed.wrapping_neg();
            }
            let mut w_remaining = w_total.saturating_sub(w_fixed);
            if w_remaining_signed > w_repeating_signed && w_repeating_signed > 0 {
                n_copies = u32::try_from(
                    w_remaining_signed
                        .checked_div(w_repeating_signed)
                        .unwrap_or(0)
                        .wrapping_sub(1),
                )
                .unwrap_or(u32::MAX);
            }

            // An extra repeat, squeezed, may fit better.
            let mut extra_repeat_overlap: i32 = 0;
            let shortfall = w_remaining_signed
                .wrapping_sub(w_repeating_signed.wrapping_mul(i64::from(n_copies).wrapping_add(1)));
            if shortfall > 0 && n_repeating > 0 {
                n_copies = n_copies.wrapping_add(1);
                let excess = i64::from(n_copies)
                    .wrapping_add(1)
                    .wrapping_mul(w_repeating_signed)
                    .wrapping_sub(w_remaining_signed);
                if excess > 0 {
                    let d = i64::from(n_copies).wrapping_mul(i64::from(n_repeating));
                    let q = excess.checked_div(d).unwrap_or(0);
                    extra_repeat_overlap =
                        i32::try_from(q).unwrap_or(if q < 0 { i32::MIN } else { i32::MAX });
                    w_remaining = 0;
                }
            }

            let mut max_copies = 0;
            if n_repeating > 0 {
                let base_glyphs = n_fixed.saturating_add(n_repeating);
                if base_glyphs < STCH_MAX_GLYPHS {
                    max_copies = STCH_MAX_GLYPHS
                        .wrapping_sub(base_glyphs)
                        .checked_div(n_repeating)
                        .unwrap_or(0);
                }
            }
            n_copies = n_copies.min(max_copies);

            if !cut {
                let added = n_copies.checked_mul(n_repeating);
                match added.and_then(|a| extra_glyphs_needed.checked_add(usize::try_from(a).ok()?))
                {
                    Some(v) => extra_glyphs_needed = v,
                    None => {
                        overflow = true;
                        break;
                    }
                }
            } else {
                buffer.unsafe_to_break(context, end);
                let mut x_offset = w_remaining.checked_div(2).unwrap_or(0);
                let mut k = end;
                while k > start {
                    let src = k.wrapping_sub(1);
                    let width = widths.get(src).copied().unwrap_or(0);
                    let mut repeat: u32 = 1;
                    if buffer.info[src].shaper_aux == action::STCH_REPEATING {
                        repeat = repeat.saturating_add(n_copies);
                    }
                    buffer.pos[src].x_advance = 0;
                    for n in 0..repeat {
                        if rtl {
                            x_offset = x_offset.saturating_sub(width);
                            if n > 0 {
                                x_offset = x_offset.saturating_add(extra_repeat_overlap);
                            }
                        }
                        buffer.pos[src].x_offset = x_offset;
                        j = j.wrapping_sub(1);
                        buffer.info[j] = buffer.info[src];
                        buffer.pos[j] = buffer.pos[src];
                        if !rtl {
                            x_offset = x_offset.saturating_add(width);
                            if n > 0 {
                                x_offset = x_offset.saturating_sub(extra_repeat_overlap);
                            }
                        }
                    }
                    k = k.wrapping_sub(1);
                }
            }
        }

        if !cut {
            if overflow {
                break;
            }
            match count.checked_add(extra_glyphs_needed) {
                Some(total) if buffer.ensure(total) => {}
                _ => break,
            }
        } else {
            buffer.len = new_len;
        }
    }

    if !rtl {
        buffer.reverse();
    }
}

/// `postprocess_glyphs_arabic`.
fn postprocess_glyphs(_: &ShapePlan<'_>, buffer: &mut Buffer, font: &ShapeFont<'_, '_>) {
    apply_stch(buffer, font);
}

/// The modifier combining marks of UAX #53.
const MODIFIER_COMBINING_MARKS: [u32; 14] = [
    0x0654, // ARABIC HAMZA ABOVE
    0x0655, // ARABIC HAMZA BELOW
    0x0658, // ARABIC MARK NOON GHUNNA
    0x06DC, // ARABIC SMALL HIGH SEEN
    0x06E3, // ARABIC SMALL LOW SEEN
    0x06E7, // ARABIC SMALL HIGH YEH
    0x06E8, // ARABIC SMALL HIGH NOON
    0x08CA, // ARABIC SMALL HIGH FARSI YEH
    0x08CB, // ARABIC SMALL HIGH YEH BARREE WITH TWO DOTS BELOW
    0x08CD, // ARABIC SMALL HIGH ZAH
    0x08CE, // ARABIC LARGE ROUND DOT ABOVE
    0x08CF, // ARABIC LARGE ROUND DOT BELOW
    0x08D3, // ARABIC SMALL LOW WAW
    0x08F3, // ARABIC SMALL HIGH WAW
];

fn is_mcm(info: &Info) -> bool {
    MODIFIER_COMBINING_MARKS.contains(&info.codepoint)
}

/// `reorder_marks_arabic`: the modifier combining marks of classes 220
/// and 230 move to the start of the marks, renumbered 22 and 26.
fn reorder_marks(_: &ShapePlan<'_>, buffer: &mut Buffer, start: usize, end: usize) {
    let mut start = start;
    let mut i = start;
    for cc in [mcc::BELOW, mcc::ABOVE] {
        while i < end && buffer.info[i].modified_combining_class() < cc {
            i = i.wrapping_add(1);
        }
        if i == end {
            break;
        }
        if buffer.info[i].modified_combining_class() > cc {
            continue;
        }
        let mut j = i;
        while j < end && buffer.info[j].modified_combining_class() == cc && is_mcm(&buffer.info[j])
        {
            j = j.wrapping_add(1);
        }
        if i == j {
            continue;
        }
        buffer.merge_clusters(start, j);
        // Move i..j to start.
        buffer
            .info
            .range_mut(start, j)
            .rotate_right(j.wrapping_sub(i));
        let new_start = start.wrapping_add(j.wrapping_sub(i));
        let new_cc = if cc == mcc::BELOW {
            mcc::CCC22
        } else {
            mcc::CCC26
        };
        while start < new_start {
            buffer.info[start].set_modified_combining_class(new_cc);
            start = start.wrapping_add(1);
        }
        i = j;
    }
}

/// `_hb_ot_shaper_arabic`.
pub(crate) static ARABIC: ShaperFuncs = ShaperFuncs {
    collect_features: Some(collect_features),
    override_features: None,
    data_create: Some(data_create),
    preprocess_text: None,
    postprocess_glyphs: Some(postprocess_glyphs),
    decompose: None,
    compose: None,
    setup_masks: Some(setup_masks),
    reorder_marks: Some(reorder_marks),
    gpos_tag: None,
    normalization_preference: NormalizationMode::Auto,
    zero_width_marks: ZeroWidthMarks::ByGdefLate,
    fallback_position: true,
};
