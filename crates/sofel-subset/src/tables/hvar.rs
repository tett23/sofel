//! HVAR and VVAR with implicit advance maps (ADR 0022 stage 10).

use alloc::collections::BTreeSet;
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
    let data = store.item_data(0).ok_or(SubsetError::Malformed { tag })?;
    let mut old_items = BTreeSet::new();
    for (map_index, map) in maps.iter().enumerate() {
        if map_index != 0 && map.is_none() {
            continue;
        }
        for &(_, old_gid) in plan.new_to_old_glyphs() {
            let index = map.map_or(old_gid, |map| map.map(old_gid));
            if index >> 16 != 0 {
                return Err(SubsetError::Unsupported {
                    tag,
                    reason: "HVAR/VVAR indices using more than one outer index",
                });
            }
            old_items.insert(index);
        }
    }
    if old_items.len() > usize::from(u16::MAX) {
        return Err(SubsetError::Unsupported {
            tag,
            reason: "HVAR/VVAR indices using more than one outer index",
        });
    }
    let old_items: Vec<_> = old_items.into_iter().collect();
    let store_bytes = subset_store(store, data, &old_items, tag)?;
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
    let mut map_offsets = Vec::new();
    for map in &rewritten_maps {
        map_offsets.push(if map.is_some() { offset } else { 0 });
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
    for map in rewritten_maps.into_iter().flatten() {
        serializer.extend(&map);
    }
    serializer.extend(&store_bytes);
    Ok(true)
}

fn index_map(
    plan: &Plan,
    old_items: &[u32],
    source: Option<DeltaSetIndexMap<'_>>,
    tag: Tag,
) -> Result<Option<Vec<u8>>, SubsetError> {
    let Some(&(last_gid, _)) = plan.new_to_old_glyphs().last() else {
        return Ok(None);
    };
    let map_count = last_gid.checked_add(1).ok_or(SubsetError::Limit { tag })?;
    let count = usize::try_from(map_count).map_err(|_| SubsetError::Limit { tag })?;
    let mut values = alloc::vec![0u16; count];
    for &(new_gid, old_gid) in plan.new_to_old_glyphs() {
        let index = usize::try_from(new_gid).map_err(|_| SubsetError::Limit { tag })?;
        let old_item = source.map_or(old_gid, |map| map.map(old_gid));
        let inner = old_items
            .binary_search(&old_item)
            .map_err(|_| SubsetError::Malformed { tag })?;
        *values
            .get_mut(index)
            .ok_or(SubsetError::Malformed { tag })? =
            u16::try_from(inner).map_err(|_| SubsetError::Limit { tag })?;
    }
    if values
        .iter()
        .enumerate()
        .all(|(index, &value)| usize::from(value) == index)
    {
        return Ok(None);
    }
    let max_inner =
        u16::try_from(old_items.len().saturating_sub(1)).map_err(|_| SubsetError::Limit { tag })?;
    let inner_bits = u16::BITS.saturating_sub(max_inner.leading_zeros()).max(1);
    let width = usize::try_from(inner_bits.saturating_add(1).div_ceil(8))
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
        let bytes = u32::from(value).to_be_bytes();
        output.extend(
            bytes
                .get(bytes.len().saturating_sub(width)..)
                .ok_or(SubsetError::Limit { tag })?,
        );
    }
    Ok(Some(output))
}

fn subset_store(
    store: ItemVariationStore<'_>,
    data: ItemVariationData<'_>,
    old_gids: &[u32],
    tag: Tag,
) -> Result<Vec<u8>, SubsetError> {
    let count = u16::try_from(old_gids.len()).map_err(|_| SubsetError::Limit { tag })?;
    // Classify retained columns by their largest signed delta width.
    // A repeated region may have both an all-zero and a live column.
    let mut referenced = BTreeSet::new();
    let mut columns = Vec::new();
    let valid_rows = old_gids.partition_point(|&g| g < u32::from(data.item_count));
    for col in 0..data.region_index_count() {
        let region = data
            .region_index(usize::from(col))
            .ok_or(SubsetError::Malformed { tag })?;
        if region >= store.region_count() {
            return Err(SubsetError::Malformed { tag });
        }
        let width = old_gids
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
    // Keep source order within the wide and narrow groups.
    columns.sort_by_key(|&(_, _, width)| width <= small_width);
    let word_count = u16::try_from(columns.iter().take_while(|c| c.2 > small_width).count())
        .ok()
        .filter(|&n| n <= 0x7fff)
        .ok_or(SubsetError::Limit { tag })?;
    let mut output = Serializer::new();
    output.u16(1);
    output.u32(0); // RegionList follows the packed VarData.
    output.u16(1);
    output.u32(12);
    output.u16(count);
    output.u16(word_count | if has_long { 0x8000 } else { 0 });
    output.u16(u16::try_from(columns.len()).map_err(|_| SubsetError::Limit { tag })?);
    let referenced: Vec<_> = referenced.into_iter().collect();
    for (_, region, _) in &columns {
        let new_region = referenced.binary_search(region);
        output.u16(
            u16::try_from(new_region.map_err(|_| SubsetError::Malformed { tag })?)
                .map_err(|_| SubsetError::Limit { tag })?,
        );
    }
    for &glyph in old_gids {
        for (column, &(source, _, _)) in columns.iter().enumerate() {
            let value = delta(&data, glyph, source);
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
