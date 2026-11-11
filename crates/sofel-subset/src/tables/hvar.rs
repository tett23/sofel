//! HVAR and VVAR with implicit advance maps (ADR 0022 stage 10).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::delta_set_index_map::DeltaSetIndexMap;
use sofel_font::read::hvar::{Hvar, Vvar};
use sofel_font::read::item_variation::{ItemVariationData, ItemVariationStore};
use sofel_write::Serializer;

use crate::error::SubsetError;
use crate::plan::Plan;

use super::Tables;

pub(super) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    serializer: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let vertical = tag == Tag::new(b"VVAR");
    let (store, maps) = if vertical {
        let table = Vvar::new(src).map_err(|_| SubsetError::Malformed { tag })?;
        (
            table.store().map_err(|_| SubsetError::Malformed { tag })?,
            [
                table
                    .advance_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
                table
                    .tsb_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
                table
                    .bsb_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
                table
                    .vorg_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
            ],
        )
    } else {
        let table = Hvar::new(src).map_err(|_| SubsetError::Malformed { tag })?;
        (
            table.store().map_err(|_| SubsetError::Malformed { tag })?,
            [
                table
                    .advance_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
                table
                    .lsb_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
                table
                    .rsb_map()
                    .map_err(|_| SubsetError::Malformed { tag })?,
                None,
            ],
        )
    };
    let mut old_items: BTreeMap<u16, BTreeSet<u32>> = BTreeMap::new();
    for (map_index, map) in maps.iter().enumerate() {
        if map_index != 0 && map.is_none() {
            continue;
        }
        for &(_, old_gid) in plan.new_to_old_glyphs() {
            let index = map.map_or(old_gid, |map| map.map(old_gid));
            let outer = u16::try_from(index >> 16).map_err(|_| SubsetError::Limit { tag })?;
            if outer >= store.data_count() {
                return Err(SubsetError::Malformed { tag });
            }
            old_items.entry(outer).or_default().insert(index & 0xffff);
        }
    }
    let old_items: Vec<_> = old_items
        .into_iter()
        .map(|(outer, rows)| (outer, rows.into_iter().collect::<Vec<_>>()))
        .collect();
    let store_bytes = subset_store(store, &old_items, tag)?;
    let mut rewritten_maps = Vec::new();
    for (index, map) in maps
        .into_iter()
        .enumerate()
        .take(if vertical { 4 } else { 3 })
    {
        rewritten_maps.push(if index == 0 || map.is_some() {
            index_map(plan, &old_items, map, tag)?
        } else {
            None
        });
    }

    let header_len = if vertical { 24u32 } else { 20u32 };
    let mut offset = header_len;
    let mut map_offsets = alloc::vec![0; rewritten_maps.len()];
    for (index, map) in rewritten_maps.iter().enumerate().rev() {
        *map_offsets
            .get_mut(index)
            .ok_or(SubsetError::Limit { tag })? = if map.is_some() { offset } else { 0 };
        offset = offset
            .checked_add(
                u32::try_from(map.as_ref().map_or(0, Vec::len))
                    .map_err(|_| SubsetError::Limit { tag })?,
            )
            .ok_or(SubsetError::Limit { tag })?;
    }
    serializer.u32(0x0001_0000);
    serializer.u32(offset);
    for map_offset in map_offsets {
        serializer.u32(map_offset);
    }
    for map in rewritten_maps.into_iter().rev().flatten() {
        serializer.extend(&map);
    }
    serializer.extend(&store_bytes);
    Ok(true)
}

fn index_map(
    plan: &Plan,
    old_items: &[(u16, Vec<u32>)],
    source: Option<DeltaSetIndexMap<'_>>,
    tag: Tag,
) -> Result<Option<Vec<u8>>, SubsetError> {
    let Some(&(last_gid, _)) = plan.new_to_old_glyphs().last() else {
        return Ok(None);
    };
    let map_count = last_gid.checked_add(1).ok_or(SubsetError::Limit { tag })?;
    let count = usize::try_from(map_count).map_err(|_| SubsetError::Limit { tag })?;
    let mut values = alloc::vec![0u32; count];
    for &(new_gid, old_gid) in plan.new_to_old_glyphs() {
        let index = usize::try_from(new_gid).map_err(|_| SubsetError::Limit { tag })?;
        let old_item = source.map_or(old_gid, |map| map.map(old_gid));
        let outer = u16::try_from(old_item >> 16).map_err(|_| SubsetError::Limit { tag })?;
        let new_outer = old_items
            .binary_search_by_key(&outer, |&(outer, _)| outer)
            .map_err(|_| SubsetError::Malformed { tag })?;
        let rows = &old_items
            .get(new_outer)
            .ok_or(SubsetError::Malformed { tag })?
            .1;
        let inner = rows
            .binary_search(&(old_item & 0xffff))
            .map_err(|_| SubsetError::Malformed { tag })?;
        *values
            .get_mut(index)
            .ok_or(SubsetError::Malformed { tag })? =
            (u32::try_from(new_outer).map_err(|_| SubsetError::Limit { tag })? << 16)
                | u32::try_from(inner).map_err(|_| SubsetError::Limit { tag })?;
    }
    if source.is_none()
        && values
            .iter()
            .enumerate()
            .all(|(index, &value)| usize::try_from(value).ok() == Some(index))
    {
        return Ok(None);
    }
    let max_inner = values.iter().map(|value| value & 0xffff).max().unwrap_or(0);
    let max_outer = values.iter().map(|value| value >> 16).max().unwrap_or(0);
    let inner_bits = u32::BITS.saturating_sub(max_inner.leading_zeros()).max(1);
    let outer_bits = u32::BITS.saturating_sub(max_outer.leading_zeros()).max(1);
    let width = usize::try_from(inner_bits.saturating_add(outer_bits).div_ceil(8))
        .map_err(|_| SubsetError::Limit { tag })?;
    let mut output = Vec::new();
    let format = if count <= usize::from(u16::MAX) {
        0u8
    } else {
        1u8
    };
    output.push(format);
    output.push(
        (u8::try_from(width.saturating_sub(1)).map_err(|_| SubsetError::Limit { tag })? << 4)
            | u8::try_from(inner_bits.saturating_sub(1)).map_err(|_| SubsetError::Limit { tag })?,
    );
    if format == 0 {
        output.extend(
            u16::try_from(count)
                .map_err(|_| SubsetError::Limit { tag })?
                .to_be_bytes(),
        );
    } else {
        output.extend(map_count.to_be_bytes());
    }
    for value in values {
        let bytes = (((value >> 16) << inner_bits) | (value & 0xffff)).to_be_bytes();
        output.extend(
            bytes
                .get(bytes.len().saturating_sub(width)..)
                .ok_or(SubsetError::Limit { tag })?,
        );
    }
    Ok(Some(output))
}

pub(crate) fn subset_store(
    store: ItemVariationStore<'_>,
    old_items: &[(u16, Vec<u32>)],
    tag: Tag,
) -> Result<Vec<u8>, SubsetError> {
    let mut referenced = BTreeSet::new();
    let mut plans = Vec::new();
    for (outer, rows) in old_items {
        let data = store
            .item_data(*outer)
            .ok_or(SubsetError::Malformed { tag })?;
        let valid_rows = rows.partition_point(|&g| g < u32::from(data.item_count));
        let mut columns = Vec::new();
        for col in 0..data.region_index_count() {
            let region = data
                .region_index(usize::from(col))
                .ok_or(SubsetError::Malformed { tag })?;
            if region >= store.region_count() {
                return Err(SubsetError::Malformed { tag });
            }
            let width = rows
                .get(..valid_rows)
                .unwrap_or_default()
                .iter()
                .map(|&g| match delta(&data, g, col) {
                    0 => 0,
                    -128..=127 => 1,
                    -32768..=32767 => 2,
                    _ => 4,
                })
                .max()
                .unwrap_or(0);
            if width != 0 {
                referenced.insert(region);
                columns.push((col, region, width));
            }
        }
        let has_long = columns.iter().any(|&(_, _, width)| width == 4);
        let small_width = if has_long { 2 } else { 1 };
        columns.sort_by_key(|&(_, _, width)| width <= small_width);
        plans.push((data, rows, columns));
    }
    let mut output = Serializer::new();
    output.u16(1);
    output.u32(0); // RegionList follows the packed VarData.
    output.u16(u16::try_from(plans.len()).map_err(|_| SubsetError::Limit { tag })?);
    for _ in &plans {
        output.u32(0);
    }
    let referenced: Vec<_> = referenced.into_iter().collect();
    for (index, (data, rows, columns)) in plans.iter().enumerate().rev() {
        let offset_at = 8usize
            .checked_add(index.checked_mul(4).ok_or(SubsetError::Limit { tag })?)
            .ok_or(SubsetError::Limit { tag })?;
        output.set_u32(
            offset_at,
            u32::try_from(output.current().len()).map_err(|_| SubsetError::Limit { tag })?,
        );
        let has_long = columns.iter().any(|&(_, _, width)| width == 4);
        let small_width = if has_long { 2 } else { 1 };
        let word_count = u16::try_from(columns.iter().take_while(|c| c.2 > small_width).count())
            .ok()
            .filter(|&n| n <= 0x7fff)
            .ok_or(SubsetError::Limit { tag })?;
        output.u16(u16::try_from(rows.len()).map_err(|_| SubsetError::Limit { tag })?);
        output.u16(word_count | if has_long { 0x8000 } else { 0 });
        output.u16(u16::try_from(columns.len()).map_err(|_| SubsetError::Limit { tag })?);
        for (_, region, _) in columns {
            let new_region = referenced.binary_search(region);
            output.u16(
                u16::try_from(new_region.map_err(|_| SubsetError::Malformed { tag })?)
                    .map_err(|_| SubsetError::Limit { tag })?,
            );
        }
        for &glyph in rows.iter() {
            for (column, &(source, _, _)) in columns.iter().enumerate() {
                let value = delta(data, glyph, source);
                let word = column < usize::from(word_count);
                if word {
                    if has_long {
                        output.extend(&value.to_be_bytes());
                    } else {
                        output.i16(i16::try_from(value).map_err(|_| SubsetError::Limit { tag })?);
                    }
                } else if has_long {
                    output.i16(i16::try_from(value).map_err(|_| SubsetError::Limit { tag })?);
                } else {
                    output.u8(i8::try_from(value)
                        .map_err(|_| SubsetError::Limit { tag })?
                        .to_be_bytes()[0]);
                }
            }
        }
    }
    output.set_u32(
        2,
        u32::try_from(output.current().len()).map_err(|_| SubsetError::Limit { tag })?,
    );
    output.u16(store.axis_count());
    output.u16(u16::try_from(referenced.len()).map_err(|_| SubsetError::Limit { tag })?);
    for region_index in &referenced {
        let region = store
            .region(*region_index)
            .ok_or(SubsetError::Malformed { tag })?;
        for axis_index in 0..region.len() {
            let (start, peak, end) = region
                .axis(axis_index)
                .ok_or(SubsetError::Malformed { tag })?;
            output.i16(start);
            output.i16(peak);
            output.i16(end);
        }
    }
    Ok(output.current().to_vec())
}

fn delta(data: &ItemVariationData<'_>, old_gid: u32, column: u16) -> i32 {
    u16::try_from(old_gid)
        .ok()
        .and_then(|old| data.delta(old, column))
        .unwrap_or(0)
}
