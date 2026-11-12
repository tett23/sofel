//! VARC auxiliary-index and sparse variation-store subsetting.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::read::varc::Varc;
use sofel_font::read::{GlyphId, Tag};
use sofel_write::{Errors, Serializer};

use crate::error::SubsetError;
use crate::plan::Plan;

use super::{Tables, cff, varc};

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn put_u32(data: &mut [u8], at: usize, value: usize) -> Option<()> {
    let bytes = u32::try_from(value).ok()?.to_be_bytes();
    data.get_mut(at..at.checked_add(4)?)?
        .copy_from_slice(&bytes);
    Some(())
}

fn u24_at(data: &[u8], at: usize) -> Option<usize> {
    Some(
        data.get(at..at.checked_add(3)?)?
            .iter()
            .fold(0usize, |value, &byte| {
                value.saturating_mul(256).saturating_add(usize::from(byte))
            }),
    )
}

fn put_u24(data: &mut [u8], at: usize, value: usize) -> Option<()> {
    let bytes = u32::try_from(value).ok()?.to_be_bytes();
    if value > 0x00ff_ffff {
        return None;
    }
    data.get_mut(at..at.checked_add(3)?)?
        .copy_from_slice(bytes.get(1..)?);
    Some(())
}

#[allow(clippy::too_many_lines)] // Each format serializes its offset fields with its child nodes.
fn append_condition(
    source: &[u8],
    at: usize,
    output: &mut Vec<u8>,
    var_map: &BTreeMap<u32, u32>,
    depth: usize,
) -> Option<usize> {
    if depth == 0 {
        return None;
    }
    let start = output.len();
    match u16_at(source, at)? {
        1 => output.extend_from_slice(source.get(at..at.checked_add(8)?)?),
        2 => {
            output.extend_from_slice(source.get(at..at.checked_add(8)?)?);
            let old = u32_at(source, at.checked_add(4)?)?;
            let new = if old == u32::MAX {
                old
            } else {
                *var_map.get(&old)?
            };
            output
                .get_mut(start.checked_add(4)?..start.checked_add(8)?)?
                .copy_from_slice(&new.to_be_bytes());
        }
        3 | 4 => {
            let count = usize::from(*source.get(at.checked_add(2)?)?);
            let head = 3usize.checked_add(count.checked_mul(3)?)?;
            output.extend_from_slice(source.get(at..at.checked_add(head)?)?);
            for child in (0..count).rev() {
                let field = at.checked_add(3)?.checked_add(child.checked_mul(3)?)?;
                let relative = u24_at(source, field)?;
                if relative == 0 {
                    continue;
                }
                let child_start = append_condition(
                    source,
                    at.checked_add(relative)?,
                    output,
                    var_map,
                    depth.saturating_sub(1),
                )?;
                let new_field = start.checked_add(3)?.checked_add(child.checked_mul(3)?)?;
                put_u24(output, new_field, child_start.checked_sub(start)?)?;
            }
        }
        5 => {
            output.extend_from_slice(source.get(at..at.checked_add(5)?)?);
            let relative = u24_at(source, at.checked_add(2)?)?;
            if relative != 0 {
                let child = append_condition(
                    source,
                    at.checked_add(relative)?,
                    output,
                    var_map,
                    depth.saturating_sub(1),
                )?;
                put_u24(output, start.checked_add(2)?, child.checked_sub(start)?)?;
            }
        }
        _ => return None,
    }
    Some(start)
}

fn subset_conditions(
    src: &[u8],
    selected: &BTreeSet<u32>,
    var_map: &BTreeMap<u32, u32>,
) -> Option<(Vec<u8>, BTreeMap<u32, u32>)> {
    if selected.is_empty() {
        return Some((Vec::new(), BTreeMap::new()));
    }
    let at = usize::try_from(u32_at(src, 12)?).ok()?;
    if at == 0 {
        return None;
    }
    let source = src.get(at..)?;
    let count = usize::try_from(u32_at(source, 0)?).ok()?;
    let header = 4usize.checked_add(selected.len().checked_mul(4)?)?;
    let mut output = alloc::vec![0; header];
    put_u32(&mut output, 0, selected.len())?;
    let mut map = BTreeMap::new();
    for (new, &old) in selected.iter().enumerate() {
        if usize::try_from(old).ok()? >= count {
            return None;
        }
        map.insert(old, u32::try_from(new).ok()?);
    }
    for (&old, &new) in map.iter().rev() {
        let field = 4usize.checked_add(usize::try_from(old).ok()?.checked_mul(4)?)?;
        let offset = usize::try_from(u32_at(source, field)?).ok()?;
        if offset == 0 {
            continue;
        }
        let new_offset = append_condition(source, offset, &mut output, var_map, 64)?;
        let target = 4usize.checked_add(usize::try_from(new).ok()?.checked_mul(4)?)?;
        put_u32(&mut output, target, new_offset)?;
    }
    Some((output, map))
}

fn collect_condition_var_indices(
    source: &[u8],
    at: usize,
    indices: &mut BTreeSet<u32>,
    depth: usize,
) -> Option<()> {
    if depth == 0 {
        return None;
    }
    match u16_at(source, at)? {
        1 => {
            source.get(at..at.checked_add(8)?)?;
        }
        2 => {
            let index = u32_at(source, at.checked_add(4)?)?;
            if index != u32::MAX {
                indices.insert(index);
            }
        }
        3 | 4 => {
            let count = usize::from(*source.get(at.checked_add(2)?)?);
            for child in 0..count {
                let field = at.checked_add(3)?.checked_add(child.checked_mul(3)?)?;
                let relative = u24_at(source, field)?;
                if relative != 0 {
                    collect_condition_var_indices(
                        source,
                        at.checked_add(relative)?,
                        indices,
                        depth.saturating_sub(1),
                    )?;
                }
            }
        }
        5 => {
            let relative = u24_at(source, at.checked_add(2)?)?;
            if relative != 0 {
                collect_condition_var_indices(
                    source,
                    at.checked_add(relative)?,
                    indices,
                    depth.saturating_sub(1),
                )?;
            }
        }
        _ => return None,
    }
    Some(())
}

fn selected_condition_var_indices(
    src: &[u8],
    selected: &BTreeSet<u32>,
    indices: &mut BTreeSet<u32>,
) -> Option<()> {
    if selected.is_empty() {
        return Some(());
    }
    let at = usize::try_from(u32_at(src, 12)?).ok()?;
    let source = src.get(at..)?;
    let count = usize::try_from(u32_at(source, 0)?).ok()?;
    for &condition in selected {
        let condition = usize::try_from(condition).ok()?;
        if condition >= count {
            return None;
        }
        let field = 4usize.checked_add(condition.checked_mul(4)?)?;
        let offset = usize::try_from(u32_at(source, field)?).ok()?;
        if offset != 0 {
            collect_condition_var_indices(source, offset, indices, 64)?;
        }
    }
    Some(())
}

fn index_items(data: &[u8], at: usize) -> Option<Vec<Vec<u8>>> {
    let count = usize::try_from(u32_at(data, at)?).ok()?;
    if count == 0 {
        return Some(Vec::new());
    }
    let offsize = usize::from(*data.get(at.checked_add(4)?)?);
    if !(1..=4).contains(&offsize) || count > data.len() {
        return None;
    }
    let offsets = at.checked_add(5)?;
    let body = offsets.checked_add(count.checked_add(1)?.checked_mul(offsize)?)?;
    let mut previous = 1usize;
    let mut items = Vec::with_capacity(count);
    for i in 0..=count {
        let field = offsets.checked_add(i.checked_mul(offsize)?)?;
        let offset = data
            .get(field..field.checked_add(offsize)?)?
            .iter()
            .fold(0usize, |value, &byte| {
                value.saturating_mul(256).saturating_add(usize::from(byte))
            });
        if offset == 0 || offset < previous {
            return None;
        }
        if i != 0 {
            let start = body.checked_add(previous.checked_sub(1)?)?;
            let end = body.checked_add(offset.checked_sub(1)?)?;
            items.push(data.get(start..end)?.to_vec());
        }
        previous = offset;
    }
    Some(items)
}

fn index_bytes(items: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut serializer = Serializer::new();
    if !cff::index_n(&mut serializer, items, 1, true) {
        return None;
    }
    serializer.end().ok()?.resolve().ok()
}

#[derive(Clone)]
struct DataSet {
    regions: Vec<u16>,
    items: Vec<Vec<u8>>,
}

fn data_set(store: &[u8], at: usize) -> Option<DataSet> {
    if *store.get(at)? != 1 {
        return None;
    }
    let count = usize::from(u16_at(store, at.checked_add(1)?)?);
    let indices = at.checked_add(3)?;
    let mut regions = Vec::with_capacity(count);
    for i in 0..count {
        regions.push(u16_at(store, indices.checked_add(i.checked_mul(2)?)?)?);
    }
    let tuple_field = indices.checked_add(count.checked_mul(2)?)?;
    let tuple_at = at.checked_add(usize::try_from(u32_at(store, tuple_field)?).ok()?)?;
    Some(DataSet {
        regions,
        items: index_items(store, tuple_at)?,
    })
}

fn region_axes(store: &[u8], list: usize, index: usize) -> Option<Vec<[u8; 8]>> {
    let region_field = list.checked_add(4)?.checked_add(index.checked_mul(4)?)?;
    let region = list.checked_add(usize::try_from(u32_at(store, region_field)?).ok()?)?;
    let count = usize::from(u16_at(store, region)?);
    let mut axes = Vec::with_capacity(count);
    for i in 0..count {
        let field = region.checked_add(2)?.checked_add(i.checked_mul(4)?)?;
        let axis = region.checked_add(usize::try_from(u32_at(store, field)?).ok()?)?;
        axes.push(store.get(axis..axis.checked_add(8)?)?.try_into().ok()?);
    }
    Some(axes)
}

/// HarfBuzz's last-packed-first layout for sparse regions, with shared axis
/// objects emitted after their last referring region in reverse order.
#[allow(clippy::indexing_slicing)] // All positions are made from the checked region and axis counts.
fn region_list_bytes(regions: &[Vec<[u8; 8]>]) -> Option<Vec<u8>> {
    let count = regions.len();
    let header = 4usize.checked_add(count.checked_mul(4)?)?;
    let mut out = alloc::vec![0; header];
    put_u32(&mut out, 0, count)?;
    let mut axis_owner = BTreeMap::<[u8; 8], usize>::new();
    let mut region_owner = BTreeMap::<Vec<[u8; 8]>, usize>::new();
    for (region, axes) in regions.iter().enumerate() {
        region_owner.entry(axes.clone()).or_insert(region);
        for &axis in axes {
            axis_owner.entry(axis).or_insert(region);
        }
    }
    let mut region_at = alloc::vec![0; count];
    let mut axis_at = BTreeMap::<[u8; 8], usize>::new();
    for index in (0..count).rev() {
        if region_owner.get(&regions[index]) != Some(&index) {
            continue;
        }
        region_at[index] = out.len();
        let len = 2usize.checked_add(regions[index].len().checked_mul(4)?)?;
        out.resize(out.len().checked_add(len)?, 0);
        out.get_mut(region_at[index]..region_at[index].checked_add(2)?)?
            .copy_from_slice(&u16::try_from(regions[index].len()).ok()?.to_be_bytes());
        for &axis in &regions[index] {
            if axis_owner.get(&axis) == Some(&index) {
                axis_at.insert(axis, out.len());
                out.extend_from_slice(&axis);
            }
        }
    }
    for (index, axes) in regions.iter().enumerate() {
        region_at[index] = *region_at.get(*region_owner.get(axes)?)?;
    }
    for (index, axes) in regions.iter().enumerate() {
        put_u32(
            &mut out,
            4usize.checked_add(index.checked_mul(4)?)?,
            region_at[index],
        )?;
        for (axis_index, axis) in axes.iter().enumerate() {
            let field = region_at[index]
                .checked_add(2)?
                .checked_add(axis_index.checked_mul(4)?)?;
            put_u32(
                &mut out,
                field,
                axis_at.get(axis)?.checked_sub(region_at[index])?,
            )?;
        }
    }
    Some(out)
}

#[allow(clippy::indexing_slicing)] // Selected indices are checked against parsed arrays before indexing.
fn subset_store(src: &[u8], indices: &BTreeSet<u32>) -> Option<(Vec<u8>, BTreeMap<u32, u32>)> {
    if indices.is_empty() {
        return Some((Vec::new(), BTreeMap::new()));
    }
    let at = usize::try_from(u32_at(src, 8)?).ok()?;
    let store = src.get(at..)?;
    if u16_at(store, 0)? != 1 {
        return None;
    }
    let old_count = usize::from(u16_at(store, 6)?);
    let mut by_outer = BTreeMap::<usize, Vec<usize>>::new();
    for &index in indices {
        by_outer
            .entry(usize::try_from(index >> 16).ok()?)
            .or_default()
            .push(usize::try_from(index & 0xffff).ok()?);
    }
    let mut selected = Vec::new();
    let mut map = BTreeMap::new();
    let mut used_regions = BTreeSet::<u16>::new();
    for (new_outer, (&old_outer, inners)) in by_outer.iter().enumerate() {
        if old_outer >= old_count {
            return None;
        }
        let field = 8usize.checked_add(old_outer.checked_mul(4)?)?;
        let dataset_at = usize::try_from(u32_at(store, field)?).ok()?;
        let old = data_set(store, dataset_at)?;
        let mut items = Vec::new();
        for (new_inner, &old_inner) in inners.iter().enumerate() {
            items.push(old.items.get(old_inner)?.clone());
            let old_index =
                u32::try_from(old_outer.checked_mul(65536)?.checked_add(old_inner)?).ok()?;
            let new_index =
                u32::try_from(new_outer.checked_mul(65536)?.checked_add(new_inner)?).ok()?;
            map.insert(old_index, new_index);
        }
        used_regions.extend(old.regions.iter().copied());
        selected.push(DataSet {
            regions: old.regions,
            items,
        });
    }
    let list = usize::try_from(u32_at(store, 2)?).ok()?;
    let total_regions = usize::try_from(u32_at(store, list)?).ok()?;
    let mut region_map = BTreeMap::<u16, u16>::new();
    let mut regions = Vec::new();
    for (new_index, &old_index) in used_regions.iter().enumerate() {
        if usize::from(old_index) >= total_regions {
            return None;
        }
        region_map.insert(old_index, u16::try_from(new_index).ok()?);
        regions.push(region_axes(store, list, usize::from(old_index))?);
    }
    let mut out = alloc::vec![0; 8usize.checked_add(selected.len().checked_mul(4)?)?];
    out.get_mut(0..2)?.copy_from_slice(&1u16.to_be_bytes());
    out.get_mut(6..8)?
        .copy_from_slice(&u16::try_from(selected.len()).ok()?.to_be_bytes());
    for index in (0..selected.len()).rev() {
        let dataset = selected.get(index)?;
        let offset = out.len();
        put_u32(&mut out, 8usize.checked_add(index.checked_mul(4)?)?, offset)?;
        out.push(1);
        out.extend_from_slice(&u16::try_from(dataset.regions.len()).ok()?.to_be_bytes());
        for region in &dataset.regions {
            out.extend_from_slice(&region_map.get(region)?.to_be_bytes());
        }
        let tuple_at = 7usize.checked_add(dataset.regions.len().checked_mul(2)?)?;
        out.extend_from_slice(&u32::try_from(tuple_at).ok()?.to_be_bytes());
        out.extend_from_slice(&index_bytes(&dataset.items)?);
    }
    let list_at = out.len();
    put_u32(&mut out, 2, list_at)?;
    out.extend_from_slice(&region_list_bytes(&regions)?);
    Some((out, map))
}

fn varint_size(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xc0 {
        2
    } else if first < 0xe0 {
        3
    } else if first < 0xf0 {
        4
    } else {
        5
    }
}

fn advance_varint(data: &[u8], at: usize) -> Option<usize> {
    at.checked_add(varint_size(*data.get(at)?))
        .filter(|&end| end <= data.len())
}

#[allow(clippy::indexing_slicing)] // Component offsets come from the validated VARC record view.
fn rewrite_record(
    varc_view: &Varc<'_>,
    old_gid: u32,
    plan: &Plan,
    condition_map: &BTreeMap<u32, u32>,
    axis_map: &BTreeMap<u32, u32>,
    var_map: &BTreeMap<u32, u32>,
) -> Option<Vec<u8>> {
    let record = varc_view.record(GlyphId(old_gid))?;
    let mut result = Vec::new();
    let mut start = 0usize;
    for part in varc_view.components(GlyphId(old_gid))? {
        let part = part?;
        let end = start.checked_add(part.size)?;
        let source = record.get(start..end)?;
        let child = plan.glyphs.new_gid(part.glyph.0)?;
        let wide = part.glyph_size == 3 || child > 0xffff;
        let flags = if wide {
            part.flags | (1 << 12)
        } else {
            part.flags
        };
        varc::varint(&mut result, flags);
        let encoded = child.to_be_bytes();
        result.extend_from_slice(if wide { &encoded[1..] } else { &encoded[2..] });
        let mut cursor = part.glyph_offset.checked_add(part.glyph_size)?;
        if let Some(condition) = part.condition_index {
            cursor = advance_varint(source, cursor)?;
            varc::varint(&mut result, *condition_map.get(&condition)?);
        }
        if let Some(axis) = part.axis_indices_index {
            cursor = advance_varint(source, cursor)?;
            varc::varint(&mut result, *axis_map.get(&axis)?);
            let values_end = part.axis_values_end?;
            result.extend_from_slice(source.get(cursor..values_end)?);
            cursor = values_end;
        }
        if let Some(index) = part.axis_values_var_index {
            cursor = advance_varint(source, cursor)?;
            let new = if index == u32::MAX {
                index
            } else {
                *var_map.get(&index)?
            };
            varc::varint(&mut result, new);
        }
        if let Some(index) = part.transform_var_index {
            cursor = advance_varint(source, cursor)?;
            let new = if index == u32::MAX {
                index
            } else {
                *var_map.get(&index)?
            };
            varc::varint(&mut result, new);
        }
        result.extend_from_slice(source.get(cursor..)?);
        start = end;
    }
    Some(result)
}

#[allow(clippy::too_many_lines)] // The maps and their serialized auxiliary tables form one transaction.
pub(super) fn subset(
    plan: &Plan,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let malformed = || SubsetError::Malformed { tag };
    let failed = || SubsetError::Failed { tag };
    let unsupported = || SubsetError::Unsupported {
        tag,
        reason: "VARC auxiliary references",
    };
    let varc_view = Varc::new(src).map_err(|_| malformed())?;
    let mut old_gids = Vec::new();
    let mut new_gids = Vec::new();
    let mut axis_indices = BTreeSet::<u32>::new();
    let mut var_indices = BTreeSet::<u32>::new();
    let mut conditions = BTreeSet::<u32>::new();
    for (first, last) in varc_view.coverage().ranges() {
        for old in first.0..=last.0 {
            if !plan.glyphset_varced.contains(old) {
                continue;
            }
            old_gids.push(old);
            new_gids.push(plan.glyphs.new_gid(old).ok_or_else(failed)?);
            for part in varc_view.components(GlyphId(old)).ok_or_else(malformed)? {
                let part = part.ok_or_else(malformed)?;
                if let Some(condition) = part.condition_index {
                    conditions.insert(condition);
                }
                if let Some(index) = part.axis_indices_index {
                    axis_indices.insert(index);
                }
                for index in [part.axis_values_var_index, part.transform_var_index]
                    .into_iter()
                    .flatten()
                {
                    if index != u32::MAX {
                        var_indices.insert(index);
                    }
                }
            }
        }
    }
    if old_gids.is_empty() {
        return Ok(false);
    }
    selected_condition_var_indices(src, &conditions, &mut var_indices).ok_or_else(unsupported)?;
    let axis_items = if axis_indices.is_empty() {
        Vec::new()
    } else {
        let at =
            usize::try_from(u32_at(src, 16).ok_or_else(malformed)?).map_err(|_| malformed())?;
        index_items(src, at).ok_or_else(malformed)?
    };
    let mut axis_map = BTreeMap::new();
    let mut selected_axes = Vec::new();
    for (new, &old) in axis_indices.iter().enumerate() {
        axis_map.insert(old, u32::try_from(new).map_err(|_| failed())?);
        selected_axes.push(
            axis_items
                .get(usize::try_from(old).map_err(|_| malformed())?)
                .ok_or_else(malformed)?
                .clone(),
        );
    }
    let (store, var_map) = subset_store(src, &var_indices).ok_or_else(unsupported)?;
    let (condition, condition_map) =
        subset_conditions(src, &conditions, &var_map).ok_or_else(unsupported)?;
    let mut records = Vec::new();
    for old in old_gids {
        records.push(
            rewrite_record(&varc_view, old, plan, &condition_map, &axis_map, &var_map)
                .ok_or_else(failed)?,
        );
    }
    s.extend(src.get(..4).ok_or_else(malformed)?);
    let coverage_field = s.zeros(4);
    let store_field = s.zeros(4);
    let condition_field = s.zeros(4);
    let axes_field = s.zeros(4);
    s.u32(24);
    if !cff::index_n(s, &records, 1, true) {
        return Err(failed());
    }
    for (field, bytes) in [
        (store_field, &store),
        (axes_field, &index_bytes(&selected_axes).ok_or_else(failed)?),
    ] {
        if bytes.is_empty() || (field == axes_field && selected_axes.is_empty()) {
            continue;
        }
        let at = s.len();
        if !s.check_assign_u32(field, at as u64, Errors::INT_OVERFLOW) {
            return Err(SubsetError::OffsetOverflow { tag });
        }
        s.extend(bytes);
    }
    if !conditions.is_empty() {
        let at = s.len();
        if !s.check_assign_u32(condition_field, at as u64, Errors::INT_OVERFLOW) {
            return Err(SubsetError::OffsetOverflow { tag });
        }
        s.extend(&condition);
    }
    let at = s.len();
    if !s.check_assign_u32(coverage_field, at as u64, Errors::INT_OVERFLOW) {
        return Err(SubsetError::OffsetOverflow { tag });
    }
    let mut context = crate::layout::common::Sub::new(plan, s, false);
    if !context.coverage(&new_gids) {
        return Err(failed());
    }
    Ok(true)
}
