//! Normalization before glyph mapping (ADR 0020, シェーピングの手順):
//! decomposition with the font's glyphs in mind, reordering by combining
//! class, and recomposition (`_hb_ot_shape_normalize`).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shape-normalize.cc`, and
//! HarfRust 0.15.0 `src/normalize.rs`, MIT; see THIRD_PARTY_NOTICES.md.

use crate::buffer::{Buffer, Info, scratch, uprops};
use crate::font::ShapeFont;
use crate::plan::{NormalizationMode, ShapePlan};
use crate::unicode;

/// `HB_OT_SHAPE_MAX_COMBINING_MARKS`.
const MAX_COMBINING_MARKS: usize = 32;

/// `hb_ot_shape_normalize_context_t`: what a shaper's `decompose` and
/// `compose` see.
pub(crate) struct NormalizeContext<'p, 'f, 'a, 'l, 's> {
    pub(crate) plan: &'p ShapePlan<'a>,
    pub(crate) font: &'f ShapeFont<'f, 'a, 'l, 's>,
}

/// `decompose_unicode`.
fn decompose_unicode(ab: u32) -> Option<(u32, u32)> {
    sofel_unicode::decompose(ab)
}

/// `compose_unicode`.
fn compose_unicode(a: u32, b: u32) -> Option<u32> {
    sofel_unicode::compose(a, b)
}

impl NormalizeContext<'_, '_, '_, '_, '_> {
    fn decompose(&self, ab: u32) -> Option<(u32, u32)> {
        match self.plan.funcs().decompose {
            Some(f) => f(self, ab),
            None => decompose_unicode(ab),
        }
    }
    fn compose(&self, a: u32, b: u32) -> Option<u32> {
        if a == 0 || b == 0 {
            return None;
        }
        match self.plan.funcs().compose {
            Some(f) => f(self, a, b),
            None => compose_unicode(a, b),
        }
    }
}

/// `_hb_glyph_info_set_unicode_props`.
pub(crate) fn set_unicode_props(info: &mut Info, scratch_flags: &mut u32) {
    let u = info.codepoint;
    let gc = sofel_unicode::general_category(u);
    let mut props = u16::from(gc as u8);
    if u >= 0x80 {
        if unicode::is_default_ignorable(u) {
            *scratch_flags |= scratch::HAS_DEFAULT_IGNORABLES;
            props |= uprops::IGNORABLE;
            if u == 0x200C {
                props |= uprops::CF_ZWNJ;
            } else if u == 0x200D {
                props |= uprops::CF_ZWJ;
            } else if matches!(u, 0x180B..=0x180D | 0x180F | 0xE0020..=0xE007F) {
                props |= uprops::HIDDEN;
            } else if u == 0x034F {
                *scratch_flags |= scratch::HAS_CGJ;
                props |= uprops::HIDDEN;
            }
        }
        if unicode::is_mark_category(gc) {
            *scratch_flags |= scratch::HAS_CONTINUATIONS;
            props |= uprops::CONTINUATION;
            props |= u16::from(unicode::modified_combining_class(u)) << 8;
        }
    }
    info.unicode_props = props;
}

/// `output_char`.
fn output_char(buffer: &mut Buffer, unichar: u32, glyph: u32) {
    buffer.cur_mut(0).glyph_index = glyph;
    let _ = buffer.output_glyph(unichar);
    let mut flags = buffer.scratch_flags;
    set_unicode_props(buffer.prev_mut(), &mut flags);
    buffer.scratch_flags = flags;
}

/// `next_char`.
fn next_char(buffer: &mut Buffer, glyph: u32) {
    buffer.cur_mut(0).glyph_index = glyph;
    let _ = buffer.next_glyph();
}

/// `decompose`: the number of characters output.
fn decompose(
    c: &NormalizeContext<'_, '_, '_, '_, '_>,
    buffer: &mut Buffer,
    shortest: bool,
    ab: u32,
) -> usize {
    let Some((a, b)) = c.decompose(ab) else {
        return 0;
    };
    let b_glyph = if b != 0 {
        match c.font.nominal_glyph(b) {
            Some(g) => g,
            None => return 0,
        }
    } else {
        0
    };
    let a_glyph = c.font.nominal_glyph(a);
    if shortest && let Some(ag) = a_glyph {
        output_char(buffer, a, ag);
        if b != 0 {
            output_char(buffer, b, b_glyph);
            return 2;
        }
        return 1;
    }
    let ret = decompose(c, buffer, shortest, a);
    if ret != 0 {
        if b != 0 {
            output_char(buffer, b, b_glyph);
            return ret.wrapping_add(1);
        }
        return ret;
    }
    if let Some(ag) = a_glyph {
        output_char(buffer, a, ag);
        if b != 0 {
            output_char(buffer, b, b_glyph);
            return 2;
        }
        return 1;
    }
    0
}

/// `decompose_current_character`.
fn decompose_current_character(
    c: &NormalizeContext<'_, '_, '_, '_, '_>,
    buffer: &mut Buffer,
    shortest: bool,
) {
    let u = buffer.cur(0).codepoint;
    let not_found = buffer.not_found;
    let mut glyph = 0u32;
    if shortest {
        match c.font.nominal_glyph(u) {
            Some(g) => {
                next_char(buffer, g);
                return;
            }
            None => glyph = not_found,
        }
    }
    if decompose(c, buffer, shortest, u) != 0 {
        buffer.skip_glyph();
        return;
    }
    if !shortest {
        match c.font.nominal_glyph(u) {
            Some(g) => {
                next_char(buffer, g);
                return;
            }
            None => glyph = not_found,
        }
    }
    if buffer.cur(0).is_unicode_space() {
        let space_type = unicode::space_fallback_type(u);
        if space_type != unicode::SpaceFallback::NotSpace {
            let space_glyph = c.font.nominal_glyph(0x20).unwrap_or(buffer.invisible);
            if space_glyph != 0 {
                buffer.cur_mut(0).set_space_fallback(space_type);
                next_char(buffer, space_glyph);
                buffer.scratch_flags |= scratch::HAS_SPACE_FALLBACK;
                return;
            }
        }
    }
    if u == 0x2011
        && let Some(g) = c.font.nominal_glyph(0x2010)
    {
        next_char(buffer, g);
        return;
    }
    next_char(buffer, glyph);
}

/// `set_glyph`.
fn set_glyph(buffer: &mut Buffer, font: &ShapeFont<'_, '_, '_, '_>) {
    let u = buffer.cur(0).codepoint;
    buffer.cur_mut(0).glyph_index = font.nominal_glyph(u).unwrap_or(0);
}

/// `handle_variation_selector_cluster`.
fn handle_variation_selector_cluster(
    c: &NormalizeContext<'_, '_, '_, '_, '_>,
    buffer: &mut Buffer,
    end: usize,
) {
    let font = c.font;
    while buffer.idx < end.wrapping_sub(1) && buffer.successful {
        if unicode::is_variation_selector(buffer.cur(1).codepoint) {
            let (u, vs) = (buffer.cur(0).codepoint, buffer.cur(1).codepoint);
            if let Some(g) = font.variation_glyph(u, vs) {
                buffer.cur_mut(0).glyph_index = g;
                let _ = buffer.replace_glyphs(2, 1, &[u]);
            } else {
                set_glyph(buffer, font);
                let _ = buffer.next_glyph();
                buffer.scratch_flags |= scratch::HAS_VARIATION_SELECTOR_FALLBACK;
                buffer.cur_mut(0).set_variation_selector(true);
                if buffer.not_found_variation_selector.is_some() {
                    buffer.cur_mut(0).clear_default_ignorable();
                }
                set_glyph(buffer, font);
                let _ = buffer.next_glyph();
            }
            while buffer.idx < end
                && buffer.successful
                && unicode::is_variation_selector(buffer.cur(0).codepoint)
            {
                set_glyph(buffer, font);
                let _ = buffer.next_glyph();
            }
        } else {
            set_glyph(buffer, font);
            let _ = buffer.next_glyph();
        }
    }
    if buffer.idx < end {
        set_glyph(buffer, font);
        let _ = buffer.next_glyph();
    }
}

/// `decompose_multi_char_cluster`.
fn decompose_multi_char_cluster(
    c: &NormalizeContext<'_, '_, '_, '_, '_>,
    buffer: &mut Buffer,
    end: usize,
    short_circuit: bool,
) {
    let has_vs =
        (buffer.idx..end).any(|i| unicode::is_variation_selector(buffer.info[i].codepoint));
    if has_vs {
        handle_variation_selector_cluster(c, buffer, end);
        return;
    }
    while buffer.idx < end && buffer.successful {
        decompose_current_character(c, buffer, short_circuit);
    }
}

/// `_hb_ot_shape_normalize`.
pub(crate) fn normalize(
    plan: &ShapePlan<'_>,
    buffer: &mut Buffer,
    font: &ShapeFont<'_, '_, '_, '_>,
) {
    if buffer.len == 0 {
        return;
    }
    let funcs = plan.funcs();
    let mut mode = funcs.normalization_preference;
    if mode == NormalizationMode::Auto {
        mode = NormalizationMode::ComposedDiacritics;
    }
    let c = NormalizeContext { plan, font };
    let always_short_circuit = mode == NormalizationMode::None;
    let might_short_circuit = always_short_circuit
        || (mode != NormalizationMode::Decomposed
            && mode != NormalizationMode::ComposedDiacriticsNoShortCircuit);

    // First round, decompose.
    let mut all_simple = true;
    buffer.clear_output();
    let count = buffer.len;
    buffer.idx = 0;
    loop {
        let mut end = buffer.idx.wrapping_add(1);
        while end < count && !buffer.info[end].is_unicode_mark() {
            end = end.wrapping_add(1);
        }
        if end < count {
            end = end.wrapping_sub(1);
        }
        if might_short_circuit {
            // `get_nominal_glyphs`: as many as map from the start.
            let mut done = 0usize;
            let mut i = buffer.idx;
            while i < end {
                match font.nominal_glyph(buffer.info[i].codepoint) {
                    Some(g) => buffer.info[i].glyph_index = g,
                    None => break,
                }
                done = done.wrapping_add(1);
                i = i.wrapping_add(1);
            }
            if !buffer.next_glyphs(done) {
                break;
            }
        }
        while buffer.idx < end && buffer.successful {
            decompose_current_character(&c, buffer, might_short_circuit);
        }
        if buffer.idx == count || !buffer.successful {
            break;
        }
        all_simple = false;
        let mut end = buffer.idx.wrapping_add(1);
        while end < count && buffer.info[end].is_unicode_mark() {
            end = end.wrapping_add(1);
        }
        decompose_multi_char_cluster(&c, buffer, end, always_short_circuit);
        if !(buffer.idx < count && buffer.successful) {
            break;
        }
    }
    buffer.sync();

    // Second round, reorder.
    if !all_simple {
        let count = buffer.len;
        let mut i = 0usize;
        while i < count {
            if buffer.info[i].modified_combining_class() == 0 {
                i = i.wrapping_add(1);
                continue;
            }
            let mut end = i.wrapping_add(1);
            while end < count && buffer.info[end].modified_combining_class() != 0 {
                end = end.wrapping_add(1);
            }
            if end.wrapping_sub(i) <= MAX_COMBINING_MARKS {
                buffer.sort(i, end, |a, b| {
                    let (x, y) = (a.modified_combining_class(), b.modified_combining_class());
                    i32::from(x > y).wrapping_sub(i32::from(x < y))
                });
                if let Some(f) = funcs.reorder_marks {
                    f(plan, buffer, i, end);
                }
            }
            i = end.wrapping_add(1);
        }
    }

    if buffer.scratch_flags & scratch::HAS_CGJ != 0 {
        let count = buffer.len;
        let mut i = 1usize;
        while i.wrapping_add(1) < count {
            let next_cc = buffer.info[i.wrapping_add(1)].modified_combining_class();
            if buffer.info[i].codepoint == 0x034F
                && (next_cc == 0
                    || buffer.info[i.wrapping_sub(1)].modified_combining_class() <= next_cc)
            {
                buffer.info[i].unhide();
            }
            i = i.wrapping_add(1);
        }
    }

    // Third round, recompose.
    if !all_simple
        && buffer.successful
        && matches!(
            mode,
            NormalizationMode::ComposedDiacritics
                | NormalizationMode::ComposedDiacriticsNoShortCircuit
        )
    {
        buffer.clear_output();
        let count = buffer.len;
        let mut starter = 0usize;
        let _ = buffer.next_glyph();
        while buffer.idx < count {
            let cur = *buffer.cur(0);
            if cur.is_unicode_mark() {
                let prev_cc = buffer.prev().modified_combining_class();
                let starter_cp = buffer.out_info()[starter].codepoint;
                if (starter == buffer.out_len.wrapping_sub(1)
                    || prev_cc < cur.modified_combining_class())
                    && let Some(composed) = c.compose(starter_cp, cur.codepoint)
                    && let Some(glyph) = font.nominal_glyph(composed)
                {
                    if !buffer.next_glyph() {
                        break;
                    }
                    let out_len = buffer.out_len;
                    buffer.merge_out_clusters(starter, out_len);
                    buffer.out_len = buffer.out_len.wrapping_sub(1);
                    let mut flags = buffer.scratch_flags;
                    let s = &mut buffer.out_info_mut()[starter];
                    s.codepoint = composed;
                    s.glyph_index = glyph;
                    set_unicode_props(s, &mut flags);
                    buffer.scratch_flags = flags;
                    continue;
                }
            }
            if !buffer.next_glyph() {
                break;
            }
            if buffer.prev().modified_combining_class() == 0 {
                starter = buffer.out_len.wrapping_sub(1);
            }
        }
        buffer.sync();
    }
}
