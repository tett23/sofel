//! The cmap validator.
//!
//! What FreeType rejects at `FT_VALIDATE_DEFAULT` is an `Error` here;
//! what it rejects only at `TIGHT` or `PARANOID` is a `Warning`
//! (FreeType 2.15.0, `src/sfnt/ttcmap.c`). The work done is bounded by a
//! budget proportional to the size of the table.

use crate::be::{self, to_u32, to_usize};
use crate::cmap::TAG;
use crate::cmap::formats::{F0_LEN, F2_KEYS, F2_SUBS, F8_GROUPS, f2_num_subs, f4_layout};
use crate::{Diagnostic, Kind};

struct V<'r> {
    report: &'r mut dyn FnMut(Diagnostic),
    num_glyphs: Option<u32>,
    /// Remaining element visits for checks that loop over data.
    budget: usize,
}

impl V<'_> {
    fn emit(&mut self, at: usize, kind: Kind) {
        (self.report)(Diagnostic::new(TAG, to_u32(at), kind));
    }

    fn truncated(&mut self, at: usize, needed: usize, available: usize) {
        self.emit(
            at,
            Kind::Truncated {
                needed: to_u32(needed),
                available: to_u32(available),
            },
        );
    }

    /// Spends one unit of the budget; `false` once it is used up.
    fn spend(&mut self) -> bool {
        match self.budget.checked_sub(1) {
            Some(b) => {
                self.budget = b;
                true
            }
            None => false,
        }
    }

    /// Whether `glyph` is at or beyond numGlyphs (glyph 0 never is).
    fn out_of_range(&self, glyph: u32) -> Option<u32> {
        let n = self.num_glyphs?;
        (glyph != 0 && glyph >= n).then_some(n)
    }

    fn check_glyph(&mut self, at: usize, glyph: u32) -> bool {
        match self.out_of_range(glyph) {
            Some(num_glyphs) => {
                self.emit(at, Kind::GlyphIdOutOfRange { glyph, num_glyphs });
                true
            }
            None => false,
        }
    }
}

/// Validates a `cmap` table. With `num_glyphs`, glyph ids are also
/// checked against it.
pub fn validate(data: &[u8], num_glyphs: Option<u32>, report: &mut dyn FnMut(Diagnostic)) {
    let budget = data.len().saturating_mul(4).saturating_add(1024);
    let mut v = V {
        report,
        num_glyphs,
        budget,
    };
    let Some(declared) = be::u16_at(data, 2) else {
        return v.truncated(0, 4, data.len());
    };
    let declared = usize::from(declared);
    let whole = data.len().saturating_sub(4) / 8;
    if whole < declared {
        v.truncated(
            whole.saturating_mul(8).saturating_add(4),
            declared.saturating_mul(8).saturating_add(4),
            data.len(),
        );
    }
    let mut previous: Option<(u16, u16)> = None;
    for i in 0..whole.min(declared) {
        let at = i.saturating_mul(8).saturating_add(4);
        let (Some(p), Some(e), Some(offset)) = (
            be::u16_at(data, at),
            be::u16_at(data, at.saturating_add(2)),
            be::u32_at(data, at.saturating_add(4)),
        ) else {
            return;
        };
        if previous.is_some_and(|prev| (p, e) < prev) {
            v.emit(at, Kind::RecordsNotSorted { index: to_u32(i) });
        }
        previous = Some((p, e));
        let off = to_usize(offset);
        if off == 0 || off > data.len().saturating_sub(2) {
            v.emit(
                at.saturating_add(4),
                Kind::OffsetOutOfBounds {
                    offset,
                    length: 2,
                    available: to_u32(data.len()),
                },
            );
            continue;
        }
        subtable(&mut v, data, off);
    }
}

/// Validates only the subtable at `offset` in the `cmap` table `data`,
/// as [`validate`] does for each encoding record. An offset that is 0 or
/// leaves less than 2 bytes is reported at offset 0.
pub fn validate_subtable(
    data: &[u8],
    offset: u32,
    num_glyphs: Option<u32>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let budget = data.len().saturating_mul(4).saturating_add(1024);
    let mut v = V {
        report,
        num_glyphs,
        budget,
    };
    let off = to_usize(offset);
    if off == 0 || off > data.len().saturating_sub(2) {
        return v.emit(
            0,
            Kind::OffsetOutOfBounds {
                offset,
                length: 2,
                available: to_u32(data.len()),
            },
        );
    }
    subtable(&mut v, data, off);
}

fn subtable(v: &mut V<'_>, cmap: &[u8], off: usize) {
    let Some(sub) = be::tail_at(cmap, off) else {
        return;
    };
    let Some(format) = be::u16_at(sub, 0) else {
        return v.truncated(off, 2, sub.len());
    };
    match format {
        0 => format0(v, sub, off),
        2 => format2(v, sub, off),
        4 => format4(v, sub, off),
        6 => format6(v, sub, off),
        8 => format8(v, sub, off),
        10 => format10(v, sub, off),
        12 => groups(v, sub, off, false),
        13 => groups(v, sub, off, true),
        14 => uvs(v, sub, off, false),
        15 => uvs(v, sub, off, true),
        other => v.emit(
            off,
            Kind::UnknownFormat {
                found: u32::from(other),
            },
        ),
    }
}

fn add_delta(g: u32, delta: i16) -> u32 {
    g.wrapping_add_signed(i32::from(delta)) & 0xFFFF
}

fn format0(v: &mut V<'_>, sub: &[u8], off: usize) {
    let avail = sub.len();
    let Some(length) = be::u16_at(sub, 2).map(usize::from) else {
        return v.truncated(off, 4, avail);
    };
    if length < F0_LEN || length > avail {
        return v.truncated(off, length.max(F0_LEN), avail);
    }
    for i in 0..256usize {
        let at = i.saturating_add(6);
        if let Some(g) = be::u8_at(sub, at) {
            v.check_glyph(off.saturating_add(at), u32::from(g));
        }
    }
}

fn format2(v: &mut V<'_>, sub: &[u8], off: usize) {
    let avail = sub.len();
    let Some(length) = be::u16_at(sub, 2).map(usize::from) else {
        return v.truncated(off, 4, avail);
    };
    if length < F2_SUBS || length > avail {
        return v.truncated(off, length.max(F2_SUBS), avail);
    }
    let Some(keys) = be::chunks_at::<2>(sub, F2_KEYS, 256) else {
        return;
    };
    for (i, k) in keys.iter().enumerate() {
        let k = u16::from_be_bytes(*k);
        if k & 7 != 0 {
            let at = off
                .saturating_add(F2_KEYS)
                .saturating_add(i.saturating_mul(2));
            v.emit(
                at,
                Kind::SubHeaderKeyNotAligned {
                    index: to_u32(i),
                    found: u32::from(k),
                },
            );
        }
    }
    let nsubs = f2_num_subs(keys);
    let glyph_ids = nsubs.saturating_mul(8).saturating_add(F2_SUBS);
    if glyph_ids > avail {
        return v.truncated(off.saturating_add(F2_SUBS), glyph_ids, avail);
    }
    for i in 0..nsubs {
        let pos = i.saturating_mul(8).saturating_add(F2_SUBS);
        let (Some(first), Some(count), Some(delta), Some(ro)) = (
            be::u16_at(sub, pos).map(usize::from),
            be::u16_at(sub, pos.saturating_add(2)).map(usize::from),
            be::i16_at(sub, pos.saturating_add(4)),
            be::u16_at(sub, pos.saturating_add(6)).map(usize::from),
        ) else {
            return;
        };
        if count == 0 {
            continue;
        }
        if first >= 256 || count > 256usize.saturating_sub(first) {
            v.emit(
                off.saturating_add(pos),
                Kind::SubHeaderRangeInvalid { index: to_u32(i) },
            );
        }
        if ro == 0 {
            continue;
        }
        let ids = pos.saturating_add(6).saturating_add(ro);
        if ids < glyph_ids || ids.saturating_add(count.saturating_mul(2)) > length {
            v.emit(
                off.saturating_add(pos).saturating_add(6),
                Kind::RangeOffsetOutOfBounds { index: to_u32(i) },
            );
            continue;
        }
        for k in 0..count {
            if !v.spend() {
                break;
            }
            let at = ids.saturating_add(k.saturating_mul(2));
            let Some(g) = be::u16_at(sub, at) else { break };
            if g != 0 && v.check_glyph(off.saturating_add(at), add_delta(u32::from(g), delta)) {
                break;
            }
        }
    }
}

/// Whether some glyph `(c + delta) mod 65536` for `c` in `start..=end`
/// is non-zero and not below `num_glyphs`; the first such glyph.
fn delta_out_of_range(start: u32, end: u32, delta: i16, num_glyphs: u32) -> Option<u32> {
    let limit = num_glyphs.max(1);
    let la = add_delta(start, delta);
    let span = end.checked_sub(start)?;
    let lb = la.checked_add(span)?;
    if lb < 0x1_0000 {
        (lb >= limit).then(|| la.max(limit))
    } else {
        // Wraps: covers la..=0xFFFF and 0..=(lb - 0x10000).
        (0xFFFF >= limit).then(|| la.max(limit))
    }
}

fn format4(v: &mut V<'_>, sub: &[u8], off: usize) {
    let avail = sub.len();
    let Some(length) = be::u16_at(sub, 2).map(usize::from) else {
        return v.truncated(off, 4, avail);
    };
    if length > avail {
        v.emit(
            off.saturating_add(2),
            Kind::LengthMismatch {
                declared: to_u32(length),
                available: to_u32(avail),
            },
        );
    }
    let Some(seg_x2) = be::u16_at(sub, 6) else {
        return v.truncated(off, 16, avail);
    };
    let l = f4_layout(seg_x2);
    if avail < 16 || avail < l.glyph_ids {
        return v.truncated(off, l.glyph_ids.max(16), avail);
    }
    let n = l.segs;
    if seg_x2 & 1 != 0 {
        v.emit(
            off.saturating_add(6),
            Kind::SegCountOdd {
                found: u32::from(seg_x2),
            },
        );
    }
    if n > 0 {
        let (Some(sr), Some(es), Some(rs)) =
            (be::u16_at(sub, 8), be::u16_at(sub, 10), be::u16_at(sub, 12))
        else {
            return;
        };
        let (sr2, rs2) = (usize::from(sr / 2), usize::from(rs / 2));
        let ok = (sr | rs) & 1 == 0
            && sr2 <= n
            && sr2.saturating_mul(2) >= n
            && sr2.saturating_add(rs2) == n
            && 1usize.checked_shl(u32::from(es)) == Some(sr2);
        if !ok {
            v.emit(off.saturating_add(8), Kind::SearchParametersInvalid);
        }
        let last_end_at = l.ends.saturating_add(n.saturating_sub(1).saturating_mul(2));
        if let Some(e) = be::u16_at(sub, last_end_at)
            && e != 0xFFFF
        {
            v.emit(
                off.saturating_add(last_end_at),
                Kind::LastSegmentNotFfff {
                    found: u32::from(e),
                },
            );
        }
    }
    let (mut last_start, mut last_end) = (0u32, 0u32);
    for i in 0..n {
        let i2 = i.saturating_mul(2);
        let start_at = l.starts.saturating_add(i2);
        let delta_at = l.deltas.saturating_add(i2);
        let ro_at = l.range_offsets.saturating_add(i2);
        let (Some(start), Some(end), Some(delta), Some(ro)) = (
            be::u16_at(sub, start_at).map(u32::from),
            be::u16_at(sub, l.ends.saturating_add(i2)).map(u32::from),
            be::i16_at(sub, delta_at),
            be::u16_at(sub, ro_at).map(usize::from),
        ) else {
            return;
        };
        if start > end {
            v.emit(
                off.saturating_add(start_at),
                Kind::StartAfterEnd {
                    index: to_u32(i),
                    start,
                    end,
                },
            );
        } else if i > 0 && start <= last_end {
            let kind = if last_start > start || last_end > end {
                Kind::SegmentsNotSorted { index: to_u32(i) }
            } else {
                Kind::SegmentsOverlap { index: to_u32(i) }
            };
            v.emit(off.saturating_add(start_at), kind);
        }
        last_start = start;
        last_end = end;
        if start > end {
            continue;
        }
        let final_ffff = i.saturating_add(1) == n && start == 0xFFFF && end == 0xFFFF;
        let count = to_usize(end.saturating_sub(start)).saturating_add(1);
        if ro == 0xFFFF {
            if !final_ffff {
                v.emit(
                    off.saturating_add(ro_at),
                    Kind::RangeOffsetFfff { index: to_u32(i) },
                );
            }
        } else if ro != 0 {
            let p = ro_at.saturating_add(ro);
            if final_ffff {
                continue;
            }
            if p < l.glyph_ids || p.saturating_add(count.saturating_mul(2)) > avail {
                v.emit(
                    off.saturating_add(ro_at),
                    Kind::RangeOffsetOutOfBounds { index: to_u32(i) },
                );
                continue;
            }
            for k in 0..count {
                if !v.spend() {
                    break;
                }
                let at = p.saturating_add(k.saturating_mul(2));
                let Some(g) = be::u16_at(sub, at) else { break };
                if g != 0 && v.check_glyph(off.saturating_add(at), add_delta(u32::from(g), delta)) {
                    break;
                }
            }
        } else if let Some(n_glyphs) = v.num_glyphs
            && let Some(glyph) = delta_out_of_range(start, end, delta, n_glyphs)
        {
            v.emit(
                off.saturating_add(delta_at),
                Kind::GlyphIdOutOfRange {
                    glyph,
                    num_glyphs: n_glyphs,
                },
            );
        }
    }
}

fn format6(v: &mut V<'_>, sub: &[u8], off: usize) {
    let avail = sub.len();
    let (Some(length), Some(count)) = (
        be::u16_at(sub, 2).map(usize::from),
        be::u16_at(sub, 8).map(usize::from),
    ) else {
        return v.truncated(off, 10, avail);
    };
    let need = count.saturating_mul(2).saturating_add(10);
    if length > avail || length < need {
        return v.truncated(off, length.max(need), avail);
    }
    for k in 0..count {
        let at = k.saturating_mul(2).saturating_add(10);
        if let Some(g) = be::u16_at(sub, at)
            && v.check_glyph(off.saturating_add(at), u32::from(g))
        {
            break;
        }
    }
}

fn format10(v: &mut V<'_>, sub: &[u8], off: usize) {
    let avail = sub.len();
    let (Some(length), Some(count)) = (
        be::u32_at(sub, 4).map(to_usize),
        be::u32_at(sub, 16).map(to_usize),
    ) else {
        return v.truncated(off, 20, avail);
    };
    let need = count.saturating_mul(2).saturating_add(20);
    if length > avail || length < need {
        return v.truncated(off, length.max(need), avail);
    }
    for k in 0..count {
        let at = k.saturating_mul(2).saturating_add(20);
        if let Some(g) = be::u16_at(sub, at)
            && v.check_glyph(off.saturating_add(at), u32::from(g))
        {
            break;
        }
    }
}

/// Checks `(start, end, glyph)` groups for order and, if `many_to_one`
/// is false, for glyph ranges; formats 8, 12 and 13.
fn check_groups(
    v: &mut V<'_>,
    sub: &[u8],
    off: usize,
    base: usize,
    n: usize,
    many_to_one: bool,
    is32: Option<&[u8]>,
) {
    let mut last = 0u32;
    for i in 0..n {
        let at = i.saturating_mul(12).saturating_add(base);
        let (Some(start), Some(end), Some(id)) = (
            be::u32_at(sub, at),
            be::u32_at(sub, at.saturating_add(4)),
            be::u32_at(sub, at.saturating_add(8)),
        ) else {
            return;
        };
        let here = off.saturating_add(at);
        if start > end {
            v.emit(
                here,
                Kind::StartAfterEnd {
                    index: to_u32(i),
                    start,
                    end,
                },
            );
        } else if i > 0 && start <= last {
            v.emit(here, Kind::GroupsNotSorted { index: to_u32(i) });
        }
        last = end;
        if start > end {
            continue;
        }
        if let Some(num_glyphs) = v.num_glyphs {
            let d = end.saturating_sub(start);
            let bad = if many_to_one {
                id >= num_glyphs && id != 0
            } else {
                d > num_glyphs || id >= num_glyphs.saturating_sub(d)
            };
            if bad {
                let glyph = if many_to_one {
                    id
                } else {
                    id.saturating_add(d)
                };
                v.emit(here, Kind::GlyphIdOutOfRange { glyph, num_glyphs });
            }
        }
        if let Some(bits) = is32 {
            let set = |x: u32| {
                be::u8_at(bits, to_usize(x >> 3)).is_some_and(|b| b & (0x80u8 >> (x & 7)) != 0)
            };
            // Checked at the ends of the group only, to stay bounded.
            let wrong = [start, end].into_iter().find(|&c| {
                if c > 0xFFFF {
                    !(set(c >> 16) && set(c & 0xFFFF))
                } else {
                    set(c) || end > 0xFFFF
                }
            });
            if let Some(codepoint) = wrong {
                v.emit(here, Kind::Is32Mismatch { codepoint });
            }
        }
    }
}

fn format8(v: &mut V<'_>, sub: &[u8], off: usize) {
    let avail = sub.len();
    if avail < F8_GROUPS {
        return v.truncated(off, F8_GROUPS, avail);
    }
    let (Some(length), Some(n)) = (
        be::u32_at(sub, 4).map(to_usize),
        be::u32_at(sub, F8_GROUPS.saturating_sub(4)).map(to_usize),
    ) else {
        return;
    };
    if length > avail || length < F8_GROUPS {
        return v.truncated(off, length.max(F8_GROUPS), avail);
    }
    if n > avail.saturating_sub(F8_GROUPS) / 12 {
        return v.truncated(off, n.saturating_mul(12).saturating_add(F8_GROUPS), avail);
    }
    let is32 = be::slice_at(sub, 12, 8192);
    check_groups(v, sub, off, F8_GROUPS, n, false, is32);
}

fn groups(v: &mut V<'_>, sub: &[u8], off: usize, many_to_one: bool) {
    let avail = sub.len();
    let (Some(length), Some(n)) = (
        be::u32_at(sub, 4).map(to_usize),
        be::u32_at(sub, 12).map(to_usize),
    ) else {
        return v.truncated(off, 16, avail);
    };
    let need = n.saturating_mul(12).saturating_add(16);
    if length > avail || length < need {
        return v.truncated(off, length.max(need), avail);
    }
    check_groups(v, sub, off, 16, n, many_to_one, None);
}

fn uvs(v: &mut V<'_>, sub: &[u8], off: usize, wide: bool) {
    let avail = sub.len();
    let (Some(length), Some(n)) = (
        be::u32_at(sub, 2).map(to_usize),
        be::u32_at(sub, 6).map(to_usize),
    ) else {
        return v.truncated(off, 10, avail);
    };
    let need = n.saturating_mul(11).saturating_add(10);
    if length > avail || length < need {
        return v.truncated(off, length.max(need), avail);
    }
    let mut last_selector = 1u32;
    for i in 0..n {
        let at = i.saturating_mul(11).saturating_add(10);
        let (Some(selector), Some(default), Some(non_default)) = (
            be::u24_at(sub, at),
            be::u32_at(sub, at.saturating_add(3)),
            be::u32_at(sub, at.saturating_add(7)),
        ) else {
            return;
        };
        let here = off.saturating_add(at);
        let mut bounds_ok = true;
        for o in [default, non_default] {
            if to_usize(o) >= length {
                v.emit(
                    here,
                    Kind::OffsetOutOfBounds {
                        offset: o,
                        length: 4,
                        available: to_u32(length),
                    },
                );
                bounds_ok = false;
            }
        }
        if selector < last_selector {
            v.emit(here, Kind::SelectorsNotSorted { index: to_u32(i) });
        }
        last_selector = selector.saturating_add(1);
        if !bounds_ok {
            continue;
        }
        if default != 0 {
            default_ranges(v, sub, off, to_usize(default));
        }
        if non_default != 0 {
            non_default_mappings(v, sub, off, to_usize(non_default), wide);
        }
    }
}

fn default_ranges(v: &mut V<'_>, sub: &[u8], off: usize, at: usize) {
    let avail = sub.len();
    let Some(count) = be::u32_at(sub, at).map(to_usize) else {
        return v.truncated(off.saturating_add(at), at.saturating_add(4), avail);
    };
    if count > avail.saturating_sub(at.saturating_add(4)) / 4 {
        return v.truncated(
            off.saturating_add(at),
            count.saturating_mul(4).saturating_add(at).saturating_add(4),
            avail,
        );
    }
    let mut last = 0u32;
    for k in 0..count {
        if !v.spend() {
            return;
        }
        let pos = k.saturating_mul(4).saturating_add(at).saturating_add(4);
        let (Some(start), Some(extra)) =
            (be::u24_at(sub, pos), be::u8_at(sub, pos.saturating_add(3)))
        else {
            return;
        };
        let end = start.saturating_add(u32::from(extra));
        if end >= 0x11_0000 {
            v.emit(
                off.saturating_add(pos),
                Kind::CodepointOutOfRange { codepoint: end },
            );
        } else if start < last {
            v.emit(
                off.saturating_add(pos),
                Kind::GroupsNotSorted { index: to_u32(k) },
            );
        }
        last = end.saturating_add(1);
    }
}

fn non_default_mappings(v: &mut V<'_>, sub: &[u8], off: usize, at: usize, wide: bool) {
    let avail = sub.len();
    let size = if wide { 6usize } else { 5 };
    let Some(count) = be::u32_at(sub, at).map(to_usize) else {
        return v.truncated(off.saturating_add(at), at.saturating_add(4), avail);
    };
    if count
        > avail
            .saturating_sub(at.saturating_add(4))
            .checked_div(size)
            .unwrap_or(0)
    {
        return v.truncated(
            off.saturating_add(at),
            count
                .saturating_mul(size)
                .saturating_add(at)
                .saturating_add(4),
            avail,
        );
    }
    let mut last = 0u32;
    for k in 0..count {
        if !v.spend() {
            return;
        }
        let pos = k.saturating_mul(size).saturating_add(at).saturating_add(4);
        let Some(unicode) = be::u24_at(sub, pos) else {
            return;
        };
        let glyph = if wide {
            be::u24_at(sub, pos.saturating_add(3))
        } else {
            be::u16_at(sub, pos.saturating_add(3)).map(u32::from)
        };
        let here = off.saturating_add(pos);
        if unicode >= 0x11_0000 {
            v.emit(here, Kind::CodepointOutOfRange { codepoint: unicode });
        } else if unicode < last {
            v.emit(here, Kind::GroupsNotSorted { index: to_u32(k) });
        }
        last = unicode.saturating_add(1);
        if let Some(g) = glyph {
            v.check_glyph(here, g);
        }
    }
}
