//! cmap and DMAP (`src/hb-ot-cmap-table.hh`'s `cmap::subset`).

use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_write::{Errors, Serializer, Snapshot, Whence};

use crate::cmap_accel::{CmapTable, Record, Subtable, bsearch};
use crate::error::SubsetError;
use crate::plan::Plan;
use crate::set::{INVALID, RangeSet};
use crate::tables::Tables;

/// The low 16 bits of `v` (C's conversion to `uint16_t`).
fn low16(v: usize) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

/// The low 32 bits of `v` (C's conversion to `uint32_t`).
fn low32(v: u64) -> u32 {
    u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0)
}

/// `filter_encoding_records_for_subset`.
fn keep_record(t: &CmapTable<'_>, r: &Record) -> bool {
    matches!((r.platform, r.encoding), (0, 3) | (0, 4) | (3, 1) | (3, 10))
        || t.subtable(r).is_variation_selector()
}

/// The characters of a subtable (`SubtableUnicodesCache::set_for`).
fn unicodes_of(t: &CmapTable<'_>, r: &Record) -> RangeSet {
    let mut s = RangeSet::new();
    t.subtable(r).collect_unicodes(&mut s, u32::MAX);
    s
}

/// `cmap::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    src: &[u8],
    is_dmap: bool,
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let tag = Tag::new(if is_dmap { b"DMAP" } else { b"cmap" });
    let t = CmapTable::new(src).ok_or(SubsetError::Malformed { tag })?;
    let records: Vec<Record> = t.records().filter(|r| keep_record(&t, r)).collect();
    if records.is_empty() {
        return Ok(false);
    }
    let has_format12 = records.iter().any(|r| t.subtable(r).format() == 12);
    let has = |p: u16, e: u16| records.iter().any(|r| r.platform == p && r.encoding == e);
    if !is_dmap && !has_format12 && !has(0, 3) && !has(3, 1) {
        return Ok(false);
    }
    if !is_dmap && has_format12 && !has(0, 4) && !has(3, 10) {
        return Ok(false);
    }
    let it: Vec<(u32, u32)> = plan
        .unicode_to_new_gid
        .iter()
        .copied()
        .filter(|p| p.1 != INVALID)
        .collect();
    s.u16(0);
    s.u16(0);
    let snap = s.snapshot();
    let needed = serialize(plan, &t, &records, &it, s, snap, false);
    Ok(needed)
}

/// `cmap::serialize` after the header.
fn serialize(
    plan: &Plan,
    t: &CmapTable<'_>,
    records: &[Record],
    it: &[(u32, u32)],
    s: &mut Serializer,
    snap: Snapshot,
    drop_format_4: bool,
) -> bool {
    let mut f4 = 0u32;
    let mut f12 = 0u32;
    let mut f14 = 0u32;
    let mut f15 = 0u32;
    for r in records {
        if s.in_error() {
            return false;
        }
        let sub = t.subtable(r);
        let format = sub.format();
        if !matches!(format, 4 | 12 | 14 | 15) {
            continue;
        }
        let set = unicodes_of(t, r);
        let filtered: Vec<(u32, u32)> = it.iter().copied().filter(|p| set.contains(p.0)).collect();
        if !drop_format_4 && format == 4 {
            copy_record(plan, r, &sub, &filtered, 4, s, &mut f4);
            if s.in_error() && s.only_overflow() {
                s.revert(snap);
                return serialize(plan, t, records, it, s, snap, true);
            }
        } else if format == 12 {
            if can_drop(t, r, &set, it, records) {
                continue;
            }
            copy_record(plan, r, &sub, &filtered, 12, s, &mut f12);
        } else if format == 14 {
            copy_record(plan, r, &sub, it, 14, s, &mut f14);
        } else if format == 15 {
            copy_record(plan, r, &sub, it, 15, s, &mut f15);
        }
    }
    let len = s.len();
    let available = len.saturating_sub(4);
    s.check_assign_u16(2, (available / 8) as u64, Errors::INT_OVERFLOW);
    !drop_format_4 || f12 != 0
}

/// `EncodingRecord::copy`.
fn copy_record(
    plan: &Plan,
    r: &Record,
    sub: &Subtable<'_>,
    it: &[(u32, u32)],
    format: u16,
    s: &mut Serializer,
    objidx: &mut u32,
) {
    let snap = s.snapshot();
    let at = s.u16(r.platform);
    s.u16(r.encoding);
    s.u32(0);
    if *objidx == 0 {
        s.push();
        match format {
            4 => format4(s, it),
            12 => format12(s, it),
            _ => format14(plan, sub, format == 15, s),
        }
        if !s.is_empty() && !s.in_error() {
            *objidx = s.pop_pack(true);
        } else {
            s.pop_discard();
        }
    }
    if *objidx == 0 {
        s.revert(snap);
        return;
    }
    s.add_link(at.saturating_add(4), 4, false, *objidx, Whence::Head, 0);
}

/// `cmap::_can_drop`.
fn can_drop(
    t: &CmapTable<'_>,
    cmap12: &Record,
    cmap12_unicodes: &RangeSet,
    it: &[(u32, u32)],
    records: &[Record],
) -> bool {
    if it
        .iter()
        .any(|p| cmap12_unicodes.contains(p.0) && p.0 >= 0x10000)
    {
        return false;
    }
    let (tp, te) = match (cmap12.platform, cmap12.encoding) {
        (0, 4) => (0, 3),
        (3, 10) => (3, 1),
        _ => return false,
    };
    let lang = t.subtable(cmap12).language();
    let Some(sib) = records
        .iter()
        .find(|r| r.platform == tp && r.encoding == te && t.subtable(r).language() == lang)
    else {
        return false;
    };
    let sibling = unicodes_of(t, sib);
    let a = it
        .iter()
        .map(|p| p.0)
        .filter(|c| cmap12_unicodes.contains(*c));
    let b = it.iter().map(|p| p.0).filter(|c| sibling.contains(*c));
    a.eq(b)
}

/// `CmapSubtableFormat4::to_ranges`: calls `w(start, end, delta)` per
/// segment.
fn to_ranges(it: &[(u32, u32)], mut w: impl FnMut(u32, u32, i32)) {
    let commit = |start: u32,
                  prev_run_start: u32,
                  run_start: u32,
                  end: u32,
                  run_delta: i32,
                  prev_delta: i32,
                  split_cost: i32,
                  w: &mut dyn FnMut(u32, u32, i32)| {
        let mut split = false;
        if start < run_start && run_start < end {
            let run_cost = end
                .wrapping_sub(run_start)
                .wrapping_add(1)
                .cast_signed()
                .wrapping_mul(2);
            if run_cost >= split_cost {
                split = true;
            }
        }
        if split {
            if start == prev_run_start {
                w(start, run_start.wrapping_sub(1), prev_delta);
            } else {
                w(start, run_start.wrapping_sub(1), 0);
            }
            w(run_start, end, run_delta);
            return;
        }
        if start == run_start {
            w(start, end, run_delta);
            return;
        }
        w(start, end, 0);
    };
    let mut i = 0usize;
    let mut end_cp = 0u32;
    while let Some(&(first_cp, first_gid)) = it.get(i) {
        let mut start_cp = first_cp;
        let mut prev_run_start_cp = start_cp;
        let mut run_start_cp = start_cp;
        end_cp = start_cp;
        let mut last_gid = first_gid;
        let mut run_length: i32 = 1;
        let mut prev_delta: i32 = 0;
        let mut delta = last_gid.wrapping_sub(start_cp).cast_signed();
        let mut first_sub_range = true;
        i = i.saturating_add(1);
        while let Some(&(next_cp, next_gid)) = it.get(i) {
            if next_cp != end_cp.wrapping_add(1) {
                break;
            }
            if next_gid == last_gid.wrapping_add(1) {
                end_cp = next_cp;
                run_length = run_length.wrapping_add(1);
                last_gid = next_gid;
                i = i.saturating_add(1);
                continue;
            }
            let split_cost = if first_sub_range { 8 } else { 16 };
            if run_length.wrapping_mul(2) >= split_cost {
                commit(
                    start_cp,
                    prev_run_start_cp,
                    run_start_cp,
                    end_cp,
                    delta,
                    prev_delta,
                    split_cost,
                    &mut w,
                );
                start_cp = next_cp;
            }
            first_sub_range = false;
            prev_run_start_cp = run_start_cp;
            run_start_cp = next_cp;
            end_cp = next_cp;
            prev_delta = delta;
            delta = next_gid.wrapping_sub(run_start_cp).cast_signed();
            run_length = 1;
            last_gid = next_gid;
            i = i.saturating_add(1);
        }
        commit(
            start_cp,
            prev_run_start_cp,
            run_start_cp,
            end_cp,
            delta,
            prev_delta,
            8,
            &mut w,
        );
    }
    if end_cp != 0xFFFF {
        w(0xFFFF, 0xFFFF, 1);
    }
}

/// `CmapSubtableFormat4::serialize`.
fn format4(s: &mut Serializer, it: &[(u32, u32)]) {
    let pairs: Vec<(u32, u32)> = it.iter().copied().filter(|p| p.0 <= 0xFFFF).collect();
    if pairs.is_empty() {
        return;
    }
    let init = s.len();
    s.zeros(14);
    s.set_u16(init, 4);
    let mut segs: Vec<(u32, u32, i32)> = Vec::new();
    to_ranges(&pairs, |a, b, d| segs.push((a, b, d)));
    let n = segs.len();
    segs.iter().for_each(|&(_, e, _)| {
        s.u16((e & 0xFFFF) as u16);
    });
    s.u16(0);
    segs.iter().for_each(|&(a, _, _)| {
        s.u16((a & 0xFFFF) as u16);
    });
    segs.iter().for_each(|&(_, _, d)| {
        s.u16((d.cast_unsigned() & 0xFFFF) as u16);
    });
    let ro_at = s.zeros(n.saturating_mul(2));
    let lookup = |cp: u32| -> Option<u32> {
        bsearch(pairs.len(), |i| {
            pairs
                .get(i)
                .map_or(core::cmp::Ordering::Less, |p| p.0.cmp(&cp))
        })
        .and_then(|i| pairs.get(i))
        .map(|p| p.1)
    };
    for (i, &(a, b, d)) in segs.iter().enumerate() {
        if (d.cast_unsigned() & 0xFFFF) != 0 {
            continue;
        }
        let identity = (a..=b).all(|cp| lookup(cp).unwrap_or(0) == cp);
        if identity {
            continue;
        }
        let here = s.len();
        let rec = ro_at.saturating_add(i.saturating_mul(2));
        let ro = (here.saturating_sub(rec) / 2).saturating_mul(2);
        s.set_u16(rec, low16(ro));
        for cp in a..=b {
            s.u16((lookup(cp).unwrap_or(INVALID) & 0xFFFF) as u16);
        }
    }
    let len = s.len().saturating_sub(init);
    if len > 0xFFFF {
        s.set_u16(init.saturating_add(2), low16(len));
        s.err(Errors::INT_OVERFLOW);
        return;
    }
    s.set_u16(init.saturating_add(2), low16(len));
    let segcount = low32(n as u64);
    let entry_selector = u32::BITS
        .wrapping_sub(segcount.leading_zeros())
        .max(1)
        .wrapping_sub(1);
    let search_range = (1u32 << entry_selector).wrapping_mul(2);
    let range_shift = segcount.wrapping_mul(2).saturating_sub(search_range);
    s.set_u16(
        init.saturating_add(6),
        (segcount.wrapping_mul(2) & 0xFFFF) as u16,
    );
    s.set_u16(init.saturating_add(8), (search_range & 0xFFFF) as u16);
    s.set_u16(init.saturating_add(10), (entry_selector & 0xFFFF) as u16);
    s.set_u16(init.saturating_add(12), (range_shift & 0xFFFF) as u16);
}

/// `CmapSubtableFormat12::serialize`.
fn format12(s: &mut Serializer, it: &[(u32, u32)]) {
    let Some(&(c0, g0)) = it.first() else {
        return;
    };
    let init = s.len();
    s.zeros(16);
    let (mut start, mut end, mut gid) = (c0, c0, g0);
    let mut groups: Vec<(u32, u32, u32)> = Vec::new();
    for &(cp, g) in it.iter().skip(1) {
        if cp.wrapping_sub(1) == end && g == gid.wrapping_add(cp.wrapping_sub(start)) {
            end = cp;
        } else {
            groups.push((start, end, gid));
            (start, end, gid) = (cp, cp, g);
        }
    }
    groups.push((start, end, gid));
    for (a, b, g) in &groups {
        s.u32(*a);
        s.u32(*b);
        s.u32(*g);
    }
    let len = s.len().saturating_sub(init);
    s.set_u16(init, 12);
    s.set_u32(init.saturating_add(4), low32(len as u64));
    s.set_u32(
        init.saturating_add(12),
        low32((len.saturating_sub(16) / 12) as u64),
    );
}

/// `CmapSubtableFormat14_15::serialize`.
fn format14(plan: &Plan, src: &Subtable<'_>, wide: bool, s: &mut Serializer) {
    let snap = s.snapshot();
    let init = s.len();
    let init_tail = s.packed_len();
    s.u16(if wide { 15 } else { 14 });
    s.u32(0);
    s.u32(0);
    let recs: Vec<(u32, u32, u32, usize)> = src.uvs_records().collect();
    let mut objs: Vec<(u32, u32)> = Vec::new();
    for &(sel, def, nd, _) in recs.iter().rev() {
        if !plan.unicodes.contains(sel) {
            continue;
        }
        let r = copy_vs_record(plan, src, wide, sel, def, nd, s);
        if r.0 != 0 || r.1 != 0 {
            objs.push(r);
        }
    }
    if s.len().saturating_sub(init) == 10 {
        s.revert(snap);
        return;
    }
    let tail_len = s.packed_len().saturating_sub(init_tail);
    let length = s.len().saturating_sub(init).saturating_add(tail_len) as u64;
    s.check_assign_u32(init.saturating_add(2), length, Errors::INT_OVERFLOW);
    let n = s.len().saturating_sub(init).saturating_sub(10) / 11;
    s.check_assign_u32(init.saturating_add(6), n as u64, Errors::INT_OVERFLOW);
    // Reverse the records.
    let base = init.saturating_add(10);
    let recs_bytes: Vec<[u8; 11]> = (0..n)
        .map(|i| {
            let mut r = [0u8; 11];
            let at = base.saturating_add(i.saturating_mul(11));
            if let Some(b) = s.current().get(at..at.saturating_add(11)) {
                r.copy_from_slice(b);
            }
            r
        })
        .collect();
    for (i, r) in recs_bytes.iter().rev().enumerate() {
        s.write_at(base.saturating_add(i.saturating_mul(11)), r);
    }
    for (i, &(d, nd)) in objs.iter().enumerate() {
        let j = objs.len().saturating_sub(1).saturating_sub(i);
        let at = base.saturating_add(j.saturating_mul(11));
        s.add_link(at.saturating_add(3), 4, false, d, Whence::Head, 0);
        s.add_link(at.saturating_add(7), 4, false, nd, Whence::Head, 0);
    }
}

/// `VariationSelectorRecord::copy`: (default UVS, non-default UVS).
fn copy_vs_record(
    plan: &Plan,
    src: &Subtable<'_>,
    wide: bool,
    sel: u32,
    def: u32,
    nd: u32,
    s: &mut Serializer,
) -> (u32, u32) {
    let snap = s.snapshot();
    let b = sel.to_be_bytes();
    s.extend(b.get(1..).unwrap_or(&[]));
    s.u32(0);
    s.u32(0);
    let mut nd_idx = 0;
    if nd != 0 {
        s.push();
        if copy_non_default(plan, src, wide, nd, s) {
            nd_idx = s.pop_pack(true);
        } else {
            s.pop_discard();
        }
    }
    let mut def_idx = 0;
    if def != 0 {
        s.push();
        if copy_default(&plan.unicodes, src, def, s) {
            def_idx = s.pop_pack(true);
        } else {
            s.pop_discard();
        }
    }
    if def_idx == 0 && nd_idx == 0 {
        s.revert(snap);
    }
    (def_idx, nd_idx)
}

/// `NonDefaultUVS::copy`.
fn copy_non_default(
    plan: &Plan,
    src: &Subtable<'_>,
    wide: bool,
    off: u32,
    s: &mut Serializer,
) -> bool {
    let kept: Vec<(u32, u32)> = src
        .non_default(off)
        .filter(|&(u, g)| plan.unicodes.contains(u) || plan.glyphs_requested.contains(g))
        .collect();
    if kept.is_empty() {
        return false;
    }
    s.u32(low32(kept.len() as u64));
    for (u, g) in kept {
        let n = plan.new_glyph(g).unwrap_or(INVALID);
        let ub = u.to_be_bytes();
        s.extend(ub.get(1..).unwrap_or(&[]));
        if wide {
            let nb = n.to_be_bytes();
            s.extend(nb.get(1..).unwrap_or(&[]));
        } else {
            s.u16((n & 0xFFFF) as u16);
        }
    }
    true
}

/// `DefaultUVS::copy`.
fn copy_default(unicodes: &RangeSet, src: &Subtable<'_>, off: u32, s: &mut Serializer) -> bool {
    let snap = s.snapshot();
    let len_at = s.u32(0);
    let init = s.len();
    let ranges: Vec<(u32, u32)> = src.default_ranges(off).collect();
    let len = ranges.len() as u64;
    let write = |s: &mut Serializer, start: u32, count: u32| {
        let b = start.to_be_bytes();
        s.extend(b.get(1..).unwrap_or(&[]));
        s.u8((count & 0xFF) as u8);
    };
    let bit_storage = u64::from(u32::BITS.wrapping_sub(low32(len).leading_zeros()));
    if len > unicodes.len().saturating_mul(bit_storage) {
        let mut start = INVALID;
        let mut end = INVALID;
        for u in unicodes.iter() {
            let found = bsearch(ranges.len(), |i| {
                let (a, c) = ranges.get(i).copied().unwrap_or((0, 0));
                if u < a {
                    core::cmp::Ordering::Greater
                } else if u <= a.wrapping_add(c) {
                    core::cmp::Ordering::Equal
                } else {
                    core::cmp::Ordering::Less
                }
            });
            if found.is_none() {
                continue;
            }
            if start == INVALID {
                start = u;
                end = start.wrapping_sub(1);
            }
            if end.wrapping_add(1) != u || end.wrapping_sub(start) == 255 {
                write(s, start, end.wrapping_sub(start));
                start = u;
            }
            end = u;
        }
        if start != INVALID {
            write(s, start, end.wrapping_sub(start));
        }
    } else {
        let mut last_code = INVALID;
        let mut count: i64 = -1;
        for &(a, c) in &ranges {
            let mut cur = a.wrapping_sub(1);
            let end = cur.wrapping_add(c).wrapping_add(2);
            loop {
                cur = unicodes.next(cur);
                if cur == INVALID || cur >= end {
                    break;
                }
                count = count.saturating_add(1);
                if last_code == INVALID {
                    last_code = cur;
                } else if i64::from(last_code).saturating_add(count) != i64::from(cur)
                    || count > 255
                {
                    write(s, last_code, low32(count.wrapping_sub(1).cast_unsigned()));
                    last_code = cur;
                    count = 0;
                }
            }
        }
        if last_code != INVALID {
            write(s, last_code, low32(count.cast_unsigned()));
        }
    }
    if s.len() == init {
        s.revert(snap);
        return false;
    }
    let n = s.len().saturating_sub(init) / 4;
    s.check_assign_u32(len_at, n as u64, Errors::INT_OVERFLOW)
}
