//! AAT kerx positioning, from HarfBuzz hb-aat-layout-kerx-table.hh.
use super::{AatLookup, MachineKind, StateTable, be};
use crate::buffer::{Buffer, Mask, scratch};
use crate::layout::gpos::{ATTACH_TYPE_CURSIVE, ATTACH_TYPE_MARK, resolve_cross_offset};
use alloc::vec::Vec;
use sofel_font::HbScale;

#[derive(Clone, Debug)]
pub(crate) struct Subtable<'a> {
    coverage: u32,
    tuples: u32,
    data: &'a [u8],
    action: Kern<'a>,
}
#[derive(Clone, Debug)]
enum Kern<'a> {
    Pairs(&'a [[u8; 6]]),
    Classes(AatLookup<'a>, AatLookup<'a>, &'a [u8]),
    LegacyClasses(ClassOffsets<'a>, ClassOffsets<'a>, usize),
    LegacyHistory(StateTable<'a>, &'a [u8], usize),
    History(StateTable<'a>, &'a [u8]),
    Attachment(StateTable<'a>, u32, &'a [u8], Option<Anchors<'a>>),
}

#[derive(Clone, Debug)]
struct Anchors<'a> {
    glyphs: AatLookup<'a>,
    coordinates: &'a [u8],
}
impl<'a> Anchors<'a> {
    fn read(data: &'a [u8], count: u32) -> Option<Self> {
        if be::u16_at(data, 0)? != 0 {
            return None;
        }
        let offset = be::u32_at(data, 8)? as usize;
        Some(Self {
            glyphs: AatLookup::new(data.get(be::u32_at(data, 4)? as usize..offset)?, count)?,
            coordinates: data.get(offset..)?,
        })
    }
    fn get(&self, glyph: u32, index: u16) -> Option<(i32, i32)> {
        let list = self
            .coordinates
            .get(usize::from(self.glyphs.get(glyph)?)..)?;
        if u32::from(index) >= be::u32_at(list, 0)? {
            return None;
        }
        point(list, (usize::from(index) * 4).checked_add(4)?)
    }
}

fn point(data: &[u8], at: usize) -> Option<(i32, i32)> {
    Some((
        i32::from(be::i16_at(data, at)?),
        i32::from(be::i16_at(data, at.checked_add(2)?)?),
    ))
}

pub(crate) fn read<'a>(
    data: &'a [u8],
    ankr: Option<&'a [u8]>,
    glyphs: u32,
) -> Option<Vec<Subtable<'a>>> {
    if be::u16_at(data, 0)? < 2 {
        return None;
    }
    let n = be::u32_at(data, 4)?;
    if n == 0 || n > 1024 {
        return None;
    }
    let mut tail = data.get(8..)?;
    let mut result = Vec::new();
    for i in 0..n {
        let length = be::u32_at(tail, 0)? as usize;
        let current = if i.saturating_add(1) == n {
            tail
        } else {
            tail.get(..length)?
        };
        let coverage = be::u32_at(current, 4)?;
        let tuples = be::u32_at(current, 8)?;
        let offset = |at| current.get(be::u32_at(current, at)? as usize..);
        let action = match coverage & 255 {
            0 => Kern::Pairs(be::chunks_at(
                current,
                28,
                be::u32_at(current, 12)? as usize,
            )?),
            1 => {
                let state = current.get(12..)?;
                Kern::History(
                    StateTable::from_kind(state, glyphs, MachineKind::Kerning)?,
                    state.get(be::u32_at(state, 16)? as usize..)?,
                )
            }
            2 => Kern::Classes(
                AatLookup::new(offset(16)?, glyphs)?,
                AatLookup::new(offset(20)?, glyphs)?,
                offset(24)?,
            ),
            4 => {
                let state = current.get(12..)?;
                let flags = be::u32_at(state, 16)?;
                Kern::Attachment(
                    StateTable::from_kind(state, glyphs, MachineKind::Kerning)?,
                    flags >> 30,
                    state.get((flags & 0x00ff_ffff) as usize..)?,
                    ankr.and_then(|s| Anchors::read(s, glyphs)),
                )
            }
            _ => return None,
        };
        result.push(Subtable {
            coverage,
            tuples,
            data: current,
            action,
        });
        if i.saturating_add(1) < n {
            tail = tail.get(length..)?;
        }
    }
    Some(result)
}
#[derive(Clone, Debug)]
struct ClassOffsets<'a> {
    first: u16,
    offsets: &'a [[u8; 2]],
}
impl<'a> ClassOffsets<'a> {
    fn read(data: &'a [u8], at: usize) -> Option<Self> {
        let first = be::u16_at(data, at)?;
        let count = usize::from(be::u16_at(data, at.checked_add(2)?)?);
        Some(Self {
            first,
            offsets: be::chunks_at(data, at.checked_add(4)?, count)?,
        })
    }
    fn offset(&self, glyph: u32) -> Option<usize> {
        let i = glyph.checked_sub(u32::from(self.first))?;
        Some(usize::from(u16::from_be_bytes(
            *self.offsets.get(i as usize)?,
        )))
    }
}

/// Legacy OT and Apple kern headers use distinct coverage bits and 16-bit class offsets.
pub(crate) fn legacy(data: &[u8]) -> Option<Vec<Subtable<'_>>> {
    let (apple, count, mut tail) = match be::u16_at(data, 0)? {
        0 => (false, u32::from(be::u16_at(data, 2)?), data.get(4..)?),
        1 if be::u32_at(data, 0)? == 0x10000 => (true, be::u32_at(data, 4)?, data.get(8..)?),
        _ => return None,
    };
    let header = if apple { 8 } else { 6 };
    let mut tables = Vec::new();
    for i in 0..count {
        let (len, format, vertical, cross, variation) = if apple {
            let flags = be::u8_at(tail, 4)?;
            (
                be::u32_at(tail, 0)? as usize,
                be::u8_at(tail, 5)?,
                flags & 128 != 0,
                flags & 64 != 0,
                flags & 32 != 0,
            )
        } else {
            let flags = be::u8_at(tail, 5)?;
            (
                usize::from(be::u16_at(tail, 2)?),
                be::u8_at(tail, 4)?,
                flags & 1 == 0,
                flags & 4 != 0,
                false,
            )
        };
        if len < header {
            return None;
        }
        let (declared, next) = tail.split_at_checked(len)?;
        let current = if i.saturating_add(1) == count {
            tail
        } else {
            declared
        };
        tail = next;
        if variation {
            continue;
        }
        let body = current.get(header..)?;
        let action = match format {
            0 => Kern::Pairs(be::chunks_at(body, 8, usize::from(be::u16_at(body, 0)?))?),
            1 => {
                let base = usize::from(be::u16_at(body, 8)?);
                Kern::LegacyHistory(
                    StateTable::legacy(body, MachineKind::Kerning)?,
                    body.get(base..)?,
                    base,
                )
            }
            2 => {
                let offset = |at| be::u16_at(body, at).map(usize::from);
                let array = offset(6)?;
                current.get(array..)?;
                Kern::LegacyClasses(
                    ClassOffsets::read(current, offset(2)?)?,
                    ClassOffsets::read(current, offset(4)?)?,
                    array,
                )
            }
            _ => continue,
        };
        tables.push(Subtable {
            coverage: (u32::from(vertical) << 31) | (u32::from(cross) << 30),
            tuples: 0,
            data: current,
            action,
        });
    }
    (!tables.is_empty()).then_some(tables)
}

pub(crate) fn has_machine(tables: &[Subtable<'_>]) -> bool {
    tables
        .iter()
        .any(|t| matches!(t.action, Kern::LegacyHistory(..)))
}

pub(crate) fn cross_stream(tables: &[Subtable<'_>]) -> bool {
    tables.iter().any(|t| t.coverage & 0x4000_0000 != 0)
}
pub(crate) fn apply(tables: &[Subtable<'_>], buf: &mut Buffer, scale: &HbScale, mask: Mask) {
    buf.unsafe_to_concat(0, buf.len);
    let horizontal = buf.direction.is_horizontal();
    let mut attached = false;
    for t in tables {
        if horizontal != (t.coverage & 0x8000_0000 == 0) {
            continue;
        }
        buf.charge(buf.len);
        if buf.max_ops < 0 {
            buf.successful = false;
            return;
        }
        let cross = t.coverage & 0x4000_0000 != 0;
        let backwards = t.coverage & 0x1000_0000 != 0;
        if backwards && !matches!(t.action, Kern::History(..) | Kern::Attachment(..)) {
            continue;
        }
        if cross && !attached {
            let chain = if buf.direction.is_forward() { -1 } else { 1 };
            for p in buf.positions_mut() {
                p.attach_type = ATTACH_TYPE_CURSIVE;
                p.attach_chain = chain;
            }
            attached = true;
        }
        let reverse = backwards != buf.direction.is_backward();
        if reverse {
            buf.reverse();
        }
        if let Kern::History(machine, actions) = &t.action {
            history(
                machine,
                actions,
                t.tuples.max(1) as usize,
                buf,
                scale,
                mask,
                cross,
            );
        } else if let Kern::LegacyHistory(machine, actions, base) = &t.action {
            legacy_history(machine, actions, *base, buf, scale, mask, cross);
        } else if let Kern::Attachment(machine, kind, actions, anchors) = &t.action {
            let mut marked = None;
            walk(machine, buf, |b, idx, e| {
                if let Some(mark) = marked
                    && idx < b.len
                    && e.substitutions[0] != 0xffff
                {
                    let n = usize::from(e.substitutions[0]);
                    let coords = match kind {
                        2 => point(actions, n * 8).zip(point(actions, (n * 8).saturating_add(4))),
                        1 => {
                            let anchor = |slot: usize, offset| {
                                let glyph = b.infos().get(slot)?.codepoint;
                                let i = be::u16_at(actions, offset)?;
                                Some(
                                    anchors
                                        .as_ref()
                                        .and_then(|a| a.get(glyph, i))
                                        .unwrap_or_default(),
                                )
                            };
                            anchor(mark, n * 4).zip(anchor(idx, (n * 4).saturating_add(2)))
                        }
                        _ => None,
                    };
                    if let Some(((mx, my), (cx, cy))) = coords {
                        let dir = b.direction;
                        let cross = resolve_cross_offset(b.positions_mut(), mark, dir, reverse);
                        let chain = if reverse {
                            idx.checked_signed_diff(mark)
                        } else {
                            mark.checked_signed_diff(idx)
                        };
                        if let Some(p) = b.positions_mut().get_mut(idx) {
                            p.x_offset = scale.em_scale_x(mx).saturating_sub(scale.em_scale_x(cx));
                            p.y_offset = scale.em_scale_y(my).saturating_sub(scale.em_scale_y(cy));
                            if horizontal {
                                p.y_offset = p.y_offset.saturating_add(cross);
                            } else {
                                p.x_offset = p.x_offset.saturating_add(cross);
                            }
                            p.attach_type = ATTACH_TYPE_MARK;
                            p.attach_chain = chain.and_then(|d| i16::try_from(d).ok()).unwrap_or(0);
                            b.scratch_flags |= scratch::HAS_GPOS_ATTACHMENT;
                        }
                    }
                }
                if e.flags & 0x8000 != 0 {
                    marked = Some(idx);
                }
            });
        } else if mask != 0 {
            let mut prior = None;
            for j in 0..buf.len {
                let Some(info) = buf.infos().get(j) else {
                    break;
                };
                if info.is_mark() {
                    continue;
                }
                if info.mask & mask == 0 {
                    prior = None;
                    continue;
                }
                if let Some((i, left)) = prior {
                    let pair = t.value(left, info.codepoint);
                    let v = if horizontal {
                        scale.em_scale_x(pair)
                    } else {
                        scale.em_scale_y(pair)
                    };
                    if v != 0 {
                        if let Some(p) = buf.positions_mut().get_mut(i) {
                            let p: &mut crate::buffer::Pos = p;
                            if !cross {
                                if horizontal {
                                    p.x_advance = p.x_advance.saturating_add(v >> 1);
                                } else {
                                    p.y_advance = p.y_advance.saturating_add(v >> 1);
                                }
                            }
                        }
                        if let Some(p) = buf.positions_mut().get_mut(j) {
                            let half = v.saturating_sub(v >> 1);
                            if cross {
                                if horizontal {
                                    p.y_offset = v;
                                } else {
                                    p.x_offset = v;
                                }
                                buf.scratch_flags |= scratch::HAS_GPOS_ATTACHMENT;
                            } else if horizontal {
                                p.x_advance = p.x_advance.saturating_add(half);
                                p.x_offset = p.x_offset.saturating_add(half);
                            } else {
                                p.y_advance = p.y_advance.saturating_add(half);
                                p.y_offset = p.y_offset.saturating_add(half);
                            }
                        }
                        buf.unsafe_to_break(i, j.saturating_add(1));
                    }
                }
                prior = buf.infos().get(j).map(|info| (j, info.codepoint));
            }
        }
        if reverse {
            buf.reverse();
        }
    }
}
impl Subtable<'_> {
    fn value(&self, l: u32, r: u32) -> i32 {
        let value = match self.action {
            Kern::Pairs(pairs) => pairs
                .binary_search_by(|row| {
                    let [a, b, c, d, _, _] = *row;
                    (
                        u32::from(u16::from_be_bytes([a, b])),
                        u32::from(u16::from_be_bytes([c, d])),
                    )
                        .cmp(&(l, r))
                })
                .ok()
                .and_then(|i| be::i16_at(pairs.get(i)?, 4)),
            Kern::Classes(left, right, values) => {
                left.get(l).zip(right.get(r)).and_then(|(a, b)| {
                    be::i16_at(values, usize::from(a).saturating_add(usize::from(b)))
                })
            }
            Kern::LegacyClasses(ref left, ref right, array) => {
                left.offset(l).zip(right.offset(r)).and_then(|(a, b)| {
                    let offset = a.checked_add(b)?.checked_sub(array)?;
                    be::i16_at(self.data.get(array..)?, offset & !1)
                })
            }
            Kern::History(..) | Kern::LegacyHistory(..) | Kern::Attachment(..) => None,
        };
        value
            .and_then(|v| {
                if self.tuples == 0 {
                    Some(v)
                } else {
                    let offset = usize::try_from(v).ok()?;
                    let entries = be::chunks_at::<2>(self.data, offset, self.tuples as usize)?;
                    entries.first().map(|x| i16::from_be_bytes(*x))
                }
            })
            .map_or(0, i32::from)
    }
}

fn history(
    machine: &StateTable<'_>,
    actions: &[u8],
    stride: usize,
    b: &mut Buffer,
    scale: &HbScale,
    mask: Mask,
    cross: bool,
) {
    let mut positions = [0usize; 8];
    let mut cursor = 0usize;
    let mut pushed = false;
    walk(machine, b, |b, idx, e| {
        if e.flags & 0x8000 != 0 {
            if let Some(p) = positions.get_mut(cursor) {
                *p = idx;
            }
            cursor = cursor.saturating_add(1) % 8;
            pushed = true;
        }
        if e.substitutions[0] != 0xffff && pushed {
            let mut head = cursor;
            let mut action = usize::from(e.substitutions[0]);
            loop {
                b.charge(1);
                if b.max_ops < 0 {
                    b.successful = false;
                    break;
                }
                let Some(v) = be::i16_at(actions, action.saturating_mul(2)) else {
                    pushed = false;
                    break;
                };
                if v == -1 {
                    break;
                }
                head = head.wrapping_sub(1) & 7;
                let at = positions.get(head).copied().unwrap_or(0);
                action = action.saturating_add(stride);
                if at >= b.len {
                    continue;
                }
                b.unsafe_to_break(at, idx.saturating_add(1));
                apply_delta(b, at, v, scale, mask, cross);
            }
        }
    });
}

fn apply_delta(b: &mut Buffer, at: usize, v: i16, scale: &HbScale, mask: Mask, cross: bool) {
    let horizontal = b.direction.is_horizontal();
    let enabled = b.infos().get(at).is_some_and(|g| g.mask & mask != 0);
    if let Some(p) = b.positions_mut().get_mut(at) {
        let delta = i32::from(v);
        if cross {
            if v == i16::MIN {
                p.attach_type = 0;
                p.attach_chain = 0;
                if horizontal {
                    p.y_offset = 0;
                } else {
                    p.x_offset = 0;
                }
            } else if p.attach_type != 0 {
                if horizontal {
                    p.y_offset = p.y_offset.saturating_add(scale.em_scale_y(delta));
                } else {
                    p.x_offset = p.x_offset.saturating_add(scale.em_scale_x(delta));
                }
                b.scratch_flags |= scratch::HAS_GPOS_ATTACHMENT;
            }
        } else if enabled && horizontal {
            let v = scale.em_scale_x(delta);
            p.x_advance = p.x_advance.saturating_add(v);
            p.x_offset = p.x_offset.saturating_add(v);
        } else if enabled {
            let v = scale.em_scale_y(delta);
            p.y_advance = p.y_advance.saturating_add(v);
            p.y_offset = p.y_offset.saturating_add(v);
        }
    }
}

fn legacy_history(
    machine: &StateTable<'_>,
    actions: &[u8],
    base: usize,
    b: &mut Buffer,
    scale: &HbScale,
    mask: Mask,
    cross: bool,
) {
    let mut stack = [0usize; 8];
    let mut depth = 0usize;
    walk(machine, b, |b, idx, e| {
        if e.flags & 0x8000 != 0 {
            if let Some(slot) = stack.get_mut(depth) {
                *slot = idx;
                depth = depth.saturating_add(1);
            } else {
                depth = 0;
            }
        }
        let raw = e.substitutions[0];
        if raw == 0xffff || depth == 0 {
            return;
        }
        let words = usize::from(raw)
            .checked_sub(base)
            .and_then(|offset| be::chunks_at::<2>(actions, offset & !1, depth));
        let Some(words) = words else {
            depth = 0;
            return;
        };
        for word in words {
            depth = depth.saturating_sub(1);
            let Some(&at) = stack.get(depth) else {
                break;
            };
            if at < b.len {
                let value = i16::from_be_bytes(*word);
                apply_delta(b, at, value & !1, scale, mask, cross);
                if value & 1 != 0 {
                    break;
                }
            }
        }
    });
}

fn walk(
    machine: &StateTable<'_>,
    b: &mut Buffer,
    mut transition: impl FnMut(&mut Buffer, usize, super::Entry),
) {
    let mut state = usize::from(machine.start_state());
    let mut idx = 0usize;
    while idx <= b.len && b.successful {
        let class = machine.class(b.infos().get(idx));
        let Some(e) = machine.entry(state, class) else {
            break;
        };
        let hold = e.flags & 0x4000 != 0;
        let safe = machine.safe_boundary(state, class, e);
        if !safe && idx > 0 && idx < b.len {
            b.unsafe_to_break(idx.saturating_sub(1), idx.saturating_add(1));
        }
        transition(b, idx, e);
        if idx == b.len {
            break;
        }
        state = usize::from(e.next);
        if hold {
            b.charge(1);
        }
        if !hold || b.max_ops < 0 {
            idx = idx.saturating_add(1);
        }
    }
}
