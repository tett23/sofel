//! BASE subsetting (`hb-ot-layout-base-table.hh`), including removal of the
//! variation store when every axis is pinned to its default.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::ReadError;
use sofel_font::read::Tag;
use sofel_font::read::base::{Base, BaseCoord, MinMax};
use sofel_write::Serializer;

use super::Tables;
use crate::error::SubsetError;
use crate::layout::common::Sub;
use crate::layout::data::D;
use crate::plan::Plan;

fn coord_uses_variations(coord: Option<BaseCoord<'_>>) -> Result<bool, ReadError> {
    match coord {
        // Unknown Device formats also block this shortcut.
        Some(coord) => Ok(coord
            .device()?
            .is_some_and(|device| !matches!(device.delta_format(), 1..=3))),
        None => Ok(false),
    }
}

fn min_max_uses_variations(min_max: MinMax<'_>) -> Result<bool, ReadError> {
    if coord_uses_variations(min_max.min_coord()?)? || coord_uses_variations(min_max.max_coord()?)?
    {
        return Ok(true);
    }
    for index in 0..min_max.num_features() {
        if coord_uses_variations(min_max.feature_min_coord(index)?)?
            || coord_uses_variations(min_max.feature_max_coord(index)?)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A BASE store can be removed without remapping indices only when no
/// coordinate in either axis references a VariationIndex.
pub(crate) fn unused_variation_store(src: &[u8]) -> bool {
    let inspect = || -> Result<bool, ReadError> {
        let base = Base::new(src)?;
        if base.variation_store()?.is_none() {
            return Ok(false);
        }
        for vertical in [false, true] {
            let Some(axis) = base.axis(vertical)? else {
                continue;
            };
            let Some(scripts) = axis.base_script_list()? else {
                continue;
            };
            for index in 0..scripts.len() {
                let Some(script) = scripts.script(index)? else {
                    continue;
                };
                if let Some(values) = script.base_values()? {
                    for index in 0..values.len() {
                        if coord_uses_variations(values.coord(index)?)? {
                            return Ok(false);
                        }
                    }
                }
                if let Some(min_max) = script.default_min_max()?
                    && min_max_uses_variations(min_max)?
                {
                    return Ok(false);
                }
                for index in 0..script.num_lang_sys() {
                    if let Some(min_max) = script.lang_sys_min_max(index)?
                        && min_max_uses_variations(min_max)?
                    {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    };
    inspect().unwrap_or(false)
}

type RetainedVariations = (Vec<(u16, Vec<u32>)>, BTreeMap<u32, u32>);

fn collect_coord(coord: Option<BaseCoord<'_>>, indices: &mut BTreeSet<u32>) -> Option<()> {
    let Some(coord) = coord else { return Some(()) };
    let Some(device) = coord.device().ok()? else {
        return Some(());
    };
    match device.delta_format() {
        1..=3 => Some(()),
        0x8000 => {
            let (outer, inner) = device.variation_index()?;
            indices.insert((u32::from(outer) << 16) | u32::from(inner));
            Some(())
        }
        _ => None,
    }
}

fn collect_min_max(m: MinMax<'_>, p: &Plan, indices: &mut BTreeSet<u32>) -> Option<()> {
    collect_coord(m.min_coord().ok()?, indices)?;
    collect_coord(m.max_coord().ok()?, indices)?;
    for i in 0..m.num_features() {
        let (tag, _, _) = m.feature_record(i)?;
        if p.layout.layout_features.contains(tag.to_u32()) {
            collect_coord(m.feature_min_coord(i).ok()?, indices)?;
            collect_coord(m.feature_max_coord(i).ok()?, indices)?;
        }
    }
    Some(())
}

pub(crate) fn retained_variation_indices(src: &[u8], p: &Plan) -> Option<RetainedVariations> {
    let base = Base::new(src).ok()?;
    let store = base.variation_store().ok()??;
    let mut selected = BTreeSet::new();
    for vertical in [false, true] {
        let Some(axis) = base.axis(vertical).ok()? else {
            continue;
        };
        let Some(scripts) = axis.base_script_list().ok()? else {
            continue;
        };
        for i in 0..scripts.len() {
            let (tag, _) = scripts.record(i)?;
            if !p.layout.layout_scripts.contains(tag.to_u32()) {
                continue;
            }
            let Some(script) = scripts.script(i).ok()? else {
                continue;
            };
            if let Some(values) = script.base_values().ok()? {
                for j in 0..values.len() {
                    collect_coord(values.coord(j).ok()?, &mut selected)?;
                }
            }
            if let Some(m) = script.default_min_max().ok()? {
                collect_min_max(m, p, &mut selected)?;
            }
            for j in 0..script.num_lang_sys() {
                if let Some(m) = script.lang_sys_min_max(j).ok()? {
                    collect_min_max(m, p, &mut selected)?;
                }
            }
        }
    }
    let mut grouped: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for index in selected {
        let outer = u16::try_from(index >> 16).ok()?;
        let inner = u16::try_from(index & 0xffff).ok()?;
        if inner >= store.item_data(outer)?.item_count {
            return None;
        }
        grouped.entry(outer).or_default().push(u32::from(inner));
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

fn device_copy(s: &mut Sub<'_, '_>, dev: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    if dev.u16(4) == 0x8000 {
        let old = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
        let Some(&new) = remap.get(&old) else {
            return false;
        };
        s.s.u16(u16::try_from(new >> 16).unwrap_or(0));
        s.s.u16(u16::try_from(new & 0xffff).unwrap_or(0));
        s.s.u16(0x8000);
        return true;
    }
    if !matches!(dev.u16(4), 1..=3) {
        return false;
    }
    let start = dev.u16(0);
    let end = dev.u16(2);
    let f = dev.u16(4);
    let size = if start > end {
        6
    } else {
        2usize.saturating_mul(
            4usize.saturating_add(usize::from(end.wrapping_sub(start) >> 4u16.wrapping_sub(f))),
        )
    };
    s.s.extend(dev.slice(0, size));
    true
}

/// `BaseCoord::subset`.
fn base_coord(s: &mut Sub<'_, '_>, c: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    match c.u16(0) {
        1 => {
            s.s.extend(c.slice(0, 4));
            true
        }
        2 | 4 => {
            let wide = c.u16(0) == 4;
            let reference = if wide {
                c.u32(4) >> 8
            } else {
                u32::from(c.u16(4))
            };
            let Some(g) = s.p.glyphs.new_gid(reference) else {
                return false;
            };
            s.s.extend(c.slice(0, if wide { 9 } else { 8 }));
            if wide {
                s.s.check_assign_u24(4, u64::from(g), sofel_write::Errors::INT_OVERFLOW)
            } else {
                s.s.check_assign_u16(4, u64::from(g), sofel_write::Errors::INT_OVERFLOW)
            }
        }
        3 => {
            s.s.extend(c.slice(0, 4));
            let dev = c.off16(4);
            if s.p.var.instancing() && !s.p.var.n.pinned_at_default && dev.u16(4) == 0x8000 {
                let index = (u32::from(dev.u16(0)) << 16) | u32::from(dev.u16(2));
                let Some(&delta) = s.p.layout.pinned_base_deltas.get(&index) else {
                    return false;
                };
                let value = i32::from(c.u16(2).cast_signed()).saturating_add(delta);
                let Ok(value) = i16::try_from(value) else {
                    return s.s.err(sofel_write::Errors::INT_OVERFLOW);
                };
                s.s.set_u16(2, value.cast_unsigned());
            }
            let at = s.s.u16(0);
            s.serialize_copy(at, 2, dev, |s| device_copy(s, dev, remap));
            true
        }
        _ => false,
    }
}

/// `MinMax::subset`.
fn min_max(s: &mut Sub<'_, '_>, m: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    let min = m.off16(0);
    let max = m.off16(2);
    s.s.u16(0);
    s.s.u16(0);
    let count_at = s.s.u16(0);
    let mut ret = s.serialize_subset_opt(0, 2, min, |s| base_coord(s, min, remap));
    ret |= s.serialize_subset_opt(2, 2, max, |s| base_coord(s, max, remap));
    let mut len = 0u16;
    for i in 0..usize::from(m.u16(4)) {
        let rec = 6usize.saturating_add(i.saturating_mul(8));
        let tag = m.u32(rec);
        if !s.p.layout.layout_features.contains(tag) {
            continue;
        }
        let snap = s.s.snapshot();
        s.s.u32(tag);
        let min_at = s.s.u16(0);
        let max_at = s.s.u16(0);
        let fmin = m.off16(rec.saturating_add(4));
        let fmax = m.off16(rec.saturating_add(6));
        let mut ok = s.serialize_subset_opt(min_at, 2, fmin, |s| base_coord(s, fmin, remap));
        ok |= s.serialize_subset_opt(max_at, 2, fmax, |s| base_coord(s, fmax, remap));
        if ok {
            len = len.wrapping_add(1);
        } else {
            s.s.revert(snap);
        }
    }
    if len > 0 {
        ret = true;
    }
    if !ret {
        return false;
    }
    s.s.set_u16(count_at, len);
    true
}

/// `BaseValues::subset`.
fn base_values(s: &mut Sub<'_, '_>, v: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    s.s.u16(v.u16(0));
    let count_at = s.s.u16(0);
    let mut ret = false;
    for i in 0..usize::from(v.u16(2)) {
        let c = v.off16(4usize.saturating_add(i.saturating_mul(2)));
        let at = s.s.u16(0);
        s.bump_u16(count_at, 1);
        ret |= s.serialize_subset_opt(at, 2, c, |s| base_coord(s, c, remap));
    }
    ret
}

/// `BaseScript::subset`.
fn base_script(s: &mut Sub<'_, '_>, b: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    s.s.u16(0);
    s.s.u16(0);
    let count_at = s.s.u16(0);
    let values = b.off16(0);
    let mut ret = s.serialize_subset_opt(0, 2, values, |s| base_values(s, values, remap));
    let dmm = b.off16(2);
    ret |= s.serialize_subset_opt(2, 2, dmm, |s| min_max(s, dmm, remap));
    let mut len = 0u16;
    for i in 0..usize::from(b.u16(4)) {
        let rec = 6usize.saturating_add(i.saturating_mul(6));
        let snap = s.s.snapshot();
        s.s.u32(b.u32(rec));
        let at = s.s.u16(0);
        let mm = b.off16(rec.saturating_add(4));
        if s.serialize_subset_opt(at, 2, mm, |s| min_max(s, mm, remap)) {
            ret = true;
            len = len.wrapping_add(1);
        } else {
            s.s.revert(snap);
        }
    }
    if !ret {
        return false;
    }
    s.s.set_u16(count_at, len);
    true
}

/// `BaseScriptList::subset`.
fn base_script_list(s: &mut Sub<'_, '_>, l: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    let count_at = s.s.u16(0);
    let mut len = 0u16;
    for i in 0..usize::from(l.u16(0)) {
        let rec = 2usize.saturating_add(i.saturating_mul(6));
        let tag = l.u32(rec);
        if !s.p.layout.layout_scripts.contains(tag) {
            continue;
        }
        let snap = s.s.snapshot();
        s.s.u32(tag);
        let at = s.s.u16(0);
        let bs = l.off16(rec.saturating_add(4));
        if s.serialize_subset_opt(at, 2, bs, |s| base_script(s, bs, remap)) {
            len = len.wrapping_add(1);
        } else {
            s.s.revert(snap);
        }
    }
    if len == 0 {
        return false;
    }
    s.s.set_u16(count_at, len);
    true
}

/// `Axis::subset`.
fn axis(s: &mut Sub<'_, '_>, a: D<'_>, remap: &BTreeMap<u32, u32>) -> bool {
    s.s.u16(0);
    s.s.u16(0);
    let tags = a.off16(0);
    let mut ret = s.serialize_copy(0, 2, tags, |s| {
        let n = usize::from(tags.u16(0));
        s.s.extend(tags.slice(0, 2usize.saturating_add(n.saturating_mul(4))));
        true
    });
    let list = a.off16(2);
    ret |= s.serialize_subset_opt(2, 2, list, |s| base_script_list(s, list, remap));
    ret
}

/// `BASE::subset`.
pub(crate) fn subset(
    p: &Plan,
    font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let d = D(src);
    let version = d.u32(0);
    let mut remap = BTreeMap::new();
    let mut store_bytes = None;
    if version >= 0x0001_0001
        && d.u32(8) != 0
        && !p.var.all_axes_pinned()
        && (p.var.instancing() || !unused_variation_store(src))
    {
        let Some((old_items, new_indices)) = retained_variation_indices(src, p) else {
            return Err(SubsetError::Unsupported {
                tag,
                reason: "the BASE item variation store (ADR 0022 stage 10)",
            });
        };
        remap = new_indices;
        if p.var.instancing() || !old_items.is_empty() {
            let store = Base::new(src)
                .ok()
                .and_then(|base| base.variation_store().ok().flatten())
                .ok_or(SubsetError::Malformed { tag })?;
            if p.var.instancing() {
                let items = if old_items.is_empty() {
                    (0..store.data_count())
                        .map(|outer| {
                            let data = store
                                .item_data(outer)
                                .ok_or(SubsetError::Malformed { tag })?;
                            Ok((outer, (0..u32::from(data.item_count)).collect()))
                        })
                        .collect::<Result<Vec<_>, SubsetError>>()?
                } else {
                    old_items
                };
                let mut projected = BTreeMap::new();
                let bytes = crate::tables::hvar::instance::subset(
                    p,
                    font,
                    store,
                    &items,
                    tag,
                    true,
                    &mut projected,
                )?;
                remap.retain(|_, index| {
                    projected.get(index).is_some_and(|new| {
                        *index = *new;
                        true
                    })
                });
                if !bytes.is_empty() {
                    store_bytes = Some(bytes);
                }
            } else {
                store_bytes = Some(crate::tables::hvar::subset_store(store, &old_items, tag)?);
            }
        }
    }
    let mut c = Sub::new(p, s, false);
    c.s.u32(version);
    c.s.u16(0);
    c.s.u16(0);
    let mut ret = false;
    if version >= 0x0001_0001 {
        c.s.u32(0);
        // BASE::subset_varstore keeps the offset field even with no Axis.
        ret = true;
        if let Some(bytes) = store_bytes {
            c.serialize_serialize(8, 4, |s| {
                s.s.extend(&bytes);
                true
            });
        }
    }
    let h = d.off16(4);
    ret |= c.serialize_subset_opt(4, 2, h, |s| axis(s, h, &remap));
    let v = d.off16(6);
    ret |= c.serialize_subset_opt(6, 2, v, |s| axis(s, v, &remap));
    if let Some(reason) = c.unsupported {
        return Err(SubsetError::Unsupported { tag, reason });
    }
    Ok(ret)
}
