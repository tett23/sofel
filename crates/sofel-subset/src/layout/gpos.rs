//! The subsetting of GPOS's own subtables (`OT/Layout/GPOS/`): value
//! records and devices, anchors, single, pair, cursive and mark
//! attachment positioning.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::common::Sub;
use super::data::{Cls, Cov, D, bit_storage, n16, n32};
use crate::bits::BitSet;
use crate::input::Flags;

/// The number of 16-bit values of a ValueRecord of `format`.
fn value_len(format: u16) -> usize {
    (format & 0xFF).count_ones() as usize
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

/// Whether the hints of value records are stripped: with `NO_HINTING`
/// (a variable font keeps them only with a GDEF variation store, which
/// this version does not subset).
fn strip_hints(s: &Sub<'_, '_>) -> bool {
    s.p.flags.has(Flags::NO_HINTING)
}

/// `Device::copy` into the current object: hinting devices are copied,
/// variation devices are dropped (no variation indices are kept).
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
            s.s.u16(v);
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
            let is_var = |d: D<'_>| !d.is_null() && d.u16(4) == 0x8000;
            if is_var(x) || is_var(y) {
                return false;
            }
            let no_downgrade = !x.is_null() || !y.is_null();
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

/// `SinglePos::serialize` of `(glyph, values)` into the current object.
fn single_serialize(
    s: &mut Sub<'_, '_>,
    base: D<'_>,
    src_format: u16,
    it: &[(u32, D<'_>)],
    new_format: u16,
) {
    let len = value_len(src_format);
    let mut format = 2u16;
    if let Some(&(_, first)) = it.first() {
        format = if it
            .iter()
            .all(|(_, v)| (0..len).all(|i| v.u16_at(0, i) == first.u16_at(0, i)))
        {
            1
        } else {
            2
        };
    }
    if it.iter().any(|p| p.0 > 0xFFFF) {
        s.unsupported("24-bit SinglePos (ADR 0022 stage 3)");
        return;
    }
    let glyphs: Vec<u32> = it.iter().map(|p| p.0).collect();
    s.s.u16(format);
    let cov_at = s.s.u16(0);
    s.s.u16(new_format);
    if format == 1 {
        if let Some(&(_, v)) = it.first() {
            copy_values(s, src_format, new_format, base, v);
        }
    } else {
        s.s.u16(n16(it.len()));
        for &(_, v) in it {
            copy_values(s, src_format, new_format, base, v);
        }
    }
    s.serialize_serialize(cov_at, 2, |s| s.coverage(&glyphs));
}

/// SinglePos formats 1 and 2.
pub(crate) fn single(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let cov = Cov(st.off16(2));
    let vf = st.u16(4);
    let len = value_len(vf);
    let gs = &s.p.layout.glyphset_gsub;
    match st.u16(0) {
        1 => {
            let values = st.at(6);
            let mut inter = BitSet::new();
            cov.intersect_set(gs, &mut inter);
            let new_format = if strip_hints(s) {
                effective_format(vf, values, true, true)
            } else {
                vf
            };
            let it: Vec<(u32, D<'_>)> = inter.iter().map(|g| (s.map(g), values)).collect();
            single_serialize(s, st, vf, &it, new_format);
            !it.is_empty()
        }
        2 => {
            let n = usize::from(st.u16(6));
            let it: Vec<(u32, D<'_>)> = cov
                .glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .filter(|&(_, g)| gs.has(g))
                .map(|(i, g)| {
                    (
                        s.map(g),
                        st.at(8usize.saturating_add(i.saturating_mul(len).saturating_mul(2))),
                    )
                })
                .collect();
            let new_format = if strip_hints(s) {
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
        2 => pair2(s, st),
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
    if strip_hints(s) {
        let (mut f1, mut f2) = (0u16, 0u16);
        for &(_, ps) in &sets {
            for j in 0..usize::from(ps.u16(0)) {
                let r = ps.at(2usize.saturating_add(j.saturating_mul(record)));
                if !gs.has(u32::from(r.u16(0))) {
                    continue;
                }
                f1 |= effective_format(vf[0], r.at(2), true, true);
                f2 |= effective_format(
                    vf[1],
                    r.at(2usize.saturating_add(len1.saturating_mul(2))),
                    true,
                    true,
                );
            }
            if f1 == vf[0] && f2 == vf[1] {
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

fn pair2(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let cov_d = st.off16(2);
    let vf1 = st.u16(4);
    let vf2 = st.u16(6);
    let cd1 = st.off16(8);
    let cd2 = st.off16(10);
    let class1_count = u32::from(st.u16(12));
    let class2_count = u32::from(st.u16(14));
    let len1 = value_len(vf1);
    let total = len1.saturating_add(value_len(vf2));
    let value_at = |c1: u32, c2: u32| -> D<'_> {
        let idx = (c1 as usize)
            .saturating_mul(class2_count as usize)
            .saturating_add(c2 as usize)
            .saturating_mul(total);
        st.at(16usize.saturating_add(idx.saturating_mul(2)))
    };
    s.s.u16(2);
    let cov_at = s.s.u16(0);
    let vf_at = s.s.u16(vf1);
    s.s.u16(vf2);
    let cd1_at = s.s.u16(0);
    let cd2_at = s.s.u16(0);
    let c1_at = s.s.u16(0);
    let c2_at = s.s.u16(0);
    let mut map1 = BTreeMap::new();
    let cov = Cov(cov_d);
    s.serialize_subset_opt(cd1_at, 2, cd1, |s| {
        s.class_def_subset(Cls(cd1), Some(&mut map1), true, true, Some(cov))
    });
    s.s.set_u16(c1_at, n16(map1.len()));
    let mut map2 = BTreeMap::new();
    s.serialize_subset_opt(cd2_at, 2, cd2, |s| {
        s.class_def_subset(Cls(cd2), Some(&mut map2), true, false, None)
    });
    s.s.set_u16(c2_at, n16(map2.len()));
    let rows: Vec<u32> = (0..class1_count).filter(|c| map1.contains_key(c)).collect();
    let cols: Vec<u32> = (0..class2_count).filter(|c| map2.contains_key(c)).collect();
    let (mut f1, mut f2) = (vf1, vf2);
    if strip_hints(s) {
        f1 = 0;
        f2 = 0;
        for &c1 in &rows {
            for &c2 in &cols {
                let v = value_at(c1, c2);
                f1 |= effective_format(vf1, v, true, true);
                f2 |= effective_format(vf2, v.at(len1.saturating_mul(2)), true, true);
            }
            if f1 == vf1 && f2 == vf2 {
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
    let ret = s.serialize_subset_opt(cov_at, 2, cov_d, |s| s.coverage_subset(cov));
    !map1.is_empty() && !map2.is_empty() && ret
}

/// CursivePos format 1.
pub(crate) fn cursive(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    if st.u16(0) != 1 {
        return false;
    }
    let cov = Cov(st.off16(2));
    let n = usize::from(st.u16(4));
    let gs = &s.p.layout.glyphset_gsub;
    let it: Vec<(u32, usize)> = cov
        .glyphs()
        .into_iter()
        .take(n)
        .enumerate()
        .filter(|&(_, g)| gs.has(g))
        .map(|(i, g)| (s.map(g), i))
        .collect();
    s.s.u16(1);
    let cov_at = s.s.u16(0);
    s.s.u16(n16(it.len()));
    for &(_, i) in &it {
        let rec = 6usize.saturating_add(i.saturating_mul(4));
        let entry_at = s.s.u16(0);
        let exit_at = s.s.u16(0);
        let entry = st.off16(rec);
        let exit = st.off16(rec.saturating_add(2));
        s.serialize_subset_opt(entry_at, 2, entry, |s| anchor(s, entry));
        s.serialize_subset_opt(exit_at, 2, exit, |s| anchor(s, exit));
    }
    let glyphs: Vec<u32> = it.iter().map(|p| p.0).collect();
    s.serialize_serialize(cov_at, 2, |s| s.coverage(&glyphs));
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
