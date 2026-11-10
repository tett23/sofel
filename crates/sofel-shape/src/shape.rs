//! Shaping (ADR 0020, シェーピングの手順): the steps of HarfBuzz's
//! `hb_ot_shape_internal` in order, and the `fallback` shaper.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shape.cc` and
//! `src/hb-fallback-shape.cc`, and HarfRust 0.15.0 `src/shape.rs`, MIT;
//! see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::GlyphId;
use sofel_font::{Font, GlyphExtents};
use sofel_unicode::GeneralCategory;

use crate::buffer::{
    Buffer, BufferFlags, ClusterLevel, GlyphBuffer, Info, Mask, UnicodeBuffer, gprops, scratch,
};
use crate::direction::Direction;
use crate::fallback;
use crate::font::ShapeFont;
use crate::glyph::{GlyphFlags, GlyphInfo, GlyphPosition};
use crate::layout::apply::apply_map;
use crate::layout::gpos;
use crate::layout::{GPOS, GSUB};
use crate::normalize::{normalize, set_unicode_props};
use crate::plan::{ShapePlan, ZeroWidthMarks};
use crate::props::{ResolvedProperties, script_horizontal_direction};
use crate::scale::Scale;
use crate::unicode;

/// Shapes `buffer` with `font` at `scale` as `plan` says
/// (`hb_shape_plan_execute` with the `ot` shaper), taking glyph boxes for
/// fallback mark positioning and vertical origins from the font's
/// `glyf` table ([`sofel_font::var::Instance::glyf_extents`]).
///
/// The buffer's direction and script are the plan's.
pub fn shape(
    font: &Font<'_>,
    scale: &Scale,
    plan: &ShapePlan<'_>,
    buffer: UnicodeBuffer,
) -> GlyphBuffer {
    shape_with_extents(font, scale, plan, buffer, &|g| {
        font.default_instance().glyf_extents(g)
    })
}

/// [`shape`] with glyph boxes in font units from `extents`, for fonts
/// whose boxes do not come from `glyf` (CFF outlines, color glyphs). The
/// boxes are scaled with `HbScale::scale_glyph_extents`.
pub fn shape_with_extents(
    font: &Font<'_>,
    scale: &Scale,
    plan: &ShapePlan<'_>,
    buffer: UnicodeBuffer,
    extents: &dyn Fn(GlyphId) -> Option<GlyphExtents>,
) -> GlyphBuffer {
    let mut buf = buffer.buf;
    buf.direction = plan.props.direction;
    buf.script = plan.props.script;
    let sf = ShapeFont::new(font, *scale, extents);
    buf.enter();
    if buf.len > 0 {
        shape_internal(plan, &sf, &mut buf);
    }
    buf.leave();
    into_glyph_buffer(buf)
}

/// Moves the working buffer's glyphs out into a [`GlyphBuffer`].
fn into_glyph_buffer(mut buf: Buffer) -> GlyphBuffer {
    let len = buf.len;
    let infos: Vec<GlyphInfo> = buf
        .infos()
        .iter()
        .map(|i| GlyphInfo {
            glyph: i.codepoint,
            cluster: i.cluster,
            flags: GlyphFlags(i.mask & GlyphFlags::DEFINED.0),
        })
        .collect();
    let positions: Vec<GlyphPosition> = if buf.have_positions {
        buf.pos
            .range(0, len)
            .iter()
            .map(|p| GlyphPosition {
                x_advance: p.x_advance,
                y_advance: p.y_advance,
                x_offset: p.x_offset,
                y_offset: p.y_offset,
            })
            .collect()
    } else {
        alloc::vec![GlyphPosition::default(); len]
    };
    let direction = buf.direction;
    buf.clear();
    GlyphBuffer {
        infos,
        positions,
        direction,
        spare: buf,
    }
}

/// `hb_ot_shape_internal`.
fn shape_internal(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_>, buffer: &mut Buffer) {
    let target_direction = buffer.direction;
    set_unicode_props_all(buffer, plan.map.global_mask);
    insert_dotted_circle(buffer, font);
    form_clusters(buffer);
    ensure_native_direction(buffer);
    let funcs = plan.funcs();
    if let Some(f) = funcs.preprocess_text {
        f(plan, buffer, font);
    }
    substitute_pre(plan, font, buffer, target_direction);
    position(plan, font, buffer);
    substitute_post(plan, font, buffer);
    propagate_flags(buffer);
    buffer.direction = target_direction;
}

/// `_hb_glyph_info_set_continuation`.
fn set_continuation(info: &mut Info, flags: &mut u32) {
    *flags |= scratch::HAS_CONTINUATIONS;
    info.unicode_props |= crate::buffer::uprops::CONTINUATION;
}

/// `hb_set_unicode_props`.
fn set_unicode_props_all(buffer: &mut Buffer, global_mask: Mask) {
    let count = buffer.len;
    let mut flags = buffer.scratch_flags;
    let mut i = 0usize;
    while i < count {
        let info = &mut buffer.info[i];
        info.mask = global_mask;
        set_unicode_props(info, &mut flags);
        let cp = info.codepoint;
        if cp < 0x80 {
            i = i.wrapping_add(1);
            continue;
        }
        let gc = info.general_category();
        if matches!(
            gc,
            GeneralCategory::LowercaseLetter
                | GeneralCategory::UppercaseLetter
                | GeneralCategory::TitlecaseLetter
                | GeneralCategory::OtherLetter
                | GeneralCategory::SpaceSeparator
        ) {
            i = i.wrapping_add(1);
            continue;
        }
        if gc == GeneralCategory::ModifierSymbol && (0x1F3FB..=0x1F3FF).contains(&cp) {
            set_continuation(&mut buffer.info[i], &mut flags);
        } else if i > 0 && unicode::is_regional_indicator(cp) {
            let prev = buffer.info[i.wrapping_sub(1)];
            if unicode::is_regional_indicator(prev.codepoint) && !prev.is_continuation() {
                set_continuation(&mut buffer.info[i], &mut flags);
            }
        } else if info.is_zwj() {
            set_continuation(&mut buffer.info[i], &mut flags);
            let next = i.wrapping_add(1);
            if next < count && sofel_unicode::is_extended_pictographic(buffer.info[next].codepoint)
            {
                i = next;
                let info = &mut buffer.info[i];
                info.mask = global_mask;
                set_unicode_props(info, &mut flags);
                set_continuation(info, &mut flags);
            }
        } else if matches!(cp, 0xFF9E..=0xFF9F | 0xE0020..=0xE007F) {
            set_continuation(&mut buffer.info[i], &mut flags);
        } else if cp == 0x2044 {
            flags |= scratch::HAS_FRACTION_SLASH;
        }
        i = i.wrapping_add(1);
    }
    buffer.scratch_flags = flags;
}

/// `hb_insert_dotted_circle`.
fn insert_dotted_circle(buffer: &mut Buffer, font: &ShapeFont<'_, '_>) {
    if buffer
        .flags
        .contains(BufferFlags::DO_NOT_INSERT_DOTTED_CIRCLE)
        || !buffer.flags.contains(BufferFlags::BOT)
        || buffer.context_len[0] != 0
        || !buffer.info[0].is_unicode_mark()
    {
        return;
    }
    if !font.has_glyph(0x25CC) {
        return;
    }
    let first = buffer.info[0];
    let mut dottedcircle = Info {
        codepoint: 0x25CC,
        ..Info::default()
    };
    let mut flags = buffer.scratch_flags;
    set_unicode_props(&mut dottedcircle, &mut flags);
    buffer.scratch_flags = flags;
    buffer.clear_output();
    buffer.idx = 0;
    dottedcircle.cluster = first.cluster;
    dottedcircle.mask = first.mask;
    let _ = buffer.output_info(dottedcircle);
    buffer.sync();
}

/// `_hb_grapheme_group_func`.
fn grapheme_group(_: &Info, b: &Info) -> bool {
    b.is_continuation()
}

/// `hb_form_clusters`.
fn form_clusters(buffer: &mut Buffer) {
    if buffer.scratch_flags & scratch::HAS_CONTINUATIONS == 0 {
        return;
    }
    for (start, end) in buffer.groups(grapheme_group) {
        buffer.merge_grapheme_clusters(start, end);
    }
}

/// `hb_ensure_native_direction`.
fn ensure_native_direction(buffer: &mut Buffer) {
    let direction = buffer.direction;
    let mut horiz = script_horizontal_direction(buffer.script);
    if horiz == Some(Direction::Rtl) && direction == Direction::Ltr {
        let mut found_number = false;
        let mut found_letter = false;
        let mut found_ri = false;
        for info in buffer.infos() {
            let gc = info.general_category();
            if gc == GeneralCategory::DecimalNumber {
                found_number = true;
            } else if unicode::is_letter_category(gc) {
                found_letter = true;
                break;
            } else if unicode::is_regional_indicator(info.codepoint) {
                found_ri = true;
            }
        }
        if (found_number || found_ri) && !found_letter {
            horiz = Some(Direction::Ltr);
        }
    }
    let reverse = (direction.is_horizontal() && horiz.is_some_and(|h| h != direction))
        || (direction.is_vertical() && direction != Direction::Ttb);
    if reverse {
        let merge = buffer.cluster_level == ClusterLevel::MonotoneCharacters;
        buffer.reverse_groups(grapheme_group, merge);
        buffer.direction = direction.reverse();
    }
}

/// `hb_ot_rotate_chars`.
fn rotate_chars(
    plan: &ShapePlan<'_>,
    font: &ShapeFont<'_, '_>,
    buffer: &mut Buffer,
    target: Direction,
) {
    if target.is_backward() {
        let rtlm_mask = plan.rtlm_mask;
        for info in buffer.infos_mut() {
            let m = sofel_unicode::mirroring(info.codepoint);
            if m != info.codepoint && font.has_glyph(m) {
                info.codepoint = m;
            } else {
                info.mask |= rtlm_mask;
            }
        }
    }
    if target.is_vertical() && !plan.has_vert {
        for info in buffer.infos_mut() {
            let v = unicode::vertical_char_for(info.codepoint);
            if v != info.codepoint && font.has_glyph(v) {
                info.codepoint = v;
            }
        }
    }
}

/// `hb_ot_shape_setup_masks_fraction`.
fn setup_masks_fraction(plan: &ShapePlan<'_>, buffer: &mut Buffer) {
    if buffer.scratch_flags & scratch::HAS_FRACTION_SLASH == 0 || !plan.has_frac {
        return;
    }
    let (pre_mask, post_mask) = if buffer.direction.is_forward() {
        (
            plan.numr_mask | plan.frac_mask,
            plan.frac_mask | plan.dnom_mask,
        )
    } else {
        (
            plan.frac_mask | plan.dnom_mask,
            plan.numr_mask | plan.frac_mask,
        )
    };
    let count = buffer.len;
    let mut i = 0usize;
    while i < count {
        if buffer.info[i].codepoint != 0x2044 {
            i = i.wrapping_add(1);
            continue;
        }
        let is_digit =
            |b: &Buffer, k: usize| b.info[k].general_category() == GeneralCategory::DecimalNumber;
        let mut start = i;
        while start > 0 && is_digit(buffer, start.wrapping_sub(1)) {
            start = start.wrapping_sub(1);
        }
        let mut end = i.wrapping_add(1);
        while end < count && is_digit(buffer, end) {
            end = end.wrapping_add(1);
        }
        if start == i || end == i.wrapping_add(1) {
            if start == i {
                buffer.unsafe_to_concat(start, start.wrapping_add(1));
            }
            if end == i.wrapping_add(1) {
                buffer.unsafe_to_concat(end.wrapping_sub(1), end);
            }
            i = i.wrapping_add(1);
            continue;
        }
        buffer.unsafe_to_break(start, end);
        for k in start..i {
            buffer.info[k].mask |= pre_mask;
        }
        buffer.info[i].mask |= plan.frac_mask;
        for k in i.wrapping_add(1)..end {
            buffer.info[k].mask |= post_mask;
        }
        i = end;
    }
}

/// `hb_ot_shape_setup_masks`.
fn setup_masks(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_>, buffer: &mut Buffer) {
    setup_masks_fraction(plan, buffer);
    if let Some(f) = plan.funcs().setup_masks {
        f(plan, buffer, font);
    }
    for f in &plan.user_features {
        if !f.is_global() {
            let (mask, shift) = plan.map.mask(f.tag);
            buffer.set_masks(f.value.wrapping_shl(shift), mask, f.start, f.end);
        }
    }
}

/// `hb_synthesize_glyph_classes`.
fn synthesize_glyph_classes(buffer: &mut Buffer) {
    for info in buffer.infos_mut() {
        info.glyph_props = if info.general_category() != GeneralCategory::NonspacingMark
            || info.is_default_ignorable()
        {
            gprops::BASE_GLYPH
        } else {
            gprops::MARK
        };
    }
}

/// `hb_ot_substitute_pre`.
fn substitute_pre(
    plan: &ShapePlan<'_>,
    font: &ShapeFont<'_, '_>,
    buffer: &mut Buffer,
    target: Direction,
) {
    // hb_ot_substitute_default
    rotate_chars(plan, font, buffer, target);
    normalize(plan, buffer, font);
    setup_masks(plan, font, buffer);
    if plan.fallback_mark_positioning {
        fallback::recategorize_marks(buffer);
    }
    for info in buffer.infos_mut() {
        info.codepoint = info.glyph_index;
    }
    // hb_ot_substitute_plan
    let gdef = &plan.layout.gdef;
    let mut digest = crate::digest::Digest::default();
    for info in buffer.infos_mut() {
        digest.add(info.codepoint);
        info.glyph_props = gdef.glyph_props(info.codepoint);
        info.lig_props = 0;
    }
    buffer.digest = digest;
    if plan.fallback_glyph_classes {
        synthesize_glyph_classes(buffer);
    }
    apply_map(GSUB, plan, font, buffer);
}

/// `hb_ot_deal_with_variation_selectors`.
fn deal_with_variation_selectors(buffer: &mut Buffer) {
    if buffer.scratch_flags & scratch::HAS_VARIATION_SELECTOR_FALLBACK == 0 {
        return;
    }
    let Some(nf) = buffer.not_found_variation_selector else {
        return;
    };
    let (infos, pos) = buffer.infos_and_pos_mut();
    for (info, p) in infos.iter_mut().zip(pos) {
        if info.is_variation_selector() {
            info.codepoint = nf;
            p.x_advance = 0;
            p.y_advance = 0;
            p.x_offset = 0;
            p.y_offset = 0;
            info.set_variation_selector(false);
        }
    }
}

/// `hb_ot_hide_default_ignorables`.
fn hide_default_ignorables(buffer: &mut Buffer, font: &ShapeFont<'_, '_>) {
    if buffer.scratch_flags & scratch::HAS_DEFAULT_IGNORABLES == 0
        || buffer
            .flags
            .contains(BufferFlags::PRESERVE_DEFAULT_IGNORABLES)
    {
        return;
    }
    if !buffer
        .flags
        .contains(BufferFlags::REMOVE_DEFAULT_IGNORABLES)
    {
        let invisible = if buffer.invisible != 0 {
            Some(buffer.invisible)
        } else {
            font.nominal_glyph(0x20)
        };
        if let Some(invisible) = invisible {
            for info in buffer.infos_mut() {
                if info.is_default_ignorable() {
                    info.codepoint = invisible;
                }
            }
            return;
        }
    }
    buffer.delete_glyphs_inplace(Info::is_default_ignorable);
}

/// `hb_ot_substitute_post`.
fn substitute_post(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_>, buffer: &mut Buffer) {
    deal_with_variation_selectors(buffer);
    hide_default_ignorables(buffer, font);
    if let Some(f) = plan.funcs().postprocess_glyphs {
        f(plan, buffer, font);
    }
}

/// `zero_mark_widths_by_gdef`.
fn zero_mark_widths_by_gdef(buffer: &mut Buffer, adjust: bool) {
    let (infos, pos) = buffer.infos_and_pos_mut();
    for (info, p) in infos.iter().zip(pos) {
        if info.is_mark() {
            if adjust {
                p.x_offset = p.x_offset.saturating_sub(p.x_advance);
                p.y_offset = p.y_offset.saturating_sub(p.y_advance);
            }
            p.x_advance = 0;
            p.y_advance = 0;
        }
    }
}

/// `hb_ot_zero_width_default_ignorables`.
fn zero_width_default_ignorables(buffer: &mut Buffer) {
    if buffer.scratch_flags & scratch::HAS_DEFAULT_IGNORABLES == 0
        || buffer
            .flags
            .contains(BufferFlags::PRESERVE_DEFAULT_IGNORABLES)
        || buffer
            .flags
            .contains(BufferFlags::REMOVE_DEFAULT_IGNORABLES)
    {
        return;
    }
    let horizontal = buffer.direction.is_horizontal();
    let (infos, pos) = buffer.infos_and_pos_mut();
    for (info, p) in infos.iter().zip(pos) {
        if info.is_default_ignorable() {
            p.x_advance = 0;
            p.y_advance = 0;
            if horizontal {
                p.x_offset = 0;
            } else {
                p.y_offset = 0;
            }
        }
    }
}

/// `hb_ot_position`.
fn position(plan: &ShapePlan<'_>, font: &ShapeFont<'_, '_>, buffer: &mut Buffer) {
    buffer.clear_positions();
    // hb_ot_position_default
    let horizontal = buffer.direction.is_horizontal();
    {
        let (infos, pos) = buffer.infos_and_pos_mut();
        for (info, p) in infos.iter().zip(pos) {
            if horizontal {
                p.x_advance = font.h_advance(info.codepoint);
            } else {
                p.y_advance = font.v_advance(info.codepoint);
                let (x, y) = font.v_origin(info.codepoint);
                p.x_offset = p.x_offset.wrapping_sub(x);
                p.y_offset = p.y_offset.wrapping_sub(y);
            }
        }
    }
    if buffer.scratch_flags & scratch::HAS_SPACE_FALLBACK != 0 {
        fallback::fallback_spaces(font, buffer);
    }
    // hb_ot_position_plan
    let adjust = plan.adjust_mark_positioning_when_zeroing && buffer.direction.is_forward();
    gpos::position_start(buffer);
    let zwm = plan.funcs().zero_width_marks;
    if plan.zero_marks && zwm == ZeroWidthMarks::ByGdefEarly {
        zero_mark_widths_by_gdef(buffer, adjust);
    }
    if plan.apply_gpos {
        apply_map(GPOS, plan, font, buffer);
    }
    // The `ot` font functions have no kerning: fallback kerning does
    // nothing.
    if plan.zero_marks && zwm == ZeroWidthMarks::ByGdefLate {
        zero_mark_widths_by_gdef(buffer, adjust);
    }
    zero_width_default_ignorables(buffer);
    gpos::position_finish_offsets(buffer);
    if plan.fallback_mark_positioning {
        fallback::position_marks(plan, font, buffer, adjust);
    }
    if buffer.direction.is_backward() {
        buffer.reverse();
    }
}

/// `hb_propagate_flags`.
fn propagate_flags(buffer: &mut Buffer) {
    let defined = GlyphFlags::DEFINED.0;
    let mut and_mask = defined;
    if !buffer.flags.contains(BufferFlags::PRODUCE_UNSAFE_TO_CONCAT) {
        and_mask &= !GlyphFlags::UNSAFE_TO_CONCAT.0;
    }
    let tatweel = buffer
        .flags
        .contains(BufferFlags::PRODUCE_SAFE_TO_INSERT_TATWEEL);
    for (start, end) in buffer.groups(|a, b| a.cluster == b.cluster) {
        let infos = buffer.info.range_mut(start, end);
        let mut mask = infos.iter().fold(0, |m, i| m | i.mask) & defined;
        if tatweel {
            if mask & GlyphFlags::UNSAFE_TO_BREAK.0 != 0 {
                mask &= !GlyphFlags::SAFE_TO_INSERT_TATWEEL.0;
            }
            if mask & GlyphFlags::SAFE_TO_INSERT_TATWEEL.0 != 0 {
                mask |= GlyphFlags::UNSAFE_TO_BREAK.0 | GlyphFlags::UNSAFE_TO_CONCAT.0;
            }
        }
        mask &= and_mask;
        for i in infos {
            i.mask = (i.mask & !defined) | mask;
        }
    }
}

/// Shapes with HarfBuzz's `fallback` shaper (`_hb_fallback_shape`): the
/// cmap glyph of every character (the space glyph, zero-advanced, for
/// default-ignorables), its advance and vertical origin, the clusters
/// reversed for backward text, and no layout tables.
pub fn shape_fallback(
    font: &Font<'_>,
    scale: &Scale,
    props: &ResolvedProperties,
    buffer: UnicodeBuffer,
) -> GlyphBuffer {
    let mut buf = buffer.buf;
    buf.direction = props.direction;
    buf.script = props.script;
    let extents = |g: GlyphId| font.default_instance().glyf_extents(g);
    let sf = ShapeFont::new(font, *scale, &extents);
    let space = sf.nominal_glyph(0x20);
    buf.clear_positions();
    let direction = buf.direction;
    let (infos, pos) = buf.infos_and_pos_mut();
    for (info, p) in infos.iter_mut().zip(pos) {
        if let Some(space) = space
            && unicode::is_default_ignorable(info.codepoint)
        {
            info.codepoint = space;
            p.x_advance = 0;
            p.y_advance = 0;
            continue;
        }
        info.codepoint = sf.nominal_glyph(info.codepoint).unwrap_or(0);
        if direction.is_horizontal() {
            p.x_advance = sf.h_advance(info.codepoint);
        } else {
            p.y_advance = sf.v_advance(info.codepoint);
            let (x, y) = sf.v_origin(info.codepoint);
            p.x_offset = p.x_offset.wrapping_sub(x);
            p.y_offset = p.y_offset.wrapping_sub(y);
        }
    }
    if direction.is_backward() {
        buf.reverse();
    }
    buf.clear_glyph_flags();
    into_glyph_buffer(buf)
}
