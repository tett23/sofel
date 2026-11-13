//! Partial avar2 instancing (sofel ADR 0061).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use sofel_font::Font;
use sofel_font::read::{Tag, avar::Avar, item_variation::ItemVariationStore};
use sofel_font::var::{Arith, HbF32};
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32, i32_to_f32};
use sofel_write::Serializer;

use crate::{Plan, SubsetError};

const TAG: Tag = Tag::new(b"avar");
type Mapping = Vec<(i16, i16)>;
type Region = Vec<[u32; 3]>;
type FloatBlock = (usize, Vec<(Region, Vec<f32>)>);

fn map_new_mapping(pairs: &[(i16, i16)], value: f64) -> f64 {
    let mut previous = None;
    for &(from, to) in pairs {
        let from = f64::from(from) / 16384.0;
        let to = f64::from(to) / 16384.0;
        if value == from {
            return to;
        }
        if value < from {
            let Some((old_from, old_to)) = previous else {
                return value - from + to;
            };
            return if from == old_from {
                old_to
            } else {
                old_to + (to - old_to) * (value - old_from) / (from - old_from)
            };
        }
        previous = Some((from, to));
    }
    previous.map_or(value, |(from, to)| value - from + to)
}

fn region_key(tents: &[[f32; 3]]) -> Region {
    tents
        .iter()
        .map(|t| t.map(|v| if v == 0.0 { 0 } else { v.to_bits() }))
        .collect()
}

fn add_column(columns: &mut Vec<(Region, Vec<f32>)>, region: Region, deltas: &[f32], scale: f32) {
    if let Some((_, values)) = columns.iter_mut().find(|(key, _)| *key == region) {
        for (value, &delta) in values.iter_mut().zip(deltas) {
            *value += delta * scale;
        }
    } else {
        columns.push((region, deltas.iter().map(|&d| d * scale).collect()));
    }
}

fn original_regions(store: ItemVariationStore<'_>) -> Result<Vec<Vec<[f32; 3]>>, SubsetError> {
    let bad = SubsetError::Malformed { tag: TAG };
    (0..store.region_count())
        .map(|index| {
            let region = store.region(index).ok_or(bad)?;
            (0..region.len())
                .map(|axis| {
                    let (start, peak, end) = region.axis(axis).ok_or(bad)?;
                    Ok(if peak == 0 {
                        [0.0; 3]
                    } else {
                        [start, peak, end].map(|v| f32::from(v) / 16384.0)
                    })
                })
                .collect()
        })
        .collect()
}

fn project_store(
    plan: &Plan,
    font: &Font<'_>,
    store: ItemVariationStore<'_>,
) -> Result<(Vec<FloatBlock>, Vec<Region>), SubsetError> {
    let bad = SubsetError::Malformed { tag: TAG };
    let axes = crate::instance::axis_infos(font);
    let changes = plan
        .var
        .n
        .old_intermediates
        .iter()
        .filter_map(|(tag, range)| {
            let axis = axes.iter().position(|a| a.tag == *tag)?;
            let info = axes.get(axis)?;
            Some((
                axis,
                [range.min, range.default, range.max],
                [info.default - info.min, info.max - info.default],
            ))
        })
        .collect::<Vec<_>>();
    let original = original_regions(store)?;
    let order = original.iter().map(|r| region_key(r)).collect();
    let mut blocks = Vec::new();
    for outer in 0..store.data_count() {
        let data = store.item_data(outer).ok_or(bad)?;
        let count = usize::from(data.item_count);
        let mut columns = Vec::new();
        for col in 0..data.region_index_count() {
            let source = original
                .get(usize::from(data.region_index(usize::from(col)).ok_or(bad)?))
                .ok_or(bad)?;
            let values = (0..data.item_count)
                .map(|row| Ok(i32_to_f32(data.delta(row, col).ok_or(bad)?)))
                .collect::<Result<Vec<_>, _>>()?;
            for (tents, scales) in super::variation_region::split(
                source.clone(),
                &changes,
                usize::from(u16::MAX),
                TAG,
            )? {
                let key = region_key(&tents);
                if key.iter().all(|t| *t == [0; 3]) {
                    continue;
                }
                add_column(&mut columns, key, &values, scales.into_iter().product());
            }
        }
        blocks.push((count, columns));
    }
    Ok((blocks, order))
}

fn default_delta(store: Option<ItemVariationStore<'_>>, index: u32, coords: &[i32]) -> f32 {
    let Some(store) = store else {
        return 0.0;
    };
    let Some(data) = u16::try_from(index >> 16)
        .ok()
        .and_then(|outer| store.item_data(outer))
    else {
        return 0.0;
    };
    let inner = (index & 0xffff) as u16;
    if inner >= data.item_count {
        return 0.0;
    }
    let mut value = 0.0f32;
    for col in 0..data.region_index_count() {
        let Some(region) = data
            .region_index(usize::from(col))
            .and_then(|i| store.region(i))
        else {
            continue;
        };
        let mut scalar = 1.0f32;
        for axis in 0..region.len() {
            let Some((start, peak, end)) = region.axis(axis) else {
                scalar = 0.0;
                break;
            };
            scalar = HbF32::fold_region_axis(
                scalar,
                coords.get(axis).copied().unwrap_or(0),
                start,
                peak,
                end,
            );
        }
        value += scalar * i32_to_f32(data.delta(inner, col).unwrap_or(0));
    }
    value
}

fn add_single(
    blocks: &mut [FloatBlock],
    outer: usize,
    inner: usize,
    tent: Vec<[i16; 3]>,
    delta: i32,
) -> Result<(), SubsetError> {
    let bad = SubsetError::Malformed { tag: TAG };
    let (count, columns) = blocks.get_mut(outer).ok_or(bad)?;
    let region = tent
        .iter()
        .map(|t| {
            t.map(|v| {
                let value = f32::from(v) / 16384.0;
                if value == 0.0 { 0 } else { value.to_bits() }
            })
        })
        .collect::<Region>();
    if let Some((_, values)) = columns.iter_mut().find(|(r, _)| *r == region) {
        *values.get_mut(inner).ok_or(bad)? += i32_to_f32(delta);
    } else {
        let mut values = alloc::vec![0.0; *count];
        *values.get_mut(inner).ok_or(bad)? = i32_to_f32(delta);
        columns.push((region, values));
    }
    Ok(())
}

pub(super) fn subset(plan: &Plan, font: &Font<'_>, src: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let avar = Avar::new(src).map_err(|_| SubsetError::Malformed { tag: TAG })?;
    let (mut output, mappings) = subset_maps(plan, font, avar)?;
    let (store, index_map) = subset_store(plan, font, avar, &mappings)?;
    if store.is_empty() {
        return Ok(Vec::new());
    }
    let store_offset = output
        .len()
        .checked_add(8)
        .ok_or(SubsetError::Limit { tag: TAG })?;
    output.extend(
        u32::try_from(
            store_offset
                .checked_add(store.len())
                .ok_or(SubsetError::Limit { tag: TAG })?,
        )
        .map_err(|_| SubsetError::Limit { tag: TAG })?
        .to_be_bytes(),
    );
    output.extend(
        u32::try_from(store_offset)
            .map_err(|_| SubsetError::Limit { tag: TAG })?
            .to_be_bytes(),
    );
    output.extend(store);
    output.extend(index_map);
    Ok(output)
}

fn subset_maps(
    plan: &Plan,
    font: &Font<'_>,
    avar: Avar<'_>,
) -> Result<(Vec<u8>, Vec<Mapping>), SubsetError> {
    let bad = SubsetError::Malformed { tag: TAG };
    let limit = SubsetError::Limit { tag: TAG };
    let axes = crate::instance::axis_infos(font);
    let mut output = Vec::from([0, 2, 0, 0, 0, 0]);
    output.extend(
        u16::try_from(plan.var.n.retained_axis_indices.len())
            .map_err(|_| limit)?
            .to_be_bytes(),
    );
    let mut mappings = alloc::vec![Vec::new(); axes.len()];
    for &old in &plan.var.n.retained_axis_indices {
        let axis = axes.get(old).ok_or(bad)?;
        let map = avar.segment_map(old).ok_or(bad)?;
        let mut pairs = Vec::with_capacity(map.len());
        if plan
            .var
            .user()
            .get(&axis.tag)
            .is_some_and(|r| r.min == r.max)
        {
            pairs.extend([(-16384, -16384), (0, 0), (16384, 16384)]);
        } else {
            for i in 0..map.len() {
                let (from, to) = map.pair(i).ok_or(bad)?;
                pairs.push((from.0, to.0));
            }
            if let Some(range) = plan.var.n.old_intermediates.get(&axis.tag) {
                let mapped = [range.min, range.default, range.max];
                let unmapped = mapped.map(|v| sofel_font::var::unmap_float(&map, v));
                let distances = [axis.default - axis.min, axis.max - axis.default];
                let convert = |value: f32, limits: [f32; 3]| {
                    let n = crate::instance::renormalize(value, limits, distances).clamp(-1.0, 1.0);
                    i16::try_from(crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(
                        n * 16384.0,
                    )))
                    .map_err(|_| limit)
                };
                pairs.retain_mut(|pair| {
                    let from = f32::from(pair.0) / 16384.0;
                    if !(unmapped[0]..=unmapped[2]).contains(&from) {
                        return false;
                    }
                    let Ok(new_from) = convert(from, unmapped) else {
                        return false;
                    };
                    let Ok(new_to) = convert(f32::from(pair.1) / 16384.0, mapped) else {
                        return false;
                    };
                    *pair = (new_from, new_to);
                    !matches!(*pair, (-16384, -16384) | (0, 0) | (16384, 16384))
                });
                pairs.extend([(-16384, -16384), (0, 0), (16384, 16384)]);
                pairs.sort_unstable();
            }
        }
        output.extend(u16::try_from(pairs.len()).map_err(|_| limit)?.to_be_bytes());
        for &(from, to) in &pairs {
            output.extend(from.to_be_bytes());
            output.extend(to.to_be_bytes());
        }
        *mappings.get_mut(old).ok_or(bad)? = pairs;
    }
    Ok((output, mappings))
}

fn subset_store(
    plan: &Plan,
    font: &Font<'_>,
    avar: Avar<'_>,
    mappings: &[Mapping],
) -> Result<(Vec<u8>, Vec<u8>), SubsetError> {
    let bad = SubsetError::Malformed { tag: TAG };
    let source = avar.variation_store().map_err(|_| bad)?;
    let source_map = avar.axis_index_map().map_err(|_| bad)?;
    let axes = crate::instance::axis_infos(font);
    let default_coords = axes
        .iter()
        .map(|a| {
            let value = plan
                .var
                .n
                .old_intermediates
                .get(&a.tag)
                .map_or(0.0, |r| r.default);
            hb_clamp_to_i32(hb_roundf32(value * 16384.0))
        })
        .collect::<Vec<_>>();
    let (mut floats, original_order) = if let Some(store) = source {
        project_store(plan, font, store)?
    } else {
        (Vec::new(), Vec::new())
    };
    let mut indices = axes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            source_map.map_or(u32::try_from(i).unwrap_or(u32::MAX), |m| {
                m.map(u32::try_from(i).unwrap_or(u32::MAX))
            })
        })
        .collect::<Vec<_>>();
    for index in &mut indices {
        let valid = source
            .and_then(|s| {
                u16::try_from(*index >> 16)
                    .ok()
                    .and_then(|o| s.item_data(o))
            })
            .is_some_and(|d| (*index & 0xffff) < u32::from(d.item_count));
        if !valid {
            *index = u32::MAX;
        }
    }
    let limit = SubsetError::Limit { tag: TAG };
    let mut processed_free = BTreeSet::new();
    for (axis, info) in axes.iter().enumerate() {
        if !plan.var.n.retained_axis_indices.contains(&axis)
            || !plan.var.user().contains_key(&info.tag)
        {
            continue;
        }
        let old = *indices.get(axis).ok_or(bad)?;
        if old == u32::MAX {
            continue;
        }
        let shared = axes.iter().enumerate().any(|(other, _)| {
            other != axis
                && plan.var.n.retained_axis_indices.contains(&other)
                && indices.get(other) == Some(&old)
        });
        if !shared {
            continue;
        }
        let outer = usize::try_from(old >> 16).map_err(|_| limit)?;
        let inner = usize::try_from(old & 0xffff).map_err(|_| limit)?;
        let (rows, cols) = floats.get_mut(outer).ok_or(bad)?;
        if inner >= *rows {
            return Err(bad);
        }
        for (_, values) in cols {
            let copy = *values.get(inner).ok_or(bad)?;
            values.push(copy);
        }
        let new_inner = *rows;
        *rows = rows.checked_add(1).ok_or(limit)?;
        *indices.get_mut(axis).ok_or(bad)? =
            (old & 0xffff0000) | u32::try_from(new_inner).map_err(|_| limit)?;
    }
    for (axis, info) in axes.iter().enumerate() {
        if !plan.var.n.retained_axis_indices.contains(&axis) {
            continue;
        }
        let old_idx = source_map.map_or(u32::try_from(axis).map_err(|_| limit)?, |m| {
            m.map(u32::try_from(axis).unwrap_or(u32::MAX))
        });
        let delta = default_delta(source, old_idx, &default_coords);
        let Some(range) = plan.var.user().get(&info.tag) else {
            if delta != 0.0
                && *indices.get(axis).ok_or(bad)? != u32::MAX
                && processed_free.insert(*indices.get(axis).ok_or(bad)?)
            {
                let idx = *indices.get(axis).ok_or(bad)?;
                add_single(
                    &mut floats,
                    usize::try_from(idx >> 16).map_err(|_| limit)?,
                    usize::try_from(idx & 0xffff).map_err(|_| limit)?,
                    alloc::vec![[0; 3]; axes.len()],
                    hb_clamp_to_i32(hb_roundf32(delta)),
                )?;
            }
            continue;
        };
        let old = plan.var.n.old_intermediates.get(&info.tag).ok_or(bad)?;
        if *indices.get(axis).ok_or(bad)? == u32::MAX {
            let outer = floats.len();
            floats.push((1, Vec::new()));
            *indices.get_mut(axis).ok_or(bad)? = u32::try_from(outer).map_err(|_| limit)? << 16;
        }
        let idx = *indices.get(axis).ok_or(bad)?;
        let outer = usize::try_from(idx >> 16).map_err(|_| limit)?;
        let inner = usize::try_from(idx & 0xffff).map_err(|_| limit)?;
        let fixed = |v: f32| hb_clamp_to_i32(hb_roundf32(v * 16384.0));
        let bias = fixed(old.default).saturating_add(hb_clamp_to_i32(hb_roundf32(delta)));
        if bias != 0 {
            add_single(
                &mut floats,
                outer,
                inner,
                alloc::vec![[0; 3]; axes.len()],
                bias,
            )?;
        }
        if range.min != range.max {
            let old_default = sofel_font::var::normalize_axis_value(
                range.min,
                range.default,
                range.max,
                info.default,
            );
            let old_default =
                map_new_mapping(mappings.get(axis).ok_or(bad)?, f64::from(old_default));
            let old_default = i16::try_from(hb_clamp_to_i32(hb_roundf32(
                crate::instance::f64_to_f32(old_default * 16384.0),
            )))
            .map_err(|_| limit)?;
            let source_map = avar.segment_map(axis).ok_or(bad)?;
            let breakpoints = mappings
                .get(axis)
                .ok_or(bad)?
                .iter()
                .filter_map(|&(from, to)| {
                    if matches!(from, -16384 | 0 | 16384) || matches!(to, -16384 | 0 | 16384) {
                        return None;
                    }
                    let z = f64::from(from) / 16384.0;
                    let user = f64::from(range.default)
                        + z * if z < 0.0 {
                            f64::from(range.default - range.min)
                        } else {
                            f64::from(range.max - range.default)
                        };
                    let normalized = sofel_font::var::normalize_axis_value(
                        info.min,
                        info.default,
                        info.max,
                        crate::instance::f64_to_f32(user),
                    );
                    let old_coord = sofel_font::var::map_float(&source_map, normalized);
                    Some((to, fixed(old_coord).saturating_sub(i32::from(to))))
                })
                .collect::<Vec<_>>();
            for (tent, delta) in super::avar::compensation_regions_many(
                [fixed(old.min), fixed(old.default), fixed(old.max)],
                old_default,
                &breakpoints,
            ) {
                if tent == [0; 3] {
                    continue;
                }
                let mut region = alloc::vec![[0; 3]; axes.len()];
                *region.get_mut(axis).ok_or(bad)? = tent;
                add_single(&mut floats, outer, inner, region, delta)?;
            }
        }
    }
    let mut used = BTreeSet::new();
    let mut order = Vec::new();
    let mut blocks = Vec::new();
    let mut long = false;
    for (count, columns) in floats {
        let mut cols = Vec::new();
        for (region, values) in columns {
            let rounded = values
                .into_iter()
                .map(|v| hb_clamp_to_i32(hb_roundf32(v)))
                .collect::<Vec<_>>();
            if !order.contains(&region) {
                order.push(region.clone());
            }
            if rounded.iter().any(|&v| v != 0) {
                long |= rounded.iter().any(|&v| i16::try_from(v).is_err());
                used.insert(region.clone());
                cols.push((region, rounded));
            }
        }
        blocks.push((count, cols));
    }
    let mut regions = Vec::new();
    for region in original_order.into_iter().chain(order) {
        if used.remove(&region) {
            regions.push(region);
        }
    }
    if regions.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let remap = super::hvar::optimize::optimize(&mut blocks, &regions, TAG)?;
    let mut output = Serializer::new();
    output.u16(1);
    let region_offset = output.u32(0);
    output.u16(u16::try_from(blocks.len()).map_err(|_| limit)?);
    let offsets = blocks.iter().map(|_| output.u32(0)).collect::<Vec<_>>();
    let ids = regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r, i))
        .collect::<BTreeMap<_, _>>();
    for ((count, cols), at) in blocks.iter().zip(offsets).rev() {
        output.set_u32(
            at,
            u32::try_from(output.current().len()).map_err(|_| limit)?,
        );
        super::hvar::instance::write_block(&mut output, *count, cols, &ids, long, TAG)?;
    }
    output.set_u32(
        region_offset,
        u32::try_from(output.current().len()).map_err(|_| limit)?,
    );
    output.u16(u16::try_from(plan.var.n.retained_axis_indices.len()).map_err(|_| limit)?);
    output.u16(u16::try_from(regions.len()).map_err(|_| limit)?);
    for region in regions {
        for &axis in &plan.var.n.retained_axis_indices {
            for &bits in region.get(axis).ok_or(bad)? {
                output.i16(
                    i16::try_from(hb_clamp_to_i32(hb_roundf32(f32::from_bits(bits) * 16384.0)))
                        .map_err(|_| limit)?,
                );
            }
        }
    }
    let mapped = plan
        .var
        .n
        .retained_axis_indices
        .iter()
        .map(|&i| {
            let index = *indices.get(i).ok_or(bad)?;
            Ok(if index == u32::MAX {
                index
            } else {
                *remap.get(&index).ok_or(bad)?
            })
        })
        .collect::<Result<Vec<_>, SubsetError>>()?;
    let map = super::hvar::encode_index_map(&mapped, TAG)?;
    Ok((output.current().to_vec(), map))
}
