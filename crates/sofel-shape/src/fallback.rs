//! Positioning without `GPOS` (ADR 0020, 位置): marks placed from glyph
//! boxes and combining classes, widths for space characters the font
//! lacks (`_hb_ot_shape_fallback_*`).
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-shape-fallback.cc`, and
//! HarfRust 0.15.0 `src/fallback.rs`, MIT; see THIRD_PARTY_NOTICES.md.

use sofel_font::GlyphExtents;
use sofel_unicode::GeneralCategory;

use crate::buffer::Buffer;
use crate::direction::Direction;
use crate::font::ShapeFont;
use crate::plan::ShapePlan;
use crate::props::script_horizontal_direction;
use crate::unicode::SpaceFallback;

/// The combining classes `position_mark` distinguishes
/// (`hb_unicode_combining_class_t`).
mod ccc {
    pub(crate) const ATTACHED_BELOW_LEFT: u8 = 200;
    pub(crate) const ATTACHED_BELOW: u8 = 202;
    pub(crate) const ATTACHED_ABOVE: u8 = 214;
    pub(crate) const ATTACHED_ABOVE_RIGHT: u8 = 216;
    pub(crate) const BELOW_LEFT: u8 = 218;
    pub(crate) const BELOW: u8 = 220;
    pub(crate) const BELOW_RIGHT: u8 = 222;
    pub(crate) const ABOVE_LEFT: u8 = 228;
    pub(crate) const ABOVE: u8 = 230;
    pub(crate) const ABOVE_RIGHT: u8 = 232;
    pub(crate) const DOUBLE_BELOW: u8 = 233;
    pub(crate) const DOUBLE_ABOVE: u8 = 234;
}

/// `recategorize_combining_class`.
fn recategorize_combining_class(u: u32, class: u8) -> u8 {
    use ccc::*;
    if class >= 200 {
        return class;
    }
    let mut klass = class;
    if u & !0xFF == 0x0E00 {
        if klass == 0 {
            klass = match u {
                0x0E31 | 0x0E34 | 0x0E35 | 0x0E36 | 0x0E37 | 0x0E47 | 0x0E4C | 0x0E4D | 0x0E4E => {
                    ABOVE_RIGHT
                }
                0x0EB1 | 0x0EB4 | 0x0EB5 | 0x0EB6 | 0x0EB7 | 0x0EBB | 0x0ECC | 0x0ECD => ABOVE,
                0x0EBC => BELOW,
                _ => klass,
            };
        } else if u == 0x0E3A {
            klass = BELOW_RIGHT;
        }
    }
    match klass {
        // Hebrew (modified classes).
        22 | 15 | 16 | 17 | 23 | 18 | 19 | 20 | 21 | 24 | 25 => BELOW,
        13 => ATTACHED_ABOVE,
        10 => ABOVE_RIGHT,
        11 | 14 => ABOVE_LEFT,
        26 => ABOVE,
        12 => klass,
        // Arabic and Syriac.
        28 | 29 | 31 | 32 | 27 | 34 | 35 | 36 => ABOVE,
        30 | 33 => BELOW,
        // Thai.
        3 => BELOW_RIGHT,
        107 => ABOVE_RIGHT,
        // Lao.
        118 => BELOW,
        122 => ABOVE,
        // Tibetan.
        129 => BELOW,
        132 => ABOVE,
        131 => BELOW,
        _ => klass,
    }
}

/// `_hb_ot_shape_fallback_mark_position_recategorize_marks`.
pub(crate) fn recategorize_marks(buffer: &mut Buffer) {
    for info in buffer.infos_mut() {
        if info.general_category() == GeneralCategory::NonspacingMark {
            let c = recategorize_combining_class(info.codepoint, info.modified_combining_class());
            info.set_modified_combining_class(c);
        }
    }
}

/// `zero_mark_advances`.
fn zero_mark_advances(buffer: &mut Buffer, start: usize, end: usize, adjust: bool) {
    for i in start..end {
        if buffer.info[i].general_category() == GeneralCategory::NonspacingMark {
            let p = &mut buffer.pos[i];
            if adjust {
                p.x_offset = p.x_offset.saturating_sub(p.x_advance);
                p.y_offset = p.y_offset.saturating_sub(p.y_advance);
            }
            p.x_advance = 0;
            p.y_advance = 0;
        }
    }
}

/// `hb_clamp_to<hb_position_t> (int64_t)`.
fn clamp(v: i64) -> i32 {
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
}

/// `position_mark`.
fn position_mark(
    font: &ShapeFont<'_, '_>,
    buffer: &mut Buffer,
    base: &mut GlyphExtents,
    i: usize,
    class: u8,
) {
    use ccc::*;
    let Some(mark) = font.glyph_extents(buffer.info[i].codepoint) else {
        return;
    };
    let y_gap = font.y_scale() / 16;
    let direction = buffer.direction;
    let pos = &mut buffer.pos[i];
    pos.x_offset = 0;
    pos.y_offset = 0;
    let (bx, bw) = (i64::from(base.x_bearing), i64::from(base.width));
    let (mx, mw) = (i64::from(mark.x_bearing), i64::from(mark.width));
    let centered = || clamp(bx + (bw - mw) / 2 - mx);
    pos.x_offset = match class {
        DOUBLE_BELOW | DOUBLE_ABOVE if direction == Direction::Ltr => {
            clamp(bx + bw - i64::from(mark.width / 2) - mx)
        }
        DOUBLE_BELOW | DOUBLE_ABOVE if direction == Direction::Rtl => {
            clamp(bx - i64::from(mark.width / 2) - mx)
        }
        ATTACHED_BELOW_LEFT | BELOW_LEFT | ABOVE_LEFT => base.x_bearing.saturating_sub(mark.x_bearing),
        ATTACHED_ABOVE_RIGHT | BELOW_RIGHT | ABOVE_RIGHT => clamp(bx + bw - mw - mx),
        _ => centered(),
    };
    match class {
        DOUBLE_BELOW | BELOW_LEFT | BELOW | BELOW_RIGHT | ATTACHED_BELOW_LEFT | ATTACHED_BELOW => {
            if !matches!(class, ATTACHED_BELOW_LEFT | ATTACHED_BELOW) {
                base.height = base.height.saturating_sub(y_gap);
            }
            pos.y_offset = clamp(
                i64::from(base.y_bearing) + i64::from(base.height) - i64::from(mark.y_bearing),
            );
            if (y_gap > 0) == (pos.y_offset > 0) {
                base.height = base.height.saturating_sub(pos.y_offset);
                pos.y_offset = 0;
            }
            base.height = base.height.saturating_add(mark.height);
        }
        DOUBLE_ABOVE | ABOVE_LEFT | ABOVE | ABOVE_RIGHT | ATTACHED_ABOVE | ATTACHED_ABOVE_RIGHT => {
            if !matches!(class, ATTACHED_ABOVE | ATTACHED_ABOVE_RIGHT) {
                base.y_bearing = base.y_bearing.saturating_add(y_gap);
                base.height = base.height.saturating_sub(y_gap);
            }
            pos.y_offset = clamp(
                i64::from(base.y_bearing) - i64::from(mark.y_bearing) - i64::from(mark.height),
            );
            if (y_gap > 0) != (pos.y_offset > 0) {
                let correction = (pos.y_offset / 2).checked_neg().unwrap_or(i32::MAX);
                base.y_bearing = base.y_bearing.saturating_add(correction);
                base.height = base.height.saturating_sub(correction);
                pos.y_offset = pos.y_offset.saturating_add(correction);
            }
            base.y_bearing = base.y_bearing.saturating_sub(mark.height);
            base.height = base.height.saturating_add(mark.height);
        }
        _ => {}
    }
}

/// `position_around_base`.
fn position_around_base(
    plan: &ShapePlan<'_>,
    font: &ShapeFont<'_, '_>,
    buffer: &mut Buffer,
    base: usize,
    end: usize,
    adjust: bool,
) {
    let mut horiz_dir: Option<Option<Direction>> = None;
    buffer.unsafe_to_break(base, end);
    let base_info = buffer.info[base];
    let Some(mut base_extents) = font.glyph_extents(base_info.codepoint) else {
        zero_mark_advances(buffer, base.wrapping_add(1), end, adjust);
        return;
    };
    base_extents.y_bearing = base_extents.y_bearing.saturating_add(buffer.pos[base].y_offset);
    base_extents.x_bearing = 0;
    base_extents.width = font.h_advance(base_info.codepoint);
    let lig_id = base_info.lig_id();
    let num_lig_components = i64::from(base_info.lig_num_comps());
    let (mut x_offset, mut y_offset) = (0i32, 0i32);
    let forward = buffer.direction.is_forward();
    if forward {
        x_offset = buffer.pos[base].x_advance.checked_neg().unwrap_or(i32::MAX);
        y_offset = buffer.pos[base].y_advance.checked_neg().unwrap_or(i32::MAX);
    }
    let mut component_extents = base_extents;
    let mut last_lig_component: i64 = -1;
    let mut last_combining_class: u16 = 255;
    let mut cluster_extents = base_extents;
    for i in base.wrapping_add(1)..end {
        let info = buffer.info[i];
        let class = info.modified_combining_class();
        if class != 0 {
            if num_lig_components > 1 {
                let this_lig_id = info.lig_id();
                let mut this_comp = i64::from(info.lig_comp()) - 1;
                if lig_id == 0 || lig_id != this_lig_id || this_comp >= num_lig_components {
                    this_comp = num_lig_components - 1;
                }
                if last_lig_component != this_comp {
                    last_lig_component = this_comp;
                    last_combining_class = 255;
                    component_extents = base_extents;
                    let dir = *horiz_dir.get_or_insert_with(|| {
                        if plan.props.direction.is_horizontal() {
                            Some(plan.props.direction)
                        } else {
                            script_horizontal_direction(plan.props.script)
                        }
                    });
                    let x = i64::from(component_extents.x_bearing);
                    let w = i64::from(component_extents.width);
                    let k = if dir == Some(Direction::Ltr) {
                        this_comp
                    } else {
                        num_lig_components - 1 - this_comp
                    };
                    component_extents.x_bearing =
                        clamp(x + (k * w).checked_div(num_lig_components).unwrap_or(0));
                    component_extents.width = i32::try_from(
                        w.checked_div(num_lig_components).unwrap_or(0),
                    )
                    .unwrap_or(0);
                }
            }
            if last_combining_class != u16::from(class) {
                last_combining_class = u16::from(class);
                cluster_extents = component_extents;
            }
            position_mark(font, buffer, &mut cluster_extents, i, class);
            let p = &mut buffer.pos[i];
            p.x_advance = 0;
            p.y_advance = 0;
            p.x_offset = p.x_offset.saturating_add(x_offset);
            p.y_offset = p.y_offset.saturating_add(y_offset);
        } else {
            let p = buffer.pos[i];
            if forward {
                x_offset = x_offset.saturating_sub(p.x_advance);
                y_offset = y_offset.saturating_sub(p.y_advance);
            } else {
                x_offset = x_offset.saturating_add(p.x_advance);
                y_offset = y_offset.saturating_add(p.y_advance);
            }
        }
    }
}

/// `position_cluster`.
fn position_cluster(
    plan: &ShapePlan<'_>,
    font: &ShapeFont<'_, '_>,
    buffer: &mut Buffer,
    start: usize,
    end: usize,
    adjust: bool,
) {
    if end.saturating_sub(start) < 2 {
        return;
    }
    let mut i = start;
    while i < end {
        if !buffer.info[i].is_unicode_mark() {
            let mut j = i.wrapping_add(1);
            while j < end {
                let info = buffer.info[j];
                if info.is_hidden() || info.is_default_ignorable() {
                    j = j.wrapping_add(1);
                    continue;
                }
                if !info.is_unicode_mark() {
                    break;
                }
                j = j.wrapping_add(1);
            }
            position_around_base(plan, font, buffer, i, j, adjust);
            i = j;
        } else {
            i = i.wrapping_add(1);
        }
    }
}

/// `_hb_ot_shape_fallback_mark_position`.
pub(crate) fn position_marks(
    plan: &ShapePlan<'_>,
    font: &ShapeFont<'_, '_>,
    buffer: &mut Buffer,
    adjust: bool,
) {
    let mut start = 0usize;
    let count = buffer.len;
    for i in 1..count {
        let info = buffer.info[i];
        if !info.is_unicode_mark() && !info.is_hidden() && !info.is_default_ignorable() {
            position_cluster(plan, font, buffer, start, i, adjust);
            start = i;
        }
    }
    position_cluster(plan, font, buffer, start, count, adjust);
}

/// `_hb_ot_shape_fallback_spaces`.
pub(crate) fn fallback_spaces(font: &ShapeFont<'_, '_>, buffer: &mut Buffer) {
    let horizontal = buffer.direction.is_horizontal();
    let invisible = buffer.invisible;
    let (xs, ys) = (i64::from(font.x_scale()), i64::from(font.y_scale()));
    let wrap = |v: i64| {
        let b = v.to_le_bytes();
        i32::from_le_bytes([b[0], b[1], b[2], b[3]])
    };
    for i in 0..buffer.len {
        let info = buffer.info[i];
        if !info.is_unicode_space() || info.ligated() {
            continue;
        }
        if invisible != 0 && info.codepoint == invisible {
            if horizontal {
                buffer.pos[i].x_advance = font.x_scale() / 4;
            } else {
                buffer.pos[i].y_advance = wrap((-ys) / 4);
            }
        }
        let advance_of = |g: u32| {
            if horizontal {
                (Some(font.h_advance(g)), None)
            } else {
                (None, Some(font.v_advance(g)))
            }
        };
        let p = &mut buffer.pos[i];
        match info.space_fallback() {
            SpaceFallback::NotSpace | SpaceFallback::Space => {}
            s @ (SpaceFallback::Em
            | SpaceFallback::Em2
            | SpaceFallback::Em3
            | SpaceFallback::Em4
            | SpaceFallback::Em5
            | SpaceFallback::Em6
            | SpaceFallback::Em16) => {
                let n = i64::from(s as u8);
                if horizontal {
                    p.x_advance = wrap((xs + n / 2).checked_div(n).unwrap_or(0));
                } else {
                    p.y_advance = wrap(-((ys + n / 2).checked_div(n).unwrap_or(0)));
                }
            }
            SpaceFallback::FourEm18 => {
                if horizontal {
                    p.x_advance = wrap(xs * 4 / 18);
                } else {
                    p.y_advance = wrap(-ys * 4 / 18);
                }
            }
            SpaceFallback::Figure => {
                if let Some(g) = (u32::from(b'0')..=u32::from(b'9')).find_map(|u| font.nominal_glyph(u)) {
                    let (h, v) = advance_of(g);
                    if let Some(h) = h {
                        p.x_advance = h;
                    }
                    if let Some(v) = v {
                        p.y_advance = v;
                    }
                }
            }
            SpaceFallback::Punctuation => {
                let g = font
                    .nominal_glyph(u32::from(b'.'))
                    .or_else(|| font.nominal_glyph(u32::from(b',')));
                if let Some(g) = g {
                    let (h, v) = advance_of(g);
                    if let Some(h) = h {
                        p.x_advance = h;
                    }
                    if let Some(v) = v {
                        p.y_advance = v;
                    }
                }
            }
            SpaceFallback::Narrow => {
                if horizontal {
                    p.x_advance /= 2;
                } else {
                    p.y_advance /= 2;
                }
            }
        }
    }
}
