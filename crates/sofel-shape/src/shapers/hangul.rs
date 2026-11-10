//! The Hangul shaper (ADR 0024, hangul): jamo sequences composed into
//! syllables or decomposed into jamo with the `ljmo`, `vjmo` and `tjmo`
//! features, and tone marks moved before their syllable.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-hangul.cc` and HarfRust
//! `src/ot/shaper/hangul.rs`, MIT; see THIRD_PARTY_NOTICES.md.

use sofel_font::read::Tag;

use crate::buffer::{Buffer, BufferFlags, Mask};
use crate::font::ShapeFont;
use crate::layout::map::flags;
use crate::plan::{NormalizationMode, Planner, ShapePlan, ShaperData, ShaperFuncs, ZeroWidthMarks};

/// The jamo features, in the order of `shaper_aux`'s values 1 to 3.
const FEATURES: [[u8; 4]; 3] = [*b"ljmo", *b"vjmo", *b"tjmo"];
const LJMO: u8 = 1;
const VJMO: u8 = 2;
const TJMO: u8 = 3;

/// `hangul_shape_plan_t`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HangulPlan {
    /// The mask of no feature, `ljmo`, `vjmo`, `tjmo`.
    mask_array: [Mask; 4],
    calt_mask: Mask,
}

/// `collect_features_hangul`.
fn collect_features(planner: &mut Planner<'_, '_>) {
    for tag in FEATURES {
        planner.map.add_feature(Tag(tag), flags::NONE, 1);
    }
}

/// `override_features_hangul`: a mask for `calt`, cleared on jamo in
/// [`setup_masks`] (Uniscribe does not apply `calt` to Hangul).
fn override_features(planner: &mut Planner<'_, '_>) {
    planner.map.add_feature(Tag(*b"calt"), flags::NONE, 1);
}

/// `data_create_hangul`.
fn data_create(plan: &ShapePlan<'_>, _: &sofel_font::Font<'_>) -> ShaperData {
    let m = |t: [u8; 4]| plan.map.one_mask(Tag(t));
    ShaperData::Hangul(HangulPlan {
        mask_array: [0, m(FEATURES[0]), m(FEATURES[1]), m(FEATURES[2])],
        calt_mask: m(*b"calt"),
    })
}

const L_BASE: u32 = 0x1100;
const V_BASE: u32 = 0x1161;
const T_BASE: u32 = 0x11A7;
const L_COUNT: u32 = 19;
const V_COUNT: u32 = 21;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = V_COUNT * T_COUNT;
const S_COUNT: u32 = L_COUNT * N_COUNT;
const S_BASE: u32 = 0xAC00;

fn is_combining_l(u: u32) -> bool {
    (L_BASE..L_BASE + L_COUNT).contains(&u)
}
fn is_combining_v(u: u32) -> bool {
    (V_BASE..V_BASE + V_COUNT).contains(&u)
}
fn is_combining_t(u: u32) -> bool {
    (T_BASE + 1..T_BASE + T_COUNT).contains(&u)
}
fn is_combined_s(u: u32) -> bool {
    (S_BASE..S_BASE + S_COUNT).contains(&u)
}
fn is_l(u: u32) -> bool {
    (0x1100..=0x115F).contains(&u) || (0xA960..=0xA97C).contains(&u)
}
fn is_v(u: u32) -> bool {
    (0x1160..=0x11A7).contains(&u) || (0xD7B0..=0xD7C6).contains(&u)
}
fn is_t(u: u32) -> bool {
    (0x11A8..=0x11FF).contains(&u) || (0xD7CB..=0xD7FB).contains(&u)
}
fn is_hangul_tone(u: u32) -> bool {
    (0x302E..=0x302F).contains(&u)
}

/// `is_zero_width_char`.
fn is_zero_width_char(font: &ShapeFont<'_, '_>, u: u32) -> bool {
    font.nominal_glyph(u)
        .is_some_and(|g| font.h_advance(g) == 0)
}

/// `preprocess_text_hangul`.
fn preprocess_text(_: &ShapePlan<'_>, buffer: &mut Buffer, font: &ShapeFont<'_, '_>) {
    buffer.clear_output();
    // The most recent syllable, valid only if start < end.
    let mut start = 0usize;
    let mut end = 0usize;
    buffer.idx = 0;
    let count = buffer.len;
    while buffer.idx < count && buffer.successful {
        let u = buffer.cur(0).codepoint;

        if is_hangul_tone(u) {
            if start < end && end == buffer.out_len {
                // A tone mark after a valid syllable: move it before,
                // unless it is zero width.
                buffer.unsafe_to_break_from_outbuffer(start, buffer.idx);
                buffer.next_glyph();
                if !is_zero_width_char(font, u) {
                    buffer.merge_out_clusters(start, end.wrapping_add(1));
                    buffer
                        .out_info_mut()
                        .range_mut(start, end.wrapping_add(1))
                        .rotate_right(1);
                }
            } else if !buffer
                .flags
                .contains(BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE)
                && font.has_glyph(0x25CC)
            {
                let chars = if is_zero_width_char(font, u) {
                    [0x25CC, u]
                } else {
                    [u, 0x25CC]
                };
                buffer.replace_glyphs(1, 2, &chars);
            } else {
                buffer.next_glyph();
            }
            start = buffer.out_len;
            end = buffer.out_len;
            continue;
        }

        start = buffer.out_len;

        if is_l(u) && buffer.idx.wrapping_add(1) < count {
            let l = u;
            let v = buffer.cur(1).codepoint;
            if is_v(v) {
                // <L,V> or <L,V,T>.
                let mut t = 0;
                let mut tindex = 0;
                if buffer.idx.wrapping_add(2) < count {
                    t = buffer.cur(2).codepoint;
                    if is_t(t) {
                        tindex = t.wrapping_sub(T_BASE);
                    } else {
                        t = 0;
                    }
                }
                let n = if t != 0 { 3 } else { 2 };
                buffer.unsafe_to_break(buffer.idx, buffer.idx.wrapping_add(n));
                if is_combining_l(l) && is_combining_v(v) && (t == 0 || is_combining_t(t)) {
                    let s = S_BASE
                        .wrapping_add(l.wrapping_sub(L_BASE).wrapping_mul(N_COUNT))
                        .wrapping_add(v.wrapping_sub(V_BASE).wrapping_mul(T_COUNT))
                        .wrapping_add(tindex);
                    if font.has_glyph(s) {
                        buffer.replace_glyphs(n, 1, &[s]);
                        end = start.wrapping_add(1);
                        continue;
                    }
                }
                // Not composed: the jamo features, and past them.
                buffer.cur_mut(0).shaper_aux = LJMO;
                buffer.next_glyph();
                buffer.cur_mut(0).shaper_aux = VJMO;
                buffer.next_glyph();
                if t != 0 {
                    buffer.cur_mut(0).shaper_aux = TJMO;
                    buffer.next_glyph();
                }
                end = start.wrapping_add(n);
                buffer.merge_out_grapheme_clusters(start, end);
                continue;
            }
        } else if is_combined_s(u) {
            // <LV>, <LVT> or <LV,T>.
            let s = u;
            let has_glyph = font.has_glyph(s);
            let si = s.wrapping_sub(S_BASE);
            let lindex = si / N_COUNT;
            let nindex = si % N_COUNT;
            let vindex = nindex / T_COUNT;
            let tindex = nindex % T_COUNT;

            let next_is = |b: &Buffer, f: fn(u32) -> bool| {
                b.idx.wrapping_add(1) < count && f(b.cur(1).codepoint)
            };
            if tindex == 0 && next_is(buffer, is_combining_t) {
                // <LV,T>: try to compose.
                let new_s = s.wrapping_add(buffer.cur(1).codepoint.wrapping_sub(T_BASE));
                if font.has_glyph(new_s) {
                    buffer.replace_glyphs(2, 1, &[new_s]);
                    end = start.wrapping_add(1);
                    continue;
                }
                buffer.unsafe_to_break(buffer.idx, buffer.idx.wrapping_add(2));
            }

            // Otherwise decompose if the font lacks <LV> or <LVT>, or
            // before a non-combining T.
            if !has_glyph || (tindex == 0 && next_is(buffer, is_t)) {
                let decomposed = [
                    L_BASE.wrapping_add(lindex),
                    V_BASE.wrapping_add(vindex),
                    T_BASE.wrapping_add(tindex),
                ];
                if font.has_glyph(decomposed[0])
                    && font.has_glyph(decomposed[1])
                    && (tindex == 0 || font.has_glyph(decomposed[2]))
                {
                    let mut s_len = if tindex != 0 { 3 } else { 2 };
                    buffer.replace_glyphs(1, s_len, &decomposed);
                    // An LV decomposed because of a non-combining T
                    // takes the T into the syllable.
                    if has_glyph && tindex == 0 {
                        buffer.next_glyph();
                        s_len = s_len.wrapping_add(1);
                    }
                    if !buffer.successful {
                        break;
                    }
                    end = start.wrapping_add(s_len);
                    let jamo = buffer.out_info_mut().range_mut(start, end);
                    let mut it = jamo.iter_mut();
                    for f in [LJMO, VJMO] {
                        if let Some(i) = it.next() {
                            i.shaper_aux = f;
                        }
                    }
                    if let Some(i) = it.next() {
                        i.shaper_aux = TJMO;
                    }
                    buffer.merge_out_grapheme_clusters(start, end);
                    continue;
                } else if tindex == 0 && next_is(buffer, is_t) {
                    buffer.unsafe_to_break(buffer.idx, buffer.idx.wrapping_add(2));
                }
            }

            if has_glyph {
                // Not decomposed: past it.
                end = start.wrapping_add(1);
            }
        }

        // No syllable recognized (end <= start keeps tone marks in place).
        buffer.next_glyph();
    }
    buffer.sync();
}

/// `setup_masks_hangul`.
fn setup_masks(plan: &ShapePlan<'_>, buffer: &mut Buffer, _: &ShapeFont<'_, '_>) {
    let ShaperData::Hangul(hp) = &plan.data else {
        return;
    };
    for info in buffer.infos_mut() {
        info.mask |= hp
            .mask_array
            .get(usize::from(info.shaper_aux))
            .copied()
            .unwrap_or(0);
        let u = info.codepoint;
        if is_l(u) || is_v(u) || is_t(u) {
            info.mask &= !hp.calt_mask;
        }
    }
}

/// `_hb_ot_shaper_hangul`.
pub(crate) static HANGUL: ShaperFuncs = ShaperFuncs {
    collect_features: Some(collect_features),
    override_features: Some(override_features),
    data_create: Some(data_create),
    preprocess_text: Some(preprocess_text),
    postprocess_glyphs: None,
    decompose: None,
    compose: None,
    setup_masks: Some(setup_masks),
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::None,
    zero_width_marks: ZeroWidthMarks::None,
    fallback_position: true,
};
