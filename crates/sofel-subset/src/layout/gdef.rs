//! GDEF subsetting (`OT/Layout/GDEF/GDEF.hh`), versions 1.0 to 1.4, and the mark glyph sets the plan keeps
//! (`remap_used_mark_sets`).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::read::Severity;
use sofel_font::read::Tag;
use sofel_font::read::gdef::{CaretValue, Gdef};
use sofel_write::Whence;

use super::common::Sub;
use super::data::{Cls, Cov, D, n32};
use crate::bits::BitSet;

// In GDEF 1.4 a nonzero extended offset supersedes its 16-bit counterpart.
fn subtable(d: D<'_>, short: usize, extended: usize) -> (usize, u8, D<'_>) {
    if d.u32(0) == 0x0001_0004 && d.u32(extended) != 0 {
        (extended, 4, d.off32(extended))
    } else {
        (short, 2, d.off16(short))
    }
}

/// `MarkGlyphSets::collect_used_mark_sets` and `remap_indexes`: the mark
/// glyph sets that cover a glyph of `glyphs`, numbered anew.
pub(crate) fn used_mark_sets(gdef: &[u8], glyphs: &BitSet) -> BTreeMap<u32, u32> {
    let d = D(gdef);
    let version = d.u32(0);
    if version < 0x0001_0002 {
        return BTreeMap::new();
    }
    let (_, _, sets) = subtable(d, 12, 34);
    if sets.is_null() || sets.u16(0) != 1 {
        return BTreeMap::new();
    }
    let n = usize::from(sets.u16(2));
    (0..n)
        .filter(|&i| Cov(sets.off32(4usize.saturating_add(i.saturating_mul(4)))).intersects(glyphs))
        .zip(0u32..)
        .map(|(i, new)| (n32(i), new))
        .collect()
}

/// Whether GDEF has an item variation store (`has_var_store`).
pub(crate) fn has_var_store(gdef: &[u8]) -> bool {
    let d = D(gdef);
    d.u32(0) >= 0x0001_0003 && d.u32(14) != 0
}

type RetainedVariations = (Vec<(u16, Vec<u32>)>, BTreeMap<u32, u32>);

pub(crate) fn retained_variation_indices(
    src: &[u8],
    glyphs: &BitSet,
    extra: &BTreeSet<u32>,
) -> Option<RetainedVariations> {
    let gdef = Gdef::new(src).ok()?;
    let store = gdef.variation_store().ok()??;
    let mut invalid = false;
    sofel_font::read::gdef::validate(src, &mut |d| {
        invalid |= d.severity == Severity::Error;
    });
    if invalid {
        return None;
    }
    let mut selected = extra.clone();
    for &index in extra {
        let data = store.item_data(u16::try_from(index >> 16).ok()?)?;
        if u16::try_from(index & 0xffff).ok()? >= data.item_count {
            return None;
        }
    }
    if let Some(list) = gdef.lig_caret_list().ok()? {
        let (_, width, caret_list) = subtable(D(src), 8, 26);
        let coverage = caret_coverage(caret_list, width == 4);
        for (i, glyph) in coverage.glyphs().into_iter().take(list.len()).enumerate() {
            if !glyphs.has(glyph) {
                continue;
            }
            let Some(ligature) = list.lig_glyph(i).ok()? else {
                continue;
            };
            for j in 0..ligature.len() {
                if let Some(CaretValue::Device(_, Some(device))) = ligature.caret(j).ok()? {
                    if matches!(device.delta_format(), 1..=3) {
                        continue;
                    }
                    if device.delta_format() != 0x8000 {
                        return None;
                    }
                    let index =
                        (u32::from(device.start_size()) << 16) | u32::from(device.end_size());
                    let outer = u16::try_from(index >> 16).ok()?;
                    let inner = u16::try_from(index & 0xffff).ok()?;
                    let data = store.item_data(outer)?;
                    if inner >= data.item_count {
                        return None;
                    }
                    selected.insert(index);
                }
            }
        }
    }
    let mut grouped: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for index in selected {
        let outer = u16::try_from(index >> 16).ok()?;
        grouped.entry(outer).or_default().push(index & 0xffff);
    }
    let old_items: Vec<_> = grouped.into_iter().collect();
    let mut remap = BTreeMap::new();
    for (new_outer, (old_outer, rows)) in old_items.iter().enumerate() {
        for (new_inner, old_inner) in rows.iter().enumerate() {
            let old = (u32::from(*old_outer) << 16) | *old_inner;
            let new = (u32::try_from(new_outer).ok()? << 16) | u32::try_from(new_inner).ok()?;
            remap.insert(old, new);
        }
    }
    Some((old_items, remap))
}

fn attach_list(s: &mut Sub<'_, '_>, al: D<'_>) -> bool {
    let cov = Cov(al.off16(0));
    let cov_at = s.s.u16(0);
    let count_at = s.s.u16(0);
    let n = usize::from(al.u16(2));
    let mut new_cov = Vec::new();
    for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
        if !s.in_gsub(g) {
            continue;
        }
        let p = al.off16(4usize.saturating_add(i.saturating_mul(2)));
        if p.is_null() {
            continue;
        }
        if s.offset_append(2, |s| {
            let m = usize::from(p.u16(0));
            s.s.extend(p.slice(0, 2usize.saturating_add(m.saturating_mul(2))));
            true
        }) {
            s.bump_u16(count_at, 1);
            new_cov.push(s.map(g));
        }
    }
    s.serialize_serialize(cov_at, 2, |s| s.coverage(&new_cov));
    !new_cov.is_empty()
}

/// Whether `dev` is a VariationIndex table (`is_variation_device`).
pub(crate) fn is_variation_device(dev: D<'_>) -> bool {
    !dev.is_null() && dev.u16(4) == 0x8000
}

pub(crate) fn pinned_delta(p: &crate::plan::Plan, dev: D<'_>) -> i32 {
    if !is_variation_device(dev) {
        return 0;
    }
    let index = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
    p.layout
        .pinned_layout_deltas
        .get(&index)
        .copied()
        .unwrap_or(0)
}

pub(crate) fn adjust_coordinate(s: &mut Sub<'_, '_>, at: usize, value: u16, device: D<'_>) -> bool {
    let adjusted = i32::from(value.cast_signed()).saturating_add(pinned_delta(s.p, device));
    let Ok(result) = i16::try_from(adjusted) else {
        return s.s.err(sofel_write::Errors::INT_OVERFLOW);
    };
    s.s.set_u16(at, result.cast_unsigned());
    true
}

fn caret_value(s: &mut Sub<'_, '_>, c: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    match c.u16(0) {
        1 | 2 => {
            s.s.extend(c.slice(0, 4));
            true
        }
        3 => {
            s.s.extend(c.slice(0, 4));
            let dev = c.off16(4);
            let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
            if !adjust_coordinate(s, 2, c.u16(2), dev) {
                return false;
            }
            if is_variation_device(dev)
                && (s.p.layout.varidx_pinned
                    || (s.p.layout.instanced_gdef_store.is_some() && !remap.contains_key(&old)))
            {
                // Mapped to no variations: downgraded to format 1.
                s.s.set_u16(0, 1);
                return true;
            }
            let at = s.s.u16(0);
            s.serialize_copy(at, 2, dev, |s| {
                if is_variation_device(dev) {
                    let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
                    let Some(&new) = remap.get(&old) else {
                        s.unsupported("the GDEF variation index (ADR 0022 stage 10)");
                        return false;
                    };
                    s.s.u16(u16::try_from(new >> 16).unwrap_or(0));
                    s.s.u16(u16::try_from(new & 0xffff).unwrap_or(0));
                    s.s.u16(0x8000);
                    return true;
                }
                if matches!(dev.u16(4), 1..=3) {
                    let start = dev.u16(0);
                    let end = dev.u16(2);
                    let f = dev.u16(4);
                    let size = if start > end {
                        6
                    } else {
                        2usize.saturating_mul(4usize.saturating_add(usize::from(
                            end.wrapping_sub(start) >> 4u16.wrapping_sub(f),
                        )))
                    };
                    s.s.extend(dev.slice(0, size));
                    true
                } else {
                    false
                }
            })
        }
        _ => false,
    }
}

fn caret_coverage(list: D<'_>, wide: bool) -> Cov<'_> {
    Cov(if wide { list.off32(0) } else { list.off16(0) })
}

fn lig_caret_list(s: &mut Sub<'_, '_>, lcl: D<'_>, wide: bool, remap: &BTreeMap<u32, u32>) -> bool {
    let cov = caret_coverage(lcl, wide);
    let (cov_width, width) = if wide { (4u8, 3u8) } else { (2, 2) };
    let cov_at = s.s.zeros(usize::from(cov_width));
    let count_at = s.s.zeros(usize::from(width));
    let n = if wide {
        sofel_font::read::be::u24_at(lcl.0, 4).unwrap_or(0) as usize
    } else {
        usize::from(lcl.u16(2))
    };
    let mut new_cov = Vec::new();
    for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
        if !s.in_gsub(g) {
            continue;
        }
        let lg = if wide {
            let at = 7usize.saturating_add(i.saturating_mul(3));
            lcl.follow(sofel_font::read::be::u24_at(lcl.0, at).unwrap_or(0))
        } else {
            lcl.off16(4usize.saturating_add(i.saturating_mul(2)))
        };
        if lg.is_null() {
            continue;
        }
        if s.offset_append(width, |s| {
            let count_at = s.s.u16(0);
            let mut k = 0u16;
            for j in 0..usize::from(lg.u16(0)) {
                let c = lg.off16(2usize.saturating_add(j.saturating_mul(2)));
                if c.is_null() {
                    continue;
                }
                if s.offset_append(2, |s| caret_value(s, c, remap)) {
                    k = k.wrapping_add(1);
                }
            }
            s.s.set_u16(count_at, k);
            k != 0
        }) {
            new_cov.push(s.map(g));
        }
    }
    if wide {
        s.s.set_u24(count_at, n32(new_cov.len()));
    } else {
        s.s.check_assign_u16(
            count_at,
            new_cov.len() as u64,
            sofel_write::Errors::ARRAY_OVERFLOW,
        );
    }
    s.serialize_serialize(cov_at, cov_width, |s| s.coverage(&new_cov));
    !new_cov.is_empty()
}

fn mark_glyph_sets(s: &mut Sub<'_, '_>, m: D<'_>) -> bool {
    if m.u16(0) != 1 {
        return false;
    }
    s.s.u16(1);
    let count_at = s.s.u16(0);
    let mut n = 0u16;
    for i in 0..usize::from(m.u16(2)) {
        let c = m.off32(4usize.saturating_add(i.saturating_mul(4)));
        let snap = s.s.snapshot();
        let at = s.s.u32(0);
        s.s.push();
        let res = !c.is_null() && s.coverage_subset(Cov(c));
        if !res {
            s.s.pop_discard();
            s.s.revert(snap);
            continue;
        }
        let o = s.s.pop_pack(true);
        s.s.add_link(at, 4, false, o, Whence::Head, 0);
        n = n.wrapping_add(1);
    }
    s.s.set_u16(count_at, n);
    n != 0
}

/// `GDEF::subset` into the root object.
pub(crate) fn subset(s: &mut Sub<'_, '_>, gdef: &[u8]) -> bool {
    let d = D(gdef);
    let version = d.u32(0);
    if version >> 16 != 1 {
        return false;
    }
    if version > 0x0001_0004 || (version == 0x0001_0004 && gdef.len() < 38) {
        s.unsupported("GDEF 1.4 (ADR 0022 stage 3)");
        return false;
    }
    let remap = &s.p.layout.gdef_var_map;
    let mut store_bytes =
        s.p.layout
            .instanced_gdef_store
            .clone()
            .filter(|bytes| !bytes.is_empty());
    if version >= 0x0001_0003
        && d.u32(14) != 0
        && !s.p.var.all_axes_pinned()
        && !s.p.layout.gdef_var_items.is_empty()
    {
        let old_items = &s.p.layout.gdef_var_items;
        if !old_items.is_empty() {
            let Some(store) = Gdef::new(gdef)
                .ok()
                .and_then(|view| view.variation_store().ok().flatten())
            else {
                s.unsupported("the GDEF item variation store (ADR 0022 stage 10)");
                return false;
            };
            match crate::tables::hvar::subset_store(store, old_items, Tag::new(b"GDEF")) {
                Ok(bytes) => store_bytes = Some(bytes),
                Err(_) => {
                    s.unsupported("the GDEF item variation store (ADR 0022 stage 10)");
                    return false;
                }
            }
        }
    }
    s.s.zeros(12);
    let snapshot0 = s.s.snapshot();
    if version >= 0x0001_0002 {
        s.s.u16(0);
    }
    let snapshot2 = s.s.snapshot();
    if version >= 0x0001_0003 {
        s.s.u32(d.u32(14));
    }
    let extended = version == 0x0001_0004 && [18, 22, 26, 30, 34].iter().any(|&at| d.u32(at) != 0);
    if extended {
        s.s.set_u32(14, 0);
        s.s.zeros(20);
    }
    s.s.set_u32(0, version);
    let subset_varstore = store_bytes.is_some_and(|bytes| {
        s.serialize_serialize(14, 4, |s| {
            s.s.extend(&bytes);
            true
        })
    });
    if !subset_varstore && version >= 0x0001_0002 && !extended {
        s.s.revert(snapshot2);
    }
    let mut sets = false;
    if version >= 0x0001_0002 {
        let (at, width, m) = subtable(d, 12, 34);
        sets = s.serialize_subset_opt(at, width, m, |s| mark_glyph_sets(s, m));
    }
    if extended {
        s.s.set_u32(0, 0x0001_0004);
    } else if subset_varstore {
        s.s.set_u32(0, 0x0001_0003);
    } else if sets {
        s.s.set_u32(0, 0x0001_0002);
    } else {
        s.s.set_u32(0, 0x0001_0000);
        s.s.revert(snapshot0);
    }
    let (at, width, gcd) = subtable(d, 4, 18);
    let glyph_classes = s.serialize_subset_opt(at, width, gcd, |s| {
        s.class_def_subset(Cls(gcd), None, false, true, None)
    });
    let (at, width, al) = subtable(d, 6, 22);
    let attach = s.serialize_subset_opt(at, width, al, |s| attach_list(s, al));
    let (at, width, macd) = subtable(d, 10, 30);
    let mark_attach = s.serialize_subset_opt(at, width, macd, |s| {
        s.class_def_subset(Cls(macd), None, false, true, None)
    });
    let (at, width, lcl) = subtable(d, 8, 26);
    let carets = s.serialize_subset_opt(at, width, lcl, |s| {
        lig_caret_list(s, lcl, width == 4, remap)
    });
    glyph_classes || attach || carets || mark_attach || sets || subset_varstore
}
