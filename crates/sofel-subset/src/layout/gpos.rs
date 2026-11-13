//! The subsetting of GPOS's own subtables (`OT/Layout/GPOS/`): value
//! records and devices, anchors, single, pair, cursive and mark
//! attachment positioning.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use super::common::Sub;
use super::data::{Cls, Cov, D, bit_storage, n16, n32};
use super::{Gsubgpos, Kind, LayoutPlan};
use crate::bits::BitSet;
use crate::input::Flags;

/// The number of 16-bit values of a ValueRecord of `format`.
fn value_len(format: u16) -> usize {
    (format & 0xFF).count_ones() as usize
}

fn collect_value_devices(
    base: D<'_>,
    values: D<'_>,
    vf: [u16; 2],
    keep: bool,
    selected: &mut BTreeSet<u32>,
) -> Option<()> {
    let mut start = 0usize;
    for format in vf {
        for bit in 4..8u16 {
            let flag = 1u16 << bit;
            if format & flag == 0 {
                continue;
            }
            let index = (format & flag.wrapping_sub(1)).count_ones() as usize;
            let off = values.u16(start.saturating_add(index.saturating_mul(2)));
            let dev = base.follow(u32::from(off));
            if dev.is_null() || dev.u16(4) != 0x8000 {
                continue;
            }
            if dev.0.len() < 6 {
                return None;
            }

            if keep {
                selected.insert((u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2)));
            }
        }
        start = start.saturating_add(value_len(format).saturating_mul(2));
    }
    Some(())
}

fn collect_anchor_devices(anchor: D<'_>, keep: bool, selected: &mut BTreeSet<u32>) -> Option<()> {
    if anchor.is_null() || anchor.u16(0) != 3 {
        return Some(());
    }
    if anchor.0.len() < 10 {
        return None;
    }
    for at in [6, 8] {
        let dev = anchor.off16(at);
        if dev.is_null() || dev.u16(4) != 0x8000 {
            continue;
        }
        if dev.0.len() < 6 {
            return None;
        }

        if keep {
            selected.insert((u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2)));
        }
    }
    Some(())
}

fn collect_mark_anchors(
    marks: D<'_>,
    coverage: Cov<'_>,
    glyphs: &BitSet,
    selected: &mut BTreeSet<u32>,
) -> Option<()> {
    let n = usize::from(marks.u16(0));
    if n.checked_mul(4)? > marks.0.len().saturating_sub(2) {
        return None;
    }
    for (i, glyph) in coverage.glyphs().into_iter().take(n).enumerate() {
        let anchor = marks.off16(4usize.saturating_add(i.saturating_mul(4)));
        collect_anchor_devices(anchor, glyphs.has(glyph), selected)?;
    }
    Some(())
}

fn collect_anchor_matrix(
    matrix: D<'_>,
    class_count: usize,
    kept: &BTreeSet<usize>,
    selected: &mut BTreeSet<u32>,
) -> Option<()> {
    let cells = usize::from(matrix.u16(0)).checked_mul(class_count)?;
    if cells.checked_mul(2)? > matrix.0.len().saturating_sub(2) {
        return None;
    }
    for i in 0..cells {
        let anchor = matrix.off16(2usize.saturating_add(i.saturating_mul(2)));
        collect_anchor_devices(anchor, kept.contains(&i), selected)?;
    }
    Some(())
}

fn collect_mark_base_like_anchors(
    st: D<'_>,
    glyphs: &BitSet,
    selected: &mut BTreeSet<u32>,
) -> Option<()> {
    let mark_cov = Cov(st.off16(2));
    let base_cov = Cov(st.off16(4));
    let class_count = usize::from(st.u16(6));
    let marks = st.off16(8);
    let bases = st.off16(10);
    if usize::from(bases.u16(0))
        .checked_mul(class_count)?
        .checked_mul(2)?
        > bases.0.len().saturating_sub(2)
    {
        return None;
    }
    collect_mark_anchors(marks, mark_cov, glyphs, selected)?;
    let classes = mark_classes(mark_cov, marks, glyphs);
    let mut kept = BTreeSet::new();
    for (row, glyph) in base_cov
        .glyphs()
        .into_iter()
        .take(usize::from(bases.u16(0)))
        .enumerate()
    {
        if !glyphs.has(glyph) {
            continue;
        }
        let at = |col: usize| row.saturating_mul(class_count).saturating_add(col);
        if (0..class_count)
            .any(|col| classes.contains_key(&n32(col)) && bases.u16_at(2, at(col)) != 0)
        {
            kept.extend(
                (0..class_count)
                    .filter(|&col| classes.contains_key(&n32(col)))
                    .map(at),
            );
        }
    }
    collect_anchor_matrix(bases, class_count, &kept, selected)
}

fn collect_mark_lig_anchors(
    st: D<'_>,
    glyphs: &BitSet,
    selected: &mut BTreeSet<u32>,
) -> Option<()> {
    let mark_cov = Cov(st.off16(2));
    let lig_cov = Cov(st.off16(4));
    let class_count = usize::from(st.u16(6));
    let marks = st.off16(8);
    let ligs = st.off16(10);
    collect_mark_anchors(marks, mark_cov, glyphs, selected)?;
    let classes = mark_classes(mark_cov, marks, glyphs);
    let n = usize::from(ligs.u16(0));
    if n.checked_mul(2)? > ligs.0.len().saturating_sub(2) {
        return None;
    }
    for (i, glyph) in lig_cov.glyphs().into_iter().take(n).enumerate() {
        let attach = ligs.off16(2usize.saturating_add(i.saturating_mul(2)));
        if attach.is_null() {
            continue;
        }
        let cells = usize::from(attach.u16(0)).checked_mul(class_count)?;
        if cells.checked_mul(2)? > attach.0.len().saturating_sub(2) {
            return None;
        }
        let in_class = |cell: usize| {
            cell.checked_rem(class_count)
                .is_some_and(|class| classes.contains_key(&n32(class)))
        };
        let keep = glyphs.has(glyph)
            && (0..cells).any(|cell| in_class(cell) && attach.u16_at(2, cell) != 0);
        let kept = (0..cells).filter(|&cell| keep && in_class(cell)).collect();
        collect_anchor_matrix(attach, class_count, &kept, selected)?;
    }
    Some(())
}

/// Collect only Devices reached through a retained ValueRecord or Anchor.
/// Context lookups invoke other lookups and contain no Devices themselves.
pub(crate) fn retained_variation_indices(
    data: &[u8],
    plan: &LayoutPlan,
    glyphs: &BitSet,
    num_glyphs: u32,
) -> Option<BTreeSet<u32>> {
    let t = Gsubgpos::new(data, false);
    let mut selected = BTreeSet::new();
    let list = t.lookup_list();
    for i in 0..usize::from(list.u16(0)) {
        let lookup = list.off16(2usize.saturating_add(i.saturating_mul(2)));
        if !plan.lookups.contains_key(&n32(i)) {
            continue;
        }
        let ty = lookup.u16(0);
        for j in 0..usize::from(lookup.u16(4)) {
            let st = lookup.off16(6usize.saturating_add(j.saturating_mul(2)));
            let Some((kind, subtable)) = super::resolve(ty, st, false) else {
                continue;
            };
            if kind == Kind::SinglePos {
                let (cov, vf, n, start) = single_records(subtable);
                match subtable.u16(0) {
                    1 | 3 => {
                        collect_value_devices(
                            subtable,
                            subtable.at(start),
                            [vf, 0],
                            cov.intersects(glyphs),
                            &mut selected,
                        )?;
                    }
                    2 | 4 => {
                        let record_len = value_len(vf).saturating_mul(2);
                        if n.checked_mul(record_len)? > subtable.0.len().saturating_sub(start) {
                            return None;
                        }
                        for (k, glyph) in cov.glyphs().into_iter().take(n).enumerate() {
                            collect_value_devices(
                                subtable,
                                subtable.at(start.saturating_add(k.saturating_mul(record_len))),
                                [vf, 0],
                                glyphs.has(glyph),
                                &mut selected,
                            )?;
                        }
                    }
                    _ => return None,
                }
                continue;
            }
            if kind == Kind::CursivePos {
                let (cov, n, start, width) = cursive_records(subtable)?;
                let stride = usize::from(width).saturating_mul(2);
                if n.checked_mul(stride)? > subtable.0.len().saturating_sub(start) {
                    return None;
                }
                for (k, glyph) in cov.glyphs().into_iter().take(n).enumerate() {
                    let rec = start.saturating_add(k.saturating_mul(stride));
                    for at in [rec, rec.saturating_add(usize::from(width))] {
                        collect_anchor_devices(
                            cursive_anchor(subtable, at, width),
                            glyphs.has(glyph),
                            &mut selected,
                        )?;
                    }
                }
                continue;
            }
            if matches!(kind, Kind::MarkBasePos | Kind::MarkMarkPos) && subtable.u16(0) == 1 {
                collect_mark_base_like_anchors(subtable, glyphs, &mut selected)?;
                continue;
            }
            if kind == Kind::MarkLigPos && subtable.u16(0) == 1 {
                collect_mark_lig_anchors(subtable, glyphs, &mut selected)?;
                continue;
            }
            if matches!(kind, Kind::Context | Kind::ChainContext) {
                continue;
            }
            if kind != Kind::PairPos {
                return None;
            }
            let pair = subtable;
            let (cov, vf) = if pair.u16(0) >= 3 {
                (Cov(pair.off32(2)), [pair.u16(6), pair.u16(8)])
            } else {
                (Cov(pair.off16(2)), [pair.u16(4), pair.u16(6)])
            };
            if (vf[0] | vf[1]) & 0x00f0 == 0 {
                continue;
            }
            let lengths = [value_len(vf[0]), value_len(vf[1])];
            match pair.u16(0) {
                1 => {
                    let record_len = 2usize
                        .saturating_add(lengths[0].saturating_add(lengths[1]).saturating_mul(2));
                    for (k, first) in cov
                        .glyphs()
                        .into_iter()
                        .take(usize::from(pair.u16(8)))
                        .enumerate()
                    {
                        let ps = pair.off16(10usize.saturating_add(k.saturating_mul(2)));
                        if ps.is_null() {
                            continue;
                        }
                        for m in 0..usize::from(ps.u16(0)) {
                            let r = ps.at(2usize.saturating_add(m.saturating_mul(record_len)));
                            let keep = glyphs.has(first) && glyphs.has(u32::from(r.u16(0)));
                            collect_value_devices(ps, r.at(2), vf, keep, &mut selected)?;
                        }
                    }
                }
                2 | 4 => {
                    let (cd1, cd2, n1, n2, start) = pair_classes(pair);
                    let (c1_count, c2_count) = (usize::from(n1), usize::from(n2));
                    let record_len = lengths[0].saturating_add(lengths[1]).saturating_mul(2);
                    let bytes = c1_count.checked_mul(c2_count)?.checked_mul(record_len)?;
                    if bytes > pair.0.len().saturating_sub(start) {
                        return None;
                    }
                    let (cd1, cd2) = (Cls(cd1), Cls(cd2));
                    let (pairs1, classes1) = cd1.subset_pairs(
                        glyphs,
                        num_glyphs,
                        |g| glyphs.has(g).then_some(g),
                        |g| cov.has(g),
                    );
                    let count_covered = glyphs.iter().filter(|&g| cov.has(g)).count();
                    let mut rows: BTreeSet<u32> = classes1.iter().collect();
                    if count_covered > pairs1.len() {
                        rows.insert(0);
                    }
                    let (_, classes2) = cd2.subset_pairs(
                        glyphs,
                        num_glyphs,
                        |g| glyphs.has(g).then_some(g),
                        |_| true,
                    );
                    let mut cols: BTreeSet<u32> = classes2.iter().collect();
                    cols.insert(0);
                    let keep_subtable = cov.intersects(glyphs) && cd2.intersects(glyphs);
                    for c1 in 0..c1_count {
                        for c2 in 0..c2_count {
                            let at = start.saturating_add(
                                c1.saturating_mul(c2_count)
                                    .saturating_add(c2)
                                    .saturating_mul(record_len),
                            );
                            let keep =
                                keep_subtable && rows.contains(&n32(c1)) && cols.contains(&n32(c2));
                            collect_value_devices(pair, pair.at(at), vf, keep, &mut selected)?;
                        }
                    }
                }
                _ => return None,
            }
        }
    }
    Some(selected)
}

/// `ValueFormat::get_effective_format` without instancing: drops the
/// device flags with `strip_hints` and the zero values with
/// `strip_empty`.
fn effective_format(format: u16, values: D<'_>, strip_hints: bool, strip_empty: bool) -> u16 {
    let mut out = format;
    let mut i = 0usize;
    for bit in 0..8u16 {
        let flag = 1u16 << bit;
        if format & flag == 0 {
            continue;
        }
        if strip_hints && flag >= 0x10 {
            out &= !flag;
            i = i.wrapping_add(1);
            continue;
        }
        if strip_empty && values.u16_at(0, i) == 0 {
            out &= !flag;
        }
        i = i.wrapping_add(1);
    }
    out
}

/// With NO_HINTING, hinting Devices disappear but a retained PairPos
/// VariationIndex continues to address the compacted GDEF store.
fn pair_effective_format(s: &Sub<'_, '_>, format: u16, base: D<'_>, values: D<'_>) -> u16 {
    let mut out = effective_format(format, values, true, true);
    for bit in 4..8u16 {
        let flag = 1u16 << bit;
        if format & flag == 0 {
            continue;
        }
        let index = (format & flag.wrapping_sub(1)).count_ones() as usize;
        let dev = base.follow(u32::from(values.u16_at(0, index)));
        if dev.u16(4) != 0x8000 {
            continue;
        }
        let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
        if s.p.layout.gdef_var_map.contains_key(&old) {
            out |= flag;
        }
    }
    out
}

/// `ValueFormat::get_effective_format` when instancing: the device
/// flags dropped (their variation indices map to no variations, or are
/// not mapped) and nothing else.
fn pinned_value_delta(s: &Sub<'_, '_>, format: u16, base: D<'_>, values: D<'_>, bit: u16) -> i32 {
    let flag = 0x10u16 << bit;
    if format & flag == 0 {
        return 0;
    }
    let index = (format & flag.wrapping_sub(1)).count_ones() as usize;
    let off = values.u16_at(0, index);
    if off == 0 {
        return 0;
    }
    super::gdef::pinned_delta(s.p, base.follow(u32::from(off)))
}

fn instanced_format(s: &Sub<'_, '_>, format: u16, base: D<'_>, values: D<'_>) -> u16 {
    let mut result = format & 0xf;
    for bit in 0..4 {
        let device_flag = 0x10u16 << bit;
        if format & device_flag != 0 {
            let index = (format & device_flag.wrapping_sub(1)).count_ones() as usize;
            let offset = values.u16_at(0, index);
            let dev = base.follow(u32::from(offset));
            let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
            if offset != 0
                && super::gdef::is_variation_device(dev)
                && s.p.layout.gdef_var_map.contains_key(&old)
            {
                result |= device_flag;
            }
        }
        if pinned_value_delta(s, format, base, values, bit) != 0 {
            result |= 1 << bit;
        }
    }
    result
}

/// Whether `NO_HINTING` strips hinting Devices from value records.
/// Retained PairPos variation Devices are handled separately.
fn strip_hints(s: &Sub<'_, '_>) -> bool {
    s.p.flags.has(Flags::NO_HINTING)
}

/// `Device::copy` into the current object: hinting devices are copied,
/// retained GPOS variation indices use the GDEF store's new indices.
fn device_copy(s: &mut Sub<'_, '_>, dev: D<'_>) -> bool {
    match dev.u16(4) {
        1..=3 => {
            let start = dev.u16(0);
            let end = dev.u16(2);
            let f = dev.u16(4);
            let size = if start > end {
                6
            } else {
                2usize.saturating_mul(4usize.saturating_add(usize::from(
                    end.wrapping_sub(start) >> (4u16.wrapping_sub(f)),
                )))
            };
            s.s.extend(dev.slice(0, size));
            true
        }
        0x8000 => {
            let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
            let Some(&new) = s.p.layout.gdef_var_map.get(&old) else {
                return false;
            };
            s.s.u16(u16::try_from(new >> 16).unwrap_or(0));
            s.s.u16(u16::try_from(new & 0xffff).unwrap_or(0));
            s.s.u16(0x8000);
            true
        }
        _ => false,
    }
}

/// `ValueFormat::copy_values`: the values of `src_format` at `values`,
/// written in `new_format`; devices are relative to `base`.
fn copy_values(s: &mut Sub<'_, '_>, src_format: u16, new_format: u16, base: D<'_>, values: D<'_>) {
    if src_format == 0 {
        return;
    }
    let mut i = 0usize;
    for bit in 0..4u16 {
        let flag = 1u16 << bit;
        if (src_format | new_format) & flag == 0 {
            continue;
        }
        let v = if src_format & flag != 0 {
            let v = values.u16_at(0, i);
            i = i.wrapping_add(1);
            v
        } else {
            0
        };
        if new_format & flag != 0 {
            s.s.u16(v.wrapping_add(crate::instance::lo16(pinned_value_delta(
                s, src_format, base, values, bit,
            ))));
        }
    }
    if src_format & 0xF0 == 0 {
        return;
    }
    for bit in 4..8u16 {
        let flag = 1u16 << bit;
        if src_format & flag == 0 {
            continue;
        }
        let off = values.u16_at(0, i);
        i = i.wrapping_add(1);
        if new_format & flag == 0 {
            continue;
        }
        let at = s.s.u16(off);
        if off == 0 {
            continue;
        }
        s.s.set_u16(at, 0);
        let dev = base.follow(u32::from(off));
        s.serialize_subset(at, 2, |s| device_copy(s, dev));
    }
}

/// `Anchor::subset` into the current object.
fn anchor(s: &mut Sub<'_, '_>, a: D<'_>) -> bool {
    match a.u16(0) {
        1 => {
            s.s.u16(1);
            s.s.extend(a.slice(2, 4));
            a.0.len() >= 6
        }
        2 => {
            if strip_hints(s) {
                s.s.u16(1);
                s.s.extend(a.slice(2, 4));
                return a.0.len() >= 6;
            }
            s.s.extend(a.slice(0, 8));
            a.0.len() >= 8
        }
        3 => {
            s.s.extend(a.slice(0, 6));
            let x = a.off16(6);
            let y = a.off16(8);
            if !super::gdef::adjust_coordinate(s, 2, a.u16(2), x)
                || !super::gdef::adjust_coordinate(s, 4, a.u16(4), y)
            {
                return false;
            }
            let is_var = super::gdef::is_variation_device;
            let mapped = |dev: D<'_>| {
                is_var(dev)
                    && s.p
                        .layout
                        .gdef_var_map
                        .contains_key(&((u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2))))
            };
            if !s.p.layout.varidx_pinned
                && s.p.layout.instanced_gdef_store.is_none()
                && ((is_var(x) && !mapped(x)) || (is_var(y) && !mapped(y)))
            {
                return false;
            }
            let hinting = |d: D<'_>| !d.is_null() && !is_var(d);
            let no_downgrade = hinting(x) || hinting(y) || mapped(x) || mapped(y);
            if !no_downgrade {
                s.s.set_u16(0, 1);
                return true;
            }
            let x_at = s.s.u16(0);
            let y_at = s.s.u16(0);
            s.serialize_copy(x_at, 2, x, |s| device_copy(s, x));
            s.serialize_copy(y_at, 2, y, |s| device_copy(s, y));
            true
        }
        _ => false,
    }
}

/// Coverage, value format, record count and first record offset of a SinglePos.
fn single_records(st: D<'_>) -> (Cov<'_>, u16, usize, usize) {
    match st.u16(0) {
        1 => (Cov(st.off16(2)), st.u16(4), 1, 6),
        2 => (Cov(st.off16(2)), st.u16(4), usize::from(st.u16(6)), 8),
        3 => (Cov(st.off32(2)), st.u16(6), 1, 8),
        4 => (Cov(st.off32(2)), st.u16(6), (st.u32(8) >> 8) as usize, 11),
        _ => (Cov(D(&[])), 0, 0, 0),
    }
}

/// `SinglePos::serialize` of `(glyph, values)` into the current object.
fn single_serialize(
    s: &mut Sub<'_, '_>,
    base: D<'_>,
    src_format: u16,
    it: &[(u32, D<'_>)],
    new_format: u16,
) {
    let (cov_at, width, shared) = single_header(s.s, src_format, it, new_format);
    let glyphs: Vec<u32> = it.iter().map(|p| p.0).collect();
    if shared {
        if let Some(&(_, v)) = it.first() {
            copy_values(s, src_format, new_format, base, v);
        }
    } else {
        for &(_, v) in it {
            copy_values(s, src_format, new_format, base, v);
        }
    }
    s.serialize_serialize(cov_at, width, |s| s.coverage(&glyphs));
}

/// Writes the header and reports whether all glyphs share one value record.
fn single_header(
    s: &mut sofel_write::Serializer,
    src_format: u16,
    records: &[(u32, D<'_>)],
    new_format: u16,
) -> (usize, u8, bool) {
    let len = value_len(src_format);
    let shared = records.first().is_some_and(|(_, first)| {
        records
            .iter()
            .all(|(_, v)| (0..len).all(|i| v.u16_at(0, i) == first.u16_at(0, i)))
    });
    let wide = records.iter().any(|p| p.0 > 0xFFFF);
    s.u16(match (wide, shared) {
        (false, true) => 1,
        (false, false) => 2,
        (true, true) => 3,
        (true, false) => 4,
    });
    let (at, width) = if wide { (s.u32(0), 4) } else { (s.u16(0), 2) };
    s.u16(new_format);
    if !shared {
        if wide {
            let count = s.u24(0);
            s.check_assign_u24(
                count,
                records.len() as u64,
                sofel_write::Errors::ARRAY_OVERFLOW,
            );
        } else {
            let count = s.u16(0);
            s.check_assign_u16(
                count,
                records.len() as u64,
                sofel_write::Errors::ARRAY_OVERFLOW,
            );
        }
    }
    (at, width, shared)
}

/// SinglePos formats 1 through 4.
pub(crate) fn single(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let (cov, vf, n, start) = single_records(st);
    let len = value_len(vf);
    let gs = &s.p.layout.glyphset_gsub;
    match st.u16(0) {
        1 | 3 => {
            let values = st.at(start);
            let mut inter = BitSet::new();
            cov.intersect_set(gs, &mut inter);
            let new_format = if s.p.var.instancing() {
                instanced_format(s, vf, st, values)
            } else if strip_hints(s) {
                effective_format(vf, values, true, true)
            } else {
                vf
            };
            let it: Vec<(u32, D<'_>)> = inter.iter().map(|g| (s.map(g), values)).collect();
            single_serialize(s, st, vf, &it, new_format);
            !it.is_empty()
        }
        2 | 4 => {
            let it: Vec<(u32, D<'_>)> = cov
                .glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .filter(|&(_, g)| gs.has(g))
                .map(|(i, g)| {
                    (
                        s.map(g),
                        st.at(start.saturating_add(i.saturating_mul(len).saturating_mul(2))),
                    )
                })
                .collect();
            let new_format = if s.p.var.instancing() {
                it.iter()
                    .fold(0, |f, (_, v)| f | instanced_format(s, vf, st, *v))
            } else if strip_hints(s) {
                it.iter()
                    .fold(0, |f, (_, v)| f | effective_format(vf, *v, true, true))
            } else {
                vf
            };
            single_serialize(s, st, vf, &it, new_format);
            !it.is_empty()
        }
        _ => false,
    }
}

/// `PairPosFormat1::intersects`.
pub(crate) fn pair1_intersects(st: D<'_>, glyphs: &BitSet) -> bool {
    let cov = Cov(st.off16(2));
    let vf1 = st.u16(4);
    let vf2 = st.u16(6);
    let n = usize::from(st.u16(8));
    let set_at = |i: usize| -> D<'_> {
        if i >= n {
            return D(&[]);
        }
        st.off16(10usize.saturating_add(i.saturating_mul(2)))
    };
    let record = 2usize.saturating_add(
        value_len(vf1)
            .saturating_add(value_len(vf2))
            .saturating_mul(2),
    );
    let set_intersects = |ps: D<'_>| {
        (0..usize::from(ps.u16(0))).any(|j| {
            glyphs.has(u32::from(
                ps.u16(2usize.saturating_add(j.saturating_mul(record))),
            ))
        })
    };
    if n as u64 > glyphs.len().saturating_mul(bit_storage(n as u64)) {
        return glyphs.iter().any(|g| {
            cov.get(g)
                .is_some_and(|i| set_intersects(set_at(i as usize)))
        });
    }
    cov.glyphs()
        .into_iter()
        .take(n)
        .enumerate()
        .filter(|&(_, g)| glyphs.has(g))
        .any(|(i, _)| set_intersects(set_at(i)))
}

/// PairPos formats 1 and 2.
pub(crate) fn pair(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    match st.u16(0) {
        1 => pair1(s, st),
        2 | 4 => pair2(s, st),
        _ => false,
    }
}

fn pair1(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let cov = Cov(st.off16(2));
    let vf = [st.u16(4), st.u16(6)];
    let n = usize::from(st.u16(8));
    let len1 = value_len(vf[0]);
    let record = 2usize.saturating_add(len1.saturating_add(value_len(vf[1])).saturating_mul(2));
    let gs = &s.p.layout.glyphset_gsub;
    let sets: Vec<(u32, D<'_>)> = cov
        .glyphs()
        .into_iter()
        .take(n)
        .enumerate()
        .filter(|&(_, g)| gs.has(g))
        .map(|(i, g)| (g, st.off16(10usize.saturating_add(i.saturating_mul(2)))))
        .collect();
    let mut new_vf = vf;
    let instancing = s.p.var.instancing();
    if instancing || strip_hints(s) {
        let (mut f1, mut f2) = (0u16, 0u16);
        for &(_, ps) in &sets {
            for j in 0..usize::from(ps.u16(0)) {
                let r = ps.at(2usize.saturating_add(j.saturating_mul(record)));
                if !gs.has(u32::from(r.u16(0))) {
                    continue;
                }
                if instancing {
                    f1 |= instanced_format(s, vf[0], ps, r.at(2));
                    f2 |= instanced_format(
                        s,
                        vf[1],
                        ps,
                        r.at(2usize.saturating_add(len1.saturating_mul(2))),
                    );
                    continue;
                }
                f1 |= pair_effective_format(s, vf[0], ps, r.at(2));
                f2 |= pair_effective_format(
                    s,
                    vf[1],
                    ps,
                    r.at(2usize.saturating_add(len1.saturating_mul(2))),
                );
            }
            if !instancing && f1 == vf[0] && f2 == vf[1] {
                break;
            }
        }
        new_vf = [f1, f2];
    }
    s.s.u16(1);
    let cov_at = s.s.u16(0);
    s.s.u16(new_vf[0]);
    s.s.u16(new_vf[1]);
    let count_at = s.s.u16(0);
    let mut new_cov = Vec::new();
    for &(g, ps) in &sets {
        if ps.is_null() {
            continue;
        }
        let ok = s.offset_append(2, |s| {
            let count_at = s.s.u16(0);
            let mut num = 0u16;
            for j in 0..usize::from(ps.u16(0)) {
                let r = ps.at(2usize.saturating_add(j.saturating_mul(record)));
                let second = u32::from(r.u16(0));
                if !s.in_gsub(second) {
                    continue;
                }
                s.s.u16((s.map(second) & 0xFFFF) as u16);
                copy_values(s, vf[0], new_vf[0], ps, r.at(2));
                copy_values(
                    s,
                    vf[1],
                    new_vf[1],
                    ps,
                    r.at(2usize.saturating_add(len1.saturating_mul(2))),
                );
                num = num.wrapping_add(1);
            }
            s.s.set_u16(count_at, num);
            num != 0
        });
        if ok {
            s.bump_u16(count_at, 1);
            new_cov.push(s.map(g));
        }
    }
    s.serialize_serialize(cov_at, 2, |s| s.coverage(&new_cov));
    !new_cov.is_empty()
}

/// The two ClassDefs, class counts, and value matrix start of PairPos formats 2 and 4.
fn pair_classes(st: D<'_>) -> (D<'_>, D<'_>, u16, u16, usize) {
    if st.u16(0) == 4 {
        (st.off32(10), st.off32(14), st.u16(18), st.u16(20), 22)
    } else {
        (st.off16(8), st.off16(10), st.u16(12), st.u16(14), 16)
    }
}

fn pair2(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let wide = st.u16(0) == 4;
    let width: u8 = if wide { 4 } else { 2 };
    let cov_d = if wide { st.off32(2) } else { st.off16(2) };
    let vf1 = st.u16(if wide { 6 } else { 4 });
    let vf2 = st.u16(if wide { 8 } else { 6 });
    let (cd1, cd2, n1, n2, start) = pair_classes(st);
    let (class1_count, class2_count) = (u32::from(n1), u32::from(n2));
    let len1 = value_len(vf1);
    let total = len1.saturating_add(value_len(vf2));
    let value_at = |c1: u32, c2: u32| -> D<'_> {
        let idx = (c1 as usize)
            .saturating_mul(class2_count as usize)
            .saturating_add(c2 as usize)
            .saturating_mul(total);
        st.at(start.saturating_add(idx.saturating_mul(2)))
    };
    s.s.u16(st.u16(0));
    let cov_at = s.s.zeros(usize::from(width));
    let vf_at = s.s.u16(vf1);
    s.s.u16(vf2);
    let cd1_at = s.s.zeros(usize::from(width));
    let cd2_at = s.s.zeros(usize::from(width));
    let c1_at = s.s.u16(0);
    let c2_at = s.s.u16(0);
    let mut map1 = BTreeMap::new();
    let cov = Cov(cov_d);
    s.serialize_subset_opt(cd1_at, width, cd1, |s| {
        s.class_def_subset(Cls(cd1), Some(&mut map1), true, true, Some(cov))
    });
    s.s.set_u16(c1_at, n16(map1.len()));
    let mut map2 = BTreeMap::new();
    s.serialize_subset_opt(cd2_at, width, cd2, |s| {
        s.class_def_subset(Cls(cd2), Some(&mut map2), true, false, None)
    });
    s.s.set_u16(c2_at, n16(map2.len()));
    let rows: Vec<u32> = (0..class1_count).filter(|c| map1.contains_key(c)).collect();
    let cols: Vec<u32> = (0..class2_count).filter(|c| map2.contains_key(c)).collect();
    let (mut f1, mut f2) = (vf1, vf2);
    let instancing = s.p.var.instancing();
    if instancing || strip_hints(s) {
        f1 = 0;
        f2 = 0;
        for &c1 in &rows {
            for &c2 in &cols {
                let v = value_at(c1, c2);
                if instancing {
                    f1 |= instanced_format(s, vf1, st, v);
                    f2 |= instanced_format(s, vf2, st, v.at(len1.saturating_mul(2)));
                    continue;
                }
                f1 |= pair_effective_format(s, vf1, st, v);
                f2 |= pair_effective_format(s, vf2, st, v.at(len1.saturating_mul(2)));
            }
            if !instancing && f1 == vf1 && f2 == vf2 {
                break;
            }
        }
        s.s.set_u16(vf_at, f1);
        s.s.set_u16(vf_at.saturating_add(2), f2);
    }
    for &c1 in &rows {
        for &c2 in &cols {
            let v = value_at(c1, c2);
            copy_values(s, vf1, f1, st, v);
            copy_values(s, vf2, f2, st, v.at(len1.saturating_mul(2)));
        }
    }
    let ret = s.serialize_subset_opt(cov_at, width, cov_d, |s| s.coverage_subset(cov));
    !map1.is_empty() && !map2.is_empty() && ret
}

/// Coverage and dimensions of the CursivePos entry/exit record array.
fn cursive_records(st: D<'_>) -> Option<(Cov<'_>, usize, usize, u8)> {
    match st.u16(0) {
        1 => Some((Cov(st.off16(2)), usize::from(st.u16(4)), 6, 2)),
        2 => Some((Cov(st.off32(2)), (st.u32(6) >> 8) as usize, 9, 3)),
        _ => None,
    }
}

fn cursive_anchor(st: D<'_>, at: usize, width: u8) -> D<'_> {
    if width == 3 {
        st.follow(st.u32(at) >> 8)
    } else {
        st.off16(at)
    }
}

/// CursivePos formats 1 and 2.
pub(crate) fn cursive(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let Some((cov, n, start, width)) = cursive_records(st) else {
        return false;
    };
    let gs = &s.p.layout.glyphset_gsub;
    let it: Vec<(u32, usize)> = cov
        .glyphs()
        .into_iter()
        .take(n)
        .enumerate()
        .filter(|&(_, g)| gs.has(g))
        .map(|(i, g)| (s.map(g), i))
        .collect();
    s.s.u16(st.u16(0));
    let cov_width = if width == 3 { 4 } else { 2 };
    let cov_at = s.s.zeros(usize::from(cov_width));
    if width == 3 {
        s.s.u24(n32(it.len()));
    } else {
        s.s.u16(n16(it.len()));
    }
    for &(_, i) in &it {
        let rec = start.saturating_add(i.saturating_mul(usize::from(width).saturating_mul(2)));
        for at in [rec, rec.saturating_add(usize::from(width))] {
            let field = s.s.zeros(usize::from(width));
            let src = cursive_anchor(st, at, width);
            s.serialize_subset_opt(field, width, src, |s| anchor(s, src));
        }
    }
    let glyphs: Vec<u32> = it.iter().map(|p| p.0).collect();
    s.serialize_serialize(cov_at, cov_width, |s| s.coverage(&glyphs));
    !it.is_empty()
}

/// `Markclass_closure_and_remap_indexes`.
fn mark_classes(cov: Cov<'_>, marks: D<'_>, gs: &BitSet) -> BTreeMap<u32, u32> {
    let n = usize::from(marks.u16(0));
    let mut classes = BitSet::new();
    for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
        if gs.has(g) {
            classes.add(u32::from(
                marks.u16(2usize.saturating_add(i.saturating_mul(4))),
            ));
        }
    }
    classes.iter().zip(0u32..).collect()
}

/// `MarkArray::subset` into the current object.
fn mark_array(s: &mut Sub<'_, '_>, marks: D<'_>, cov: Cov<'_>, map: &BTreeMap<u32, u32>) -> bool {
    let count_at = s.s.u16(0);
    let n = usize::from(marks.u16(0));
    let mut ret = false;
    let mut len = 0u16;
    for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
        if !s.in_gsub(g) {
            continue;
        }
        let rec = 2usize.saturating_add(i.saturating_mul(4));
        let class = u32::from(marks.u16(rec));
        s.s.u16((map.get(&class).copied().unwrap_or(crate::set::INVALID) & 0xFFFF) as u16);
        let at = s.s.u16(0);
        let a = marks.off16(rec.saturating_add(2));
        ret |= s.serialize_subset_opt(at, 2, a, |s| anchor(s, a));
        len = len.wrapping_add(1);
    }
    s.s.set_u16(count_at, len);
    ret
}

/// `AnchorMatrix::subset` into the current object.
fn anchor_matrix(s: &mut Sub<'_, '_>, m: D<'_>, rows: u16, indexes: &[usize]) -> bool {
    if indexes.is_empty() {
        return false;
    }
    s.s.u16(rows);
    for &i in indexes {
        let a = m.off16(2usize.saturating_add(i.saturating_mul(2)));
        let at = s.s.u16(0);
        s.serialize_subset_opt(at, 2, a, |s| anchor(s, a));
    }
    true
}

/// `collect_subset_anchor_indexes`: the kept rows of a base or mark2
/// array and their new coverage.
fn anchor_indexes(
    s: &Sub<'_, '_>,
    cov: Cov<'_>,
    m: D<'_>,
    class_count: usize,
    map: &BTreeMap<u32, u32>,
) -> (Vec<u32>, Vec<usize>) {
    let rows = usize::from(m.u16(0));
    let mut new_cov = Vec::new();
    let mut indexes = Vec::new();
    for (row, g) in cov.glyphs().into_iter().take(rows).enumerate() {
        if !s.in_gsub(g) {
            continue;
        }
        let at = |col: usize| row.saturating_mul(class_count).saturating_add(col);
        let non_empty =
            (0..class_count).any(|col| map.contains_key(&n32(col)) && m.u16_at(2, at(col)) != 0);
        if !non_empty {
            continue;
        }
        new_cov.push(s.map(g));
        (0..class_count)
            .filter(|&col| map.contains_key(&n32(col)))
            .for_each(|col| indexes.push(at(col)));
    }
    (new_cov, indexes)
}

/// MarkBasePos and MarkMarkPos format 1 (they share the layout).
fn mark_base_like(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    if st.u16(0) != 1 {
        return false;
    }
    let mark_cov = Cov(st.off16(2));
    let base_cov = Cov(st.off16(4));
    let class_count = usize::from(st.u16(6));
    let marks = st.off16(8);
    let bases = st.off16(10);
    s.s.u16(1);
    let mark_cov_at = s.s.u16(0);
    let base_cov_at = s.s.u16(0);
    let cc_at = s.s.u16(0);
    let marks_at = s.s.u16(0);
    let bases_at = s.s.u16(0);
    let map = mark_classes(mark_cov, marks, &s.p.layout.glyphset_gsub);
    if map.is_empty() {
        return false;
    }
    s.s.set_u16(cc_at, n16(map.len()));
    let n = usize::from(marks.u16(0));
    let new_cov: Vec<u32> = mark_cov
        .glyphs()
        .into_iter()
        .take(n)
        .filter(|&g| s.in_gsub(g))
        .map(|g| s.map(g))
        .collect();
    if !s.serialize_serialize(mark_cov_at, 2, |s| s.coverage(&new_cov)) {
        return false;
    }
    if !s.serialize_subset_opt(marks_at, 2, marks, |s| mark_array(s, marks, mark_cov, &map)) {
        return false;
    }
    let (new_cov, indexes) = anchor_indexes(s, base_cov, bases, class_count, &map);
    if new_cov.is_empty() {
        return false;
    }
    if !s.serialize_serialize(base_cov_at, 2, |s| s.coverage(&new_cov)) {
        return false;
    }
    let rows = n16(new_cov.len());
    s.serialize_subset_opt(bases_at, 2, bases, |s| {
        anchor_matrix(s, bases, rows, &indexes)
    })
}

/// MarkBasePos format 1.
pub(crate) fn mark_base(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    mark_base_like(s, st)
}

/// MarkMarkPos format 1.
pub(crate) fn mark_mark(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    mark_base_like(s, st)
}

/// MarkLigPos format 1.
pub(crate) fn mark_lig(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    if st.u16(0) != 1 {
        return false;
    }
    let mark_cov = Cov(st.off16(2));
    let lig_cov = Cov(st.off16(4));
    let class_count = usize::from(st.u16(6));
    let marks = st.off16(8);
    let ligs = st.off16(10);
    s.s.u16(1);
    let mark_cov_at = s.s.u16(0);
    let lig_cov_at = s.s.u16(0);
    let cc_at = s.s.u16(0);
    let marks_at = s.s.u16(0);
    let ligs_at = s.s.u16(0);
    let map = mark_classes(mark_cov, marks, &s.p.layout.glyphset_gsub);
    if map.is_empty() {
        return false;
    }
    s.s.set_u16(cc_at, n16(map.len()));
    let n = usize::from(marks.u16(0));
    let new_cov: Vec<u32> = mark_cov
        .glyphs()
        .into_iter()
        .take(n)
        .filter(|&g| s.in_gsub(g))
        .map(|g| s.map(g))
        .collect();
    if !s.serialize_serialize(mark_cov_at, 2, |s| s.coverage(&new_cov)) {
        return false;
    }
    if !s.serialize_subset_opt(marks_at, 2, marks, |s| mark_array(s, marks, mark_cov, &map)) {
        return false;
    }
    let mut new_lig_cov = Vec::new();
    let ok = s.serialize_subset_opt(ligs_at, 2, ligs, |s| {
        let count_at = s.s.u16(0);
        let mut ret = false;
        let n = usize::from(ligs.u16(0));
        for (i, g) in lig_cov.glyphs().into_iter().take(n).enumerate() {
            if !s.in_gsub(g) {
                continue;
            }
            let src = ligs.off16(2usize.saturating_add(i.saturating_mul(2)));
            let rows = src.u16(0);
            let cells = usize::from(rows).saturating_mul(class_count);
            let kept = |idx: usize| {
                class_count != 0
                    && map.contains_key(&n32(idx.checked_rem(class_count).unwrap_or(0)))
            };
            let non_empty = (0..cells).any(|idx| kept(idx) && src.u16_at(2, idx) != 0);
            if !non_empty {
                continue;
            }
            let at = s.s.u16(0);
            s.bump_u16(count_at, 1);
            let indexes: Vec<usize> = (0..cells).filter(|&idx| kept(idx)).collect();
            ret |= s.serialize_subset_opt(at, 2, src, |s| anchor_matrix(s, src, rows, &indexes));
            new_lig_cov.push(s.map(g));
        }
        ret
    });
    if !ok {
        return false;
    }
    s.serialize_serialize(lig_cov_at, 2, |s| s.coverage(&new_lig_cov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sofel_write::Serializer;

    #[test]
    fn adr0022_single_position_writes_wide_coverage_offset_and_count() {
        for (second, header) in [
            (D(&[0, 12]), &[0, 3, 0, 0, 0, 0, 0, 4][..]),
            (D(&[0, 24]), &[0, 4, 0, 0, 0, 0, 0, 4, 0, 0, 2][..]),
        ] {
            let mut out = Serializer::new();
            let (_, width, shared) =
                single_header(&mut out, 4, &[(65535, D(&[0, 12])), (65536, second)], 4);
            assert_eq!(out.current(), header);
            assert_eq!(width, 4);
            assert_eq!(shared, second.u16(0) == 12);
        }
    }
}
