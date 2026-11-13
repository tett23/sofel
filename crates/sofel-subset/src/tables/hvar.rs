//! HVAR and VVAR with implicit advance maps (ADR 0022 stage 10).

pub(crate) mod instance;
pub(crate) mod optimize;

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
    font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    serializer: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let vertical = tag == Tag::new(b"VVAR");
    let (store, mut maps) = if vertical {
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
    // Only advances use implicit glyph indexing. Empty optional maps
    // contribute no side-bearing or vertical-origin deltas.
    for map in maps.iter_mut().skip(1) {
        *map = map.filter(|m| !m.is_empty());
    }
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
    let mut remap = BTreeMap::new();
    let store_bytes = if plan.var.instancing() {
        instance::subset(
            plan,
            font,
            store,
            &old_items,
            tag,
            maps[0].is_some(),
            &mut remap,
        )?
    } else if plan.var.n.avar2 && !plan.var.n.avar2_reachable.is_empty() {
        // HarfBuzz leaves the store's rows and regions in their original
        // coordinates here, removing only regions the restricted avar2
        // mapping cannot reach.
        subset_store_inner(store, &old_items, tag, Some(plan))?
    } else {
        subset_store(store, &old_items, tag)?
    };
    if store_bytes.is_empty() {
        return Ok(false);
    }
    let mut rewritten_maps = Vec::new();
    for (index, map) in maps
        .into_iter()
        .enumerate()
        .take(if vertical { 4 } else { 3 })
    {
        rewritten_maps.push(if index == 0 || map.is_some() {
            index_map(plan, &old_items, map, &remap, tag)?
        } else {
            None
        });
    }

    let header_len = if vertical { 24u32 } else { 20u32 };
    let mut offset = header_len;
    let mut map_offsets = alloc::vec![0; rewritten_maps.len()];
    let mut written_maps: Vec<(&Vec<u8>, u32)> = Vec::new();
    for (index, map) in rewritten_maps.iter().enumerate().rev() {
        let Some(map) = map else { continue };
        let map_offset = if let Some((_, prior_offset)) = written_maps
            .iter()
            .find(|(prior, _)| prior.as_slice() == map.as_slice())
        {
            *prior_offset
        } else {
            let map_offset = offset;
            offset = offset
                .checked_add(u32::try_from(map.len()).map_err(|_| SubsetError::Limit { tag })?)
                .ok_or(SubsetError::Limit { tag })?;
            written_maps.push((map, map_offset));
            map_offset
        };
        *map_offsets
            .get_mut(index)
            .ok_or(SubsetError::Limit { tag })? = map_offset;
    }
    serializer.u32(0x0001_0000);
    serializer.u32(offset);
    for map_offset in map_offsets {
        serializer.u32(map_offset);
    }
    for (map, _) in written_maps {
        serializer.extend(map);
    }
    serializer.extend(&store_bytes);
    Ok(true)
}

fn index_map(
    plan: &Plan,
    old_items: &[(u16, Vec<u32>)],
    source: Option<DeltaSetIndexMap<'_>>,
    remap: &BTreeMap<u32, u32>,
    tag: Tag,
) -> Result<Option<Vec<u8>>, SubsetError> {
    let glyphs = plan.new_to_old_glyphs();
    let Some(&(last_gid, last_old_gid)) = glyphs.last() else {
        return Ok(None);
    };
    // A map repeats its final entry for higher glyph IDs. Walk retained
    // glyphs only: gaps introduced by RETAIN_GIDS do not end the run.
    let last_entry = if let Some(source) = source {
        let last_value = source.map(last_old_gid);
        glyphs
            .iter()
            .rev()
            .take_while(|&&(_, old)| source.map(old) == last_value)
            .last()
            .map_or(last_gid, |&(gid, _)| gid)
    } else {
        last_gid
    };
    let map_count = last_entry
        .checked_add(1)
        .ok_or(SubsetError::Limit { tag })?;
    let count = usize::try_from(map_count).map_err(|_| SubsetError::Limit { tag })?;
    let mut values = alloc::vec![0u32; count];
    for &(new_gid, old_gid) in glyphs.iter().take_while(|&&(gid, _)| gid <= last_entry) {
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
        let compact = (u32::try_from(new_outer).map_err(|_| SubsetError::Limit { tag })? << 16)
            | u32::try_from(inner).map_err(|_| SubsetError::Limit { tag })?;
        *values
            .get_mut(index)
            .ok_or(SubsetError::Malformed { tag })? = if remap.is_empty() {
            compact
        } else {
            *remap.get(&compact).ok_or(SubsetError::Malformed { tag })?
        };
    }
    if source.is_none()
        && values
            .iter()
            .enumerate()
            .all(|(index, &value)| usize::try_from(value).ok() == Some(index))
    {
        return Ok(None);
    }
    encode_index_map(&values, tag).map(Some)
}

/// Encodes an already remapped DeltaSetIndexMap, preserving the caller's entry count.
pub(super) fn encode_index_map(values: &[u32], tag: Tag) -> Result<Vec<u8>, SubsetError> {
    let count = values.len();
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
        output.extend(
            u32::try_from(count)
                .map_err(|_| SubsetError::Limit { tag })?
                .to_be_bytes(),
        );
    }
    for value in values {
        let bytes = (((value >> 16) << inner_bits) | (value & 0xffff)).to_be_bytes();
        output.extend(
            bytes
                .get(bytes.len().saturating_sub(width)..)
                .ok_or(SubsetError::Limit { tag })?,
        );
    }
    Ok(output)
}

pub(crate) fn subset_store(
    store: ItemVariationStore<'_>,
    old_items: &[(u16, Vec<u32>)],
    tag: Tag,
) -> Result<Vec<u8>, SubsetError> {
    subset_store_inner(store, old_items, tag, None)
}

pub(crate) fn subset_store_with_avar2(
    plan: &Plan,
    store: ItemVariationStore<'_>,
    old_items: &[(u16, Vec<u32>)],
    tag: Tag,
) -> Result<Vec<u8>, SubsetError> {
    subset_store_inner(store, old_items, tag, Some(plan))
}

fn subset_store_inner(
    store: ItemVariationStore<'_>,
    old_items: &[(u16, Vec<u32>)],
    tag: Tag,
    avar2_plan: Option<&Plan>,
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
            if let Some(plan) = avar2_plan {
                let source = store.region(region).ok_or(SubsetError::Malformed { tag })?;
                let tents = (0..source.len())
                    .map(|axis| {
                        source
                            .axis(axis)
                            .map(|(a, b, c)| [a, b, c].map(|value| f32::from(value) / 16384.0))
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or(SubsetError::Malformed { tag })?;
                if super::avar::region_dead(
                    &tents,
                    &plan.var.n.axis_tags,
                    &plan.var.n.avar2_reachable,
                ) {
                    continue;
                }
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
