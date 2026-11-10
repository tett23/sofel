//! Applying `GPOS` subtables, and finishing attachments
//! (`position_start`, `position_finish_offsets`).
//!
//! Ported from HarfBuzz 15.1.0 `src/OT/Layout/GPOS/`, and HarfRust
//! 0.15.0 `src/ot/lookup/gpos.rs` and `src/ot/gpos/mod.rs`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_font::read::GlyphId;
use sofel_font::read::gpos::{Anchor, AnchorMatrix, AnchorResult, MarkArray, PosSubtable, ValueRecord};
use sofel_font::read::{ReadError, layout::lookup_flag};

use super::apply::{ApplyContext, Match, Which, apply_chain_context, apply_context};
use crate::buffer::{Buffer, Pos, scratch};
use crate::direction::Direction;
use crate::font::round_to_i32;

/// `ATTACH_TYPE_MARK`.
pub(crate) const ATTACH_TYPE_MARK: u8 = 0x01;
/// `ATTACH_TYPE_CURSIVE`.
pub(crate) const ATTACH_TYPE_CURSIVE: u8 = 0x02;

/// `LookupFlag::IgnoreMarks`.
const IGNORE_MARKS: u32 = 0x0008;
/// `LookupFlag::IgnoreFlags`.
const IGNORE_FLAGS: u32 = 0x000E;

/// `ValueFormat::apply_value`: adds the record's values to `pos` and
/// says whether any was not zero.
fn apply_value(c: &ApplyContext<'_, '_, '_, '_, '_>, v: &ValueRecord<'_>, pos: &mut Pos) -> bool {
    let f = v.format.0;
    if f == 0 {
        return false;
    }
    let font = c.font;
    let horizontal = c.direction.is_horizontal();
    let mut ret = false;
    use sofel_font::read::gpos::ValueFormat as VF;
    if f & VF::X_PLACEMENT != 0 {
        ret |= v.x_placement != 0;
        pos.x_offset = pos.x_offset.saturating_add(font.em_scale_x(v.x_placement));
    }
    if f & VF::Y_PLACEMENT != 0 {
        ret |= v.y_placement != 0;
        pos.y_offset = pos.y_offset.saturating_add(font.em_scale_y(v.y_placement));
    }
    if f & VF::X_ADVANCE != 0 && horizontal {
        ret |= v.x_advance != 0;
        pos.x_advance = pos.x_advance.saturating_add(font.em_scale_x(v.x_advance));
    }
    if f & VF::Y_ADVANCE != 0 && !horizontal {
        ret |= v.y_advance != 0;
        pos.y_advance = pos.y_advance.saturating_sub(font.em_scale_y(v.y_advance));
    }
    if f & VF::DEVICES == 0 {
        return ret;
    }
    let scale = font.scale;
    let use_x = scale.x_ppem != 0;
    let use_y = scale.y_ppem != 0;
    if !use_x && !use_y {
        return ret;
    }
    if f & VF::X_PLACEMENT_DEVICE != 0 && use_x {
        ret |= v.x_placement_device.is_some();
        pos.x_offset = pos
            .x_offset
            .saturating_add(scale.x_delta(v.x_placement_device.as_ref()));
    }
    if f & VF::Y_PLACEMENT_DEVICE != 0 && use_y {
        ret |= v.y_placement_device.is_some();
        pos.y_offset = pos
            .y_offset
            .saturating_add(scale.y_delta(v.y_placement_device.as_ref()));
    }
    if f & VF::X_ADVANCE_DEVICE != 0 && horizontal && use_x {
        ret |= v.x_advance_device.is_some();
        pos.x_advance = pos
            .x_advance
            .saturating_add(scale.x_delta(v.x_advance_device.as_ref()));
    }
    if f & VF::Y_ADVANCE_DEVICE != 0 && !horizontal && use_y {
        ret |= v.y_advance_device.is_some();
        pos.y_advance = pos
            .y_advance
            .saturating_sub(scale.y_delta(v.y_advance_device.as_ref()));
    }
    ret
}

/// An anchor as HarfBuzz uses it: `None` when the offset is null or
/// the table cannot be read (sanitize fails); an unknown format is
/// found at (0, 0).
#[derive(Clone, Copy)]
enum AnchorRef<'a> {
    Known(Anchor<'a>),
    Unknown,
}

fn anchor(r: AnchorResult<'_>) -> Option<AnchorRef<'_>> {
    match r {
        Ok(Some(a)) => Some(AnchorRef::Known(a)),
        Err(ReadError::UnknownFormat(_)) => Some(AnchorRef::Unknown),
        _ => None,
    }
}

/// `Anchor::get_anchor`.
fn get_anchor(c: &ApplyContext<'_, '_, '_, '_, '_>, a: Option<AnchorRef<'_>>) -> (f32, f32) {
    let font = c.font;
    match a {
        Some(AnchorRef::Known(Anchor::Format1 { x, y }))
        | Some(AnchorRef::Known(Anchor::Format2 { x, y, .. })) => {
            (font.em_fscale_x(x), font.em_fscale_y(y))
        }
        Some(AnchorRef::Known(Anchor::Format3 {
            x,
            y,
            x_device,
            y_device,
        })) => {
            let mut fx = font.em_fscale_x(x);
            let mut fy = font.em_fscale_y(y);
            if font.scale.x_ppem != 0 {
                fx += crate::font::i32_to_f32(font.scale.x_delta(x_device.as_ref()));
            }
            if font.scale.y_ppem != 0 {
                fy += crate::font::i32_to_f32(font.scale.y_delta(y_device.as_ref()));
            }
            (fx, fy)
        }
        _ => (0.0, 0.0),
    }
}

/// Applies a subtable at the current glyph.
pub(crate) fn apply<'a>(c: &mut ApplyContext<'_, 'a, '_, '_, '_>, s: &PosSubtable<'a>) -> bool {
    let glyph = GlyphId(c.buffer.cur(0).codepoint);
    match s {
        PosSubtable::Single { coverage, value } => {
            if coverage.get(glyph).is_none() {
                return false;
            }
            let mut p = *c.buffer.cur_pos_mut();
            apply_value(c, value, &mut p);
            *c.buffer.cur_pos_mut() = p;
            c.buffer.idx = c.buffer.idx.wrapping_add(1);
            true
        }
        PosSubtable::SingleArray { coverage, values } => {
            let Some(index) = coverage.get(glyph) else {
                return false;
            };
            let Some(v) = usize::try_from(index).ok().and_then(|i| values.get(i)) else {
                return false;
            };
            let mut p = *c.buffer.cur_pos_mut();
            apply_value(c, &v, &mut p);
            *c.buffer.cur_pos_mut() = p;
            c.buffer.idx = c.buffer.idx.wrapping_add(1);
            true
        }
        PosSubtable::PairGlyphs {
            coverage,
            format1,
            format2,
            pair_sets,
        } => {
            let Some(index) = coverage.get(glyph) else {
                return false;
            };
            let idx = c.buffer.idx;
            c.iter_reset_fast(Which::Input, idx);
            if let Err(unsafe_to) = c.iter_next(Which::Input) {
                c.buffer.unsafe_to_concat(idx, unsafe_to);
                return false;
            }
            let pos = c.iter_input.idx;
            let set = usize::try_from(index)
                .ok()
                .and_then(|i| pair_sets.get(i).ok().flatten());
            let second = GlyphId(c.buffer.info[pos].codepoint);
            let Some((v1, v2)) = set.and_then(|s| s.find(second)) else {
                c.buffer.unsafe_to_concat(idx, pos.wrapping_add(1));
                return false;
            };
            apply_pair(c, &v1, &v2, format1.len(), format2.len(), pos);
            true
        }
        PosSubtable::PairClasses {
            coverage,
            format1,
            format2,
            class_def1,
            class_def2,
            class1_count,
            class2_count,
            values,
        } => {
            if coverage.get(glyph).is_none() {
                return false;
            }
            let idx = c.buffer.idx;
            c.iter_reset_fast(Which::Input, idx);
            if let Err(unsafe_to) = c.iter_next(Which::Input) {
                c.buffer.unsafe_to_concat(idx, unsafe_to);
                return false;
            }
            let pos = c.iter_input.idx;
            let k1 = class_def1.get(glyph);
            let k2 = class_def2.get(GlyphId(c.buffer.info[pos].codepoint));
            if k1 >= u32::from(*class1_count) || k2 >= u32::from(*class2_count) {
                c.buffer.unsafe_to_concat(idx, pos.wrapping_add(1));
                return false;
            }
            let pair = u16::try_from(k1)
                .ok()
                .zip(u16::try_from(k2).ok())
                .and_then(|(a, b)| values.get(a, b));
            match pair {
                Some((v1, v2)) => apply_pair(c, &v1, &v2, format1.len(), format2.len(), pos),
                None => {
                    // Values past the data read as zero.
                    let v1 = ValueRecord::read(&[], &[], *format1);
                    let v2 = ValueRecord::read(&[], &[], *format2);
                    apply_pair(c, &v1, &v2, format1.len(), format2.len(), pos);
                }
            }
            true
        }
        PosSubtable::Cursive {
            coverage,
            entry_exits,
        } => {
            let record = |g: GlyphId| {
                coverage
                    .get(g)
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| entry_exits.get(i))
            };
            let Some(entry) = record(glyph).and_then(|(entry, _)| anchor(entry)) else {
                return false;
            };
            let idx = c.buffer.idx;
            c.iter_reset_fast(Which::Input, idx);
            let i = match c.iter_prev(Which::Input) {
                Ok(()) => c.iter_input.idx,
                Err(unsafe_from) => {
                    c.buffer
                        .unsafe_to_concat_from_outbuffer(unsafe_from, idx.wrapping_add(1));
                    return false;
                }
            };
            let prev_glyph = GlyphId(c.buffer.info[i].codepoint);
            let Some(exit) = record(prev_glyph).and_then(|(_, exit)| anchor(exit)) else {
                c.buffer.unsafe_to_concat_from_outbuffer(i, idx.wrapping_add(1));
                return false;
            };
            apply_cursive(c, exit, entry, i);
            true
        }
        PosSubtable::MarkBase {
            mark_coverage,
            base_coverage,
            class_count,
            marks,
            bases,
        } => {
            let Some(mark_index) = mark_coverage.get(glyph) else {
                return false;
            };
            let accept = |b: &Buffer, i: usize| {
                let info = b.info[i];
                !info.multiplied()
                    || info.lig_comp() == 0
                    || i == 0
                    || b.info[i.wrapping_sub(1)].is_mark()
                    || !b.info[i.wrapping_sub(1)].multiplied()
                    || info.lig_id() != b.info[i.wrapping_sub(1)].lig_id()
                    || info.lig_comp() != b.info[i.wrapping_sub(1)].lig_comp().wrapping_add(1)
            };
            let Some(idx) = find_base(c, |b, i| {
                accept(b, i) || base_coverage.get(GlyphId(b.info[i].codepoint)).is_some()
            }) else {
                return false;
            };
            let Some(base_index) = base_coverage.get(GlyphId(c.buffer.info[idx].codepoint)) else {
                let cur = c.buffer.idx;
                c.buffer.unsafe_to_concat_from_outbuffer(idx, cur.wrapping_add(1));
                return false;
            };
            mark_array_apply(c, marks, mark_index, base_index, bases, *class_count, idx)
        }
        PosSubtable::MarkLig {
            mark_coverage,
            ligature_coverage,
            class_count,
            marks,
            ligatures,
        } => {
            let Some(mark_index) = mark_coverage.get(glyph) else {
                return false;
            };
            let Some(idx) = find_base(c, |b, i| {
                let info = b.info[i];
                !info.multiplied()
                    || info.lig_comp() == 0
                    || ligature_coverage.get(GlyphId(info.codepoint)).is_some()
            }) else {
                return false;
            };
            let cur = c.buffer.idx;
            let Some(lig_index) = ligature_coverage.get(GlyphId(c.buffer.info[idx].codepoint))
            else {
                c.buffer.unsafe_to_concat_from_outbuffer(idx, cur.wrapping_add(1));
                return false;
            };
            let attach = usize::try_from(lig_index)
                .ok()
                .and_then(|i| ligatures.get(i).ok().flatten())
                .unwrap_or(AnchorMatrix::empty());
            let comp_count = attach.rows();
            if comp_count == 0 {
                c.buffer.unsafe_to_concat_from_outbuffer(idx, cur.wrapping_add(1));
                return false;
            }
            let lig_id = c.buffer.info[idx].lig_id();
            let mark_id = c.buffer.cur(0).lig_id();
            let mark_comp = usize::from(c.buffer.cur(0).lig_comp());
            let comp_index = if lig_id != 0 && lig_id == mark_id && mark_comp > 0 {
                comp_count.min(mark_comp).wrapping_sub(1)
            } else {
                comp_count.wrapping_sub(1)
            };
            let row = u32::try_from(comp_index).unwrap_or(u32::MAX);
            mark_array_apply(c, marks, mark_index, row, &attach, *class_count, idx)
        }
        PosSubtable::MarkMark {
            mark1_coverage,
            mark2_coverage,
            class_count,
            marks,
            mark2s,
        } => {
            let Some(mark1_index) = mark1_coverage.get(glyph) else {
                return false;
            };
            let idx = c.buffer.idx;
            c.iter_reset_fast(Which::Input, idx);
            let props = c.lookup_props & !IGNORE_FLAGS;
            c.iter_input.set_lookup_props(props);
            let j = match c.iter_prev(Which::Input) {
                Ok(()) => c.iter_input.idx,
                Err(unsafe_from) => {
                    c.buffer
                        .unsafe_to_concat_from_outbuffer(unsafe_from, idx.wrapping_add(1));
                    return false;
                }
            };
            let prev = c.buffer.info[j];
            if !prev.is_mark() {
                c.buffer.unsafe_to_concat_from_outbuffer(j, idx.wrapping_add(1));
                return false;
            }
            let cur = *c.buffer.cur(0);
            let (id1, id2) = (cur.lig_id(), prev.lig_id());
            let (comp1, comp2) = (cur.lig_comp(), prev.lig_comp());
            let good = if id1 == id2 {
                id1 == 0 || comp1 == comp2
            } else {
                (id1 > 0 && comp1 == 0) || (id2 > 0 && comp2 == 0)
            };
            if !good {
                c.buffer.unsafe_to_concat_from_outbuffer(j, idx.wrapping_add(1));
                return false;
            }
            let Some(mark2_index) = mark2_coverage.get(GlyphId(prev.codepoint)) else {
                c.buffer.unsafe_to_concat_from_outbuffer(j, idx.wrapping_add(1));
                return false;
            };
            mark_array_apply(c, marks, mark1_index, mark2_index, mark2s, *class_count, j)
        }
        PosSubtable::Context(ctx) => apply_context(c, ctx),
        PosSubtable::ChainContext(ctx) => apply_chain_context(c, ctx),
    }
}

/// The search for the base or ligature of `MarkBasePos` and
/// `MarkLigPos`, which keeps the last base found for the lookup:
/// `accept` says whether a matching glyph may be used.
fn find_base(
    c: &mut ApplyContext<'_, '_, '_, '_, '_>,
    accept: impl Fn(&Buffer, usize) -> bool,
) -> Option<usize> {
    c.iter_input.set_lookup_props(IGNORE_MARKS);
    let idx = c.buffer.idx;
    if c.last_base_until > idx {
        c.last_base_until = 0;
        c.last_base = None;
    }
    let mut j = idx;
    while j > c.last_base_until {
        let info = c.buffer.info[j.wrapping_sub(1)];
        let mut m = c.match_at(Which::Input, &info);
        if m == Match::Match && !accept(c.buffer, j.wrapping_sub(1)) {
            m = Match::Skip;
        }
        if m == Match::Match {
            c.last_base = Some(j.wrapping_sub(1));
            break;
        }
        j = j.wrapping_sub(1);
    }
    c.last_base_until = idx;
    match c.last_base {
        Some(b) => Some(b),
        None => {
            c.buffer.unsafe_to_concat_from_outbuffer(0, idx.wrapping_add(1));
            None
        }
    }
}

/// `PairSet::apply` and the tail of `PairPosFormat2::apply`.
fn apply_pair(
    c: &mut ApplyContext<'_, '_, '_, '_, '_>,
    v1: &ValueRecord<'_>,
    v2: &ValueRecord<'_>,
    len1: usize,
    len2: usize,
    pos: usize,
) {
    let idx = c.buffer.idx;
    let mut p1 = *c.buffer.cur_pos_mut();
    let applied_first = len1 > 0 && apply_value(c, v1, &mut p1);
    *c.buffer.cur_pos_mut() = p1;
    let mut p2 = c.buffer.pos[pos];
    let applied_second = len2 > 0 && apply_value(c, v2, &mut p2);
    c.buffer.pos[pos] = p2;
    if applied_first || applied_second {
        c.buffer.unsafe_to_break(idx, pos.wrapping_add(1));
    } else {
        c.buffer.unsafe_to_concat(idx, pos.wrapping_add(1));
    }
    let mut pos = pos;
    if len2 > 0 {
        pos = pos.wrapping_add(1);
        c.buffer.unsafe_to_break(idx, pos.wrapping_add(1));
    }
    c.buffer.idx = pos;
}

/// `MarkArray::apply`.
#[allow(clippy::too_many_arguments)]
fn mark_array_apply(
    c: &mut ApplyContext<'_, '_, '_, '_, '_>,
    marks: &MarkArray<'_>,
    mark_index: u32,
    row: u32,
    anchors: &AnchorMatrix<'_>,
    class_count: u16,
    glyph_pos: usize,
) -> bool {
    let (mark_class, mark_anchor) = match usize::try_from(mark_index)
        .ok()
        .and_then(|i| marks.get(i))
    {
        // A Null anchor (format 0) is (0, 0).
        Some((class, a)) => (class, anchor(a).or(Some(AnchorRef::Unknown))),
        None => (0, Some(AnchorRef::Unknown)),
    };
    let row = usize::try_from(row).unwrap_or(usize::MAX);
    if row >= anchors.rows() || mark_class >= class_count {
        return false;
    }
    let found = match anchors.offset(row, mark_class) {
        Some(0) | None => return false,
        Some(_) => anchors.get(row, mark_class),
    };
    let glyph_anchor = match found {
        Ok(Some(a)) => AnchorRef::Known(a),
        Err(ReadError::UnknownFormat(_)) => AnchorRef::Unknown,
        _ => return false,
    };
    apply_mark_attachment(c, mark_anchor, Some(glyph_anchor), glyph_pos);
    true
}

/// `resolve_cross_offset`.
fn resolve_cross_offset(pos: &[Pos], glyph_pos: usize, direction: Direction) -> i32 {
    let horizontal = direction.is_horizontal();
    let cross = |p: &Pos| if horizontal { p.y_offset } else { p.x_offset };
    let len = pos.len();
    let mut g = glyph_pos;
    let Some(p) = pos.get(g) else { return 0 };
    let mut offset = cross(p);
    let mut count = 1usize;
    while count < len {
        let Some(p) = pos.get(g) else { break };
        if p.attach_type & ATTACH_TYPE_CURSIVE == 0 || p.attach_chain == 0 {
            break;
        }
        let parent = isize::try_from(g)
            .unwrap_or(0)
            .wrapping_add(isize::from(p.attach_chain));
        let Ok(parent) = usize::try_from(parent) else { break };
        if parent >= len {
            break;
        }
        g = parent;
        offset = offset.saturating_add(pos.get(g).map_or(0, cross));
        count = count.wrapping_add(1);
    }
    offset
}

/// `apply_mark_attachment`.
fn apply_mark_attachment(
    c: &mut ApplyContext<'_, '_, '_, '_, '_>,
    mark_anchor: Option<AnchorRef<'_>>,
    glyph_anchor: Option<AnchorRef<'_>>,
    glyph_pos: usize,
) {
    let idx = c.buffer.idx;
    c.buffer.unsafe_to_break(glyph_pos, idx.wrapping_add(1));
    let (mark_x, mark_y) = get_anchor(c, mark_anchor);
    let (base_x, base_y) = get_anchor(c, glyph_anchor);
    let direction = c.buffer.direction;
    let len = c.buffer.len;
    let base_offset = resolve_cross_offset(c.buffer.pos.range(0, len), glyph_pos, direction);
    let chain = i64::try_from(glyph_pos)
        .unwrap_or(0)
        .wrapping_sub(i64::try_from(idx).unwrap_or(0));
    let mark = c.buffer.cur_pos_mut();
    match i16::try_from(chain) {
        Ok(ch) => {
            mark.attach_chain = ch;
            mark.attach_type = ATTACH_TYPE_MARK;
            mark.x_offset = round_to_i32(base_x - mark_x);
            mark.y_offset = round_to_i32(base_y - mark_y);
            if direction.is_horizontal() {
                mark.y_offset = mark.y_offset.saturating_add(base_offset);
            } else {
                mark.x_offset = mark.x_offset.saturating_add(base_offset);
            }
            c.buffer.scratch_flags |= scratch::HAS_GPOS_ATTACHMENT;
        }
        Err(_) => mark.attach_chain = 0,
    }
    c.buffer.idx = idx.wrapping_add(1);
}

/// `reverse_cursive_minor_offset`.
fn reverse_cursive_minor_offset(
    pos: &mut [Pos],
    i: usize,
    direction: Direction,
    new_parent: usize,
    nesting: u32,
) {
    if nesting > super::apply::MAX_NESTING_LEVEL {
        return;
    }
    let Some(p) = pos.get(i) else { return };
    let (chain, ty) = (p.attach_chain, p.attach_type);
    if chain == 0 || ty & ATTACH_TYPE_CURSIVE == 0 {
        return;
    }
    if let Some(p) = pos.get_mut(i) {
        p.attach_chain = 0;
    }
    let j = isize::try_from(i).unwrap_or(0).wrapping_add(isize::from(chain));
    let Ok(j) = usize::try_from(j) else { return };
    if j >= pos.len() || j == new_parent {
        return;
    }
    let Some(reversed) = chain.checked_neg() else {
        return;
    };
    reverse_cursive_minor_offset(pos, j, direction, new_parent, nesting.wrapping_add(1));
    let pi = pos.get(i).copied().unwrap_or_default();
    if let Some(pj) = pos.get_mut(j) {
        if direction.is_horizontal() {
            pj.y_offset = pi.y_offset.checked_neg().unwrap_or(i32::MAX);
        } else {
            pj.x_offset = pi.x_offset.checked_neg().unwrap_or(i32::MAX);
        }
        pj.attach_chain = reversed;
        pj.attach_type = ty;
    }
}

/// `apply_cursive_attachment`.
fn apply_cursive(
    c: &mut ApplyContext<'_, '_, '_, '_, '_>,
    exit: AnchorRef<'_>,
    entry: AnchorRef<'_>,
    i: usize,
) {
    let j = c.buffer.idx;
    c.buffer.unsafe_to_break(i, j.wrapping_add(1));
    let (exit_x, exit_y) = get_anchor(c, Some(exit));
    let (entry_x, entry_y) = get_anchor(c, Some(entry));
    let direction = c.direction;
    let rtl_flag = c.lookup_props & u32::from(lookup_flag::RIGHT_TO_LEFT) != 0;
    let len = c.buffer.len;
    let pos = c.buffer.pos.range_mut(0, len);
    let (mut pi, mut pj) = (
        pos.get(i).copied().unwrap_or_default(),
        pos.get(j).copied().unwrap_or_default(),
    );
    match direction {
        Direction::Ltr => {
            pi.x_advance = round_to_i32(exit_x).saturating_add(pi.x_offset);
            let d = round_to_i32(entry_x).saturating_add(pj.x_offset);
            pj.x_advance = pj.x_advance.saturating_sub(d);
            pj.x_offset = pj.x_offset.saturating_sub(d);
        }
        Direction::Rtl => {
            let d = round_to_i32(exit_x).saturating_add(pi.x_offset);
            pi.x_advance = pi.x_advance.saturating_sub(d);
            pi.x_offset = pi.x_offset.saturating_sub(d);
            pj.x_advance = round_to_i32(entry_x).saturating_add(pj.x_offset);
        }
        Direction::Ttb => {
            pi.y_advance = round_to_i32(exit_y).saturating_add(pi.y_offset);
            let d = round_to_i32(entry_y).saturating_add(pj.y_offset);
            pj.y_advance = pj.y_advance.saturating_sub(d);
            pj.y_offset = pj.y_offset.saturating_sub(d);
        }
        Direction::Btt => {
            let d = round_to_i32(exit_y).saturating_add(pi.y_offset);
            pi.y_advance = pi.y_advance.saturating_sub(d);
            pi.y_offset = pi.y_offset.saturating_sub(d);
            pj.y_advance = round_to_i32(entry_y);
        }
    }
    if let Some(p) = pos.get_mut(i) {
        *p = pi;
    }
    if let Some(p) = pos.get_mut(j) {
        *p = pj;
    }

    let (mut child, mut parent) = (i, j);
    let mut x_offset = round_to_i32(entry_x - exit_x);
    let mut y_offset = round_to_i32(entry_y - exit_y);
    if !rtl_flag {
        core::mem::swap(&mut child, &mut parent);
        x_offset = x_offset.checked_neg().unwrap_or(i32::MAX);
        y_offset = y_offset.checked_neg().unwrap_or(i32::MAX);
    }
    reverse_cursive_minor_offset(pos, child, direction, parent, 0);
    let chain = i64::try_from(parent)
        .unwrap_or(0)
        .wrapping_sub(i64::try_from(child).unwrap_or(0));
    if let Ok(ch) = i16::try_from(chain) {
        if let Some(p) = pos.get_mut(child) {
            p.attach_chain = ch;
            p.attach_type = ATTACH_TYPE_CURSIVE;
            if direction.is_horizontal() {
                p.y_offset = y_offset;
            } else {
                p.x_offset = x_offset;
            }
        }
        let parent_chain = pos.get(parent).map_or(0, |p| p.attach_chain);
        if i32::from(parent_chain) == -i32::from(ch)
            && let Some(p) = pos.get_mut(parent)
        {
            p.attach_chain = 0;
            if direction.is_horizontal() {
                p.y_offset = 0;
            } else {
                p.x_offset = 0;
            }
        }
        c.buffer.scratch_flags |= scratch::HAS_GPOS_ATTACHMENT;
    } else if let Some(p) = pos.get_mut(child) {
        p.attach_chain = 0;
    }
    c.buffer.idx = j.wrapping_add(1);
}

/// `GPOS::position_start`.
pub(crate) fn position_start(buffer: &mut Buffer) {
    for p in buffer.positions_mut() {
        p.attach_chain = 0;
        p.attach_type = 0;
    }
}

/// `propagate_attachment_offsets`.
fn propagate_attachment_offsets(pos: &mut [Pos], i: usize, direction: Direction, nesting: u32) {
    let Some(p) = pos.get_mut(i) else { return };
    let (chain, ty) = (p.attach_chain, p.attach_type);
    p.attach_chain = 0;
    let j = isize::try_from(i).unwrap_or(0).wrapping_add(isize::from(chain));
    let Ok(j) = usize::try_from(j) else { return };
    if j >= pos.len() || nesting == 0 {
        return;
    }
    if pos.get(j).is_some_and(|p| p.attach_chain != 0) {
        propagate_attachment_offsets(pos, j, direction, nesting.wrapping_sub(1));
    }
    let pj = pos.get(j).copied().unwrap_or_default();
    let horizontal = direction.is_horizontal();
    let forward = direction.is_forward();
    if ty & ATTACH_TYPE_CURSIVE != 0 {
        if let Some(p) = pos.get_mut(i) {
            if horizontal {
                p.y_offset = p.y_offset.saturating_add(pj.y_offset);
            } else {
                p.x_offset = p.x_offset.saturating_add(pj.x_offset);
            }
        }
        return;
    }
    let (mut x, mut y) = pos
        .get(i)
        .map_or((0, 0), |p| (p.x_offset, p.y_offset));
    if horizontal {
        x = x.saturating_add(pj.x_offset);
    } else {
        y = y.saturating_add(pj.y_offset);
    }
    let adv = |k: usize| pos.get(k).map_or((0, 0), |p| (p.x_advance, p.y_advance));
    if j < i {
        if forward {
            for k in j..i {
                let (ax, ay) = adv(k);
                x = x.saturating_sub(ax);
                y = y.saturating_sub(ay);
            }
        } else {
            for k in j.wrapping_add(1)..i.wrapping_add(1) {
                let (ax, ay) = adv(k);
                x = x.saturating_add(ax);
                y = y.saturating_add(ay);
            }
        }
    } else if forward {
        for k in i..j {
            let (ax, ay) = adv(k);
            x = x.saturating_add(ax);
            y = y.saturating_add(ay);
        }
    } else {
        for k in i.wrapping_add(1)..j.wrapping_add(1) {
            let (ax, ay) = adv(k);
            x = x.saturating_sub(ax);
            y = y.saturating_sub(ay);
        }
    }
    if let Some(p) = pos.get_mut(i) {
        p.x_offset = x;
        p.y_offset = y;
    }
}

/// `GPOS::position_finish_offsets`.
pub(crate) fn position_finish_offsets(buffer: &mut Buffer) {
    if buffer.scratch_flags & scratch::HAS_GPOS_ATTACHMENT == 0 {
        return;
    }
    let direction = buffer.direction;
    let pos = buffer.positions_mut();
    let n = pos.len();
    let nest = super::apply::MAX_NESTING_LEVEL;
    if direction.is_forward() {
        for i in 0..n {
            if pos.get(i).is_some_and(|p| p.attach_chain != 0) {
                propagate_attachment_offsets(pos, i, direction, nest);
            }
        }
    } else {
        for i in (0..n).rev() {
            if pos.get(i).is_some_and(|p| p.attach_chain != 0) {
                propagate_attachment_offsets(pos, i, direction, nest);
            }
        }
    }
}
