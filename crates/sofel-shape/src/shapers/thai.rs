//! The Thai and Lao shaper (ADR 0024, thai): SARA AM decomposed and its
//! NIKHAHIT moved before the above-base marks, and for Thai fonts without
//! a Thai `GSUB` script the Windows and Mac private-use glyphs.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shaper-thai.cc`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_unicode::GeneralCategory;

use crate::buffer::{Buffer, scratch, uprops};
use crate::font::ShapeFont;
use crate::layout::GSUB;
use crate::plan::{NormalizationMode, ShapePlan, ShaperFuncs, ZeroWidthMarks};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Consonant {
    Nc,
    Ac,
    Rc,
    Dc,
    None,
}

/// `get_consonant_type`.
fn consonant_type(u: u32) -> Consonant {
    match u {
        0x0E1B | 0x0E1D | 0x0E1F => Consonant::Ac,
        0x0E0D | 0x0E10 => Consonant::Rc,
        0x0E0E | 0x0E0F => Consonant::Dc,
        0x0E01..=0x0E2E => Consonant::Nc,
        _ => Consonant::None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Av,
    Bv,
    T,
}

/// `get_mark_type`.
fn mark_type(u: u32) -> Option<Mark> {
    match u {
        0x0E31 | 0x0E34..=0x0E37 | 0x0E47 | 0x0E4D..=0x0E4E => Some(Mark::Av),
        0x0E38..=0x0E3A => Some(Mark::Bv),
        0x0E48..=0x0E4C => Some(Mark::T),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Nop,
    /// Shift the mark down.
    Sd,
    /// Shift the mark left.
    Sl,
    /// Shift the mark down and left.
    Sdl,
    /// Remove the descender from the base.
    Rd,
}

/// `(character, Windows PUA, Mac PUA)` for each action.
const SD: [(u32, u32, u32); 8] = [
    (0x0E48, 0xF70A, 0xF88B), // MAI EK
    (0x0E49, 0xF70B, 0xF88E), // MAI THO
    (0x0E4A, 0xF70C, 0xF891), // MAI TRI
    (0x0E4B, 0xF70D, 0xF894), // MAI CHATTAWA
    (0x0E4C, 0xF70E, 0xF897), // THANTHAKHAT
    (0x0E38, 0xF718, 0xF89B), // SARA U
    (0x0E39, 0xF719, 0xF89C), // SARA UU
    (0x0E3A, 0xF71A, 0xF89D), // PHINTHU
];
const SDL: [(u32, u32, u32); 5] = [
    (0x0E48, 0xF705, 0xF88C), // MAI EK
    (0x0E49, 0xF706, 0xF88F), // MAI THO
    (0x0E4A, 0xF707, 0xF892), // MAI TRI
    (0x0E4B, 0xF708, 0xF895), // MAI CHATTAWA
    (0x0E4C, 0xF709, 0xF898), // THANTHAKHAT
];
const SL: [(u32, u32, u32); 12] = [
    (0x0E48, 0xF713, 0xF88A), // MAI EK
    (0x0E49, 0xF714, 0xF88D), // MAI THO
    (0x0E4A, 0xF715, 0xF890), // MAI TRI
    (0x0E4B, 0xF716, 0xF893), // MAI CHATTAWA
    (0x0E4C, 0xF717, 0xF896), // THANTHAKHAT
    (0x0E31, 0xF710, 0xF884), // MAI HAN-AKAT
    (0x0E34, 0xF701, 0xF885), // SARA I
    (0x0E35, 0xF702, 0xF886), // SARA II
    (0x0E36, 0xF703, 0xF887), // SARA UE
    (0x0E37, 0xF704, 0xF888), // SARA UEE
    (0x0E47, 0xF712, 0xF889), // MAITAIKHU
    (0x0E4D, 0xF711, 0xF899), // NIKHAHIT
];
const RD: [(u32, u32, u32); 2] = [
    (0x0E0D, 0xF70F, 0xF89A), // YO YING
    (0x0E10, 0xF700, 0xF89E), // THO THAN
];

/// `thai_pua_shape`.
fn pua_shape(u: u32, action: Action, font: &ShapeFont<'_, '_, '_, '_>) -> u32 {
    let mappings: &[(u32, u32, u32)] = match action {
        Action::Nop => return u,
        Action::Sd => &SD,
        Action::Sdl => &SDL,
        Action::Sl => &SL,
        Action::Rd => &RD,
    };
    mappings
        .iter()
        .find(|m| m.0 == u)
        .and_then(|&(_, win, mac)| {
            [win, mac]
                .into_iter()
                .find(|&pua| font.nominal_glyph(pua).is_some())
        })
        .unwrap_or(u)
}

#[derive(Clone, Copy)]
enum Above {
    T0,
    T1,
    T2,
    T3,
}

#[derive(Clone, Copy)]
enum Below {
    B0,
    B1,
    B2,
}

/// `thai_above_start_state`.
fn above_start(c: Consonant) -> Above {
    match c {
        Consonant::Ac => Above::T1,
        Consonant::None => Above::T3,
        Consonant::Nc | Consonant::Rc | Consonant::Dc => Above::T0,
    }
}

/// `thai_below_start_state`.
fn below_start(c: Consonant) -> Below {
    match c {
        Consonant::Nc | Consonant::Ac => Below::B0,
        Consonant::Rc => Below::B1,
        Consonant::Dc | Consonant::None => Below::B2,
    }
}

/// `thai_above_state_machine`.
fn above_edge(s: Above, m: Mark) -> (Action, Above) {
    use Above::*;
    match (s, m) {
        (T0, Mark::Av) => (Action::Nop, T3),
        (T0, Mark::Bv) => (Action::Nop, T0),
        (T0, Mark::T) => (Action::Sd, T3),
        (T1, Mark::Av) => (Action::Sl, T2),
        (T1, Mark::Bv) => (Action::Nop, T1),
        (T1, Mark::T) => (Action::Sdl, T2),
        (T2, Mark::Av) => (Action::Nop, T3),
        (T2, Mark::Bv) => (Action::Nop, T2),
        (T2, Mark::T) => (Action::Sl, T3),
        (T3, _) => (Action::Nop, T3),
    }
}

/// `thai_below_state_machine`.
fn below_edge(s: Below, m: Mark) -> (Action, Below) {
    use Below::*;
    match (s, m) {
        (B0, Mark::Bv) => (Action::Nop, B2),
        (B0, _) => (Action::Nop, B0),
        (B1, Mark::Bv) => (Action::Rd, B2),
        (B1, _) => (Action::Nop, B1),
        (B2, Mark::Bv) => (Action::Sd, B2),
        (B2, _) => (Action::Nop, B2),
    }
}

/// `do_thai_pua_shaping`.
fn pua_shaping(buffer: &mut Buffer, font: &ShapeFont<'_, '_, '_, '_>) {
    let mut above = above_start(Consonant::None);
    let mut below = below_start(Consonant::None);
    let mut base = 0usize;
    for i in 0..buffer.len {
        let u = buffer.info[i].codepoint;
        let Some(mt) = mark_type(u) else {
            let ct = consonant_type(u);
            above = above_start(ct);
            below = below_start(ct);
            base = i;
            continue;
        };
        let (aa, an) = above_edge(above, mt);
        let (ba, bn) = below_edge(below, mt);
        above = an;
        below = bn;
        // At least one of the two actions is NOP.
        let action = if aa != Action::Nop { aa } else { ba };
        buffer.unsafe_to_break(base, i);
        if action == Action::Rd {
            buffer.info[base].codepoint = pua_shape(buffer.info[base].codepoint, action, font);
        } else {
            buffer.info[i].codepoint = pua_shape(u, action, font);
        }
    }
}

fn is_sara_am(u: u32) -> bool {
    u & !0x0080 == 0x0E33
}

fn is_above_base_mark(u: u32) -> bool {
    matches!(u & !0x0080, 0x0E34..=0x0E37 | 0x0E47..=0x0E4E | 0x0E31 | 0x0E3B)
}

/// `preprocess_text_thai`.
fn preprocess_text(plan: &ShapePlan<'_>, buffer: &mut Buffer, font: &ShapeFont<'_, '_, '_, '_>) {
    buffer.clear_output();
    buffer.idx = 0;
    let count = buffer.len;
    while buffer.idx < count {
        let u = buffer.cur(0).codepoint;
        if !is_sara_am(u) {
            if !buffer.next_glyph() {
                break;
            }
            continue;
        }
        // SARA AM: NIKHAHIT then SARA AA, NIKHAHIT moved before the
        // above-base marks.
        buffer.output_glyph(u.wrapping_sub(0x0E33).wrapping_add(0x0E4D));
        buffer.scratch_flags |= scratch::HAS_CONTINUATIONS;
        buffer.prev_mut().unicode_props |= uprops::CONTINUATION;
        if !buffer.replace_glyph(u.wrapping_sub(1)) {
            break;
        }
        let end = buffer.out_len;
        // NIKHAHIT is a mark of class 0 when widths are zeroed.
        buffer.out_info_mut()[end.wrapping_sub(2)]
            .set_general_category(GeneralCategory::NonspacingMark);
        let mut start = end.wrapping_sub(2);
        while start > 0 && is_above_base_mark(buffer.out_info()[start.wrapping_sub(1)].codepoint) {
            start = start.wrapping_sub(1);
        }
        if start.wrapping_add(2) < end {
            buffer.merge_out_clusters(start, end);
            buffer
                .out_info_mut()
                .range_mut(start, end.wrapping_sub(1))
                .rotate_right(1);
        }
        // Decomposed into a mark: merge with the previous cluster.
        if start > 0 {
            buffer.merge_out_grapheme_clusters(start.wrapping_sub(1), end);
        }
    }
    buffer.sync();

    // Fonts with a Thai GSUB script are done.
    if plan.props.script.is_some_and(|s| s.tag() == *b"Thai") && !plan.map.found_script[GSUB] {
        pua_shaping(buffer, font);
    }
}

/// `_hb_ot_shaper_thai`.
pub(crate) static THAI: ShaperFuncs = ShaperFuncs {
    collect_features: None,
    override_features: None,
    data_create: None,
    preprocess_text: Some(preprocess_text),
    postprocess_glyphs: None,
    decompose: None,
    compose: None,
    setup_masks: None,
    reorder_marks: None,
    gpos_tag: None,
    normalization_preference: NormalizationMode::Auto,
    zero_width_marks: ZeroWidthMarks::ByGdefLate,
    fallback_position: false,
};
