//! Preserve outer/inner indices for an implicit advance map while rebasing its store.
use super::{
    BTreeMap, BTreeSet, Font, ItemVariationStore, Plan, Serializer, SubsetError, Tag, Vec,
};
use crate::tables::variation_region;
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32, i32_to_f32};

pub(crate) type Region = Vec<[u32; 3]>;
pub(crate) type Column = (Region, Vec<i32>);
pub(crate) type Block = (usize, Vec<Column>);

fn key(tents: &[[f32; 3]]) -> Region {
    tents
        .iter()
        .map(|axis| axis.map(|v| if v == 0.0 { 0 } else { v.to_bits() }))
        .collect()
}

pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    store: ItemVariationStore<'_>,
    items: &[(u16, Vec<u32>)],
    tag: Tag,
    optimize: bool,
    remap: &mut BTreeMap<u32, u32>,
) -> Result<Vec<u8>, SubsetError> {
    subset_inner(
        plan,
        font,
        store,
        items,
        (tag, optimize),
        remap,
        ConditionRows {
            values: &[],
            map: &mut BTreeMap::new(),
        },
    )
}

pub(crate) fn subset_gdef_conditions(
    plan: &Plan,
    font: &Font<'_>,
    store: ItemVariationStore<'_>,
    items: &[(u16, Vec<u32>)],
    remap: &mut BTreeMap<u32, u32>,
    conditions: &[(u32, i16, f64)],
    condition_map: &mut BTreeMap<(u32, i16), (u32, i16)>,
) -> Result<Vec<u8>, SubsetError> {
    subset_inner(
        plan,
        font,
        store,
        items,
        (Tag::new(b"GDEF"), true),
        remap,
        ConditionRows {
            values: conditions,
            map: condition_map,
        },
    )
}

struct ConditionRows<'a> {
    values: &'a [(u32, i16, f64)],
    map: &'a mut BTreeMap<(u32, i16), (u32, i16)>,
}

fn subset_inner(
    plan: &Plan,
    font: &Font<'_>,
    store: ItemVariationStore<'_>,
    items: &[(u16, Vec<u32>)],
    (tag, optimize): (Tag, bool),
    remap: &mut BTreeMap<u32, u32>,
    conditions: ConditionRows<'_>,
) -> Result<Vec<u8>, SubsetError> {
    let bad = SubsetError::Malformed { tag };
    let limit = SubsetError::Limit { tag };
    let changes = variation_region::changes(font, plan);
    let kept = &plan.var.n.retained_axis_indices;
    let mut original = Vec::new();
    for index in 0..store.region_count() {
        let region = store.region(index).ok_or(bad)?;
        let mut axes = Vec::new();
        for i in 0..region.len() {
            let (a, b, c) = region.axis(i).ok_or(bad)?;
            axes.push(if b == 0 {
                [0.0; 3]
            } else {
                [a, b, c].map(|v| f32::from(v) / 16384.0)
            });
        }
        original.push(axes);
    }
    let mut used = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut blocks = Vec::new();
    let mut private_blocks = Vec::new();
    let mut pending_conditions = BTreeMap::new();
    let mut long = false;
    for (outer, rows) in items {
        let data = store.item_data(*outer).ok_or(bad)?;
        let mut indices = BTreeMap::new();
        let mut columns: Vec<(Region, Vec<f32>)> = Vec::new();
        for col in 0..data.region_index_count() {
            let region = usize::from(data.region_index(usize::from(col)).ok_or(bad)?);
            let source = original.get(region).ok_or(bad)?;
            if crate::tables::avar::region_dead(
                source,
                &plan.var.n.axis_tags,
                &plan.var.n.avar2_reachable,
            ) {
                continue;
            }
            for (tents, scales) in
                variation_region::split(source.clone(), &changes, usize::from(u16::MAX), tag)?
            {
                let region = key(&tents);
                if region.iter().all(|axis| *axis == [0; 3]) {
                    continue;
                }
                let index = *indices.entry(region.clone()).or_insert_with(|| {
                    let index = columns.len();
                    columns.push((region, alloc::vec![0.0;rows.len()]));
                    index
                });
                let (_, values) = columns.get_mut(index).ok_or(bad)?;
                for (&row, value) in rows.iter().zip(values) {
                    let original = i32_to_f32(super::delta(&data, row, col));
                    *value += scales.iter().fold(original, |d, s| d * s);
                }
            }
        }
        for &(index, default, value) in conditions.values {
            if u16::try_from(index >> 16).ok() != Some(*outer) {
                continue;
            }
            let Some(inner) = rows.iter().position(|&row| row == (index & 0xffff)) else {
                continue;
            };
            let deltas = columns
                .iter()
                .map(|(_, values)| values.get(inner).copied().unwrap_or(0.0))
                .collect::<Vec<_>>();
            let variable = deltas.iter().any(|&delta| delta != 0.0);
            if !variable {
                let sign = if value > 0.0 { 1 } else { 0 };
                pending_conditions.insert((index, default), (u32::MAX, sign));
                continue;
            }
            let maximum = deltas
                .iter()
                .fold(value.abs(), |max, delta| max.max(f64::from(delta.abs())));
            let mut scale = 1.0f64;
            while maximum * scale > f64::from(i32::MAX) {
                scale *= 0.5;
            }
            while maximum * scale * 2.0 <= f64::from(i32::MAX) {
                let integral = value * scale == sofel_geom::float::hb_roundf(value * scale)
                    && deltas.iter().all(|&delta| {
                        let scaled = f64::from(delta) * scale;
                        scaled == sofel_geom::float::hb_roundf(scaled)
                    });
                if integral {
                    break;
                }
                scale *= 2.0;
            }
            let default_value =
                crate::instance::f64_to_i32(sofel_geom::float::hb_roundf(value * scale));
            let default_value = i16::try_from(default_value).map_err(|_| limit)?;
            let compact = if scale == 1.0 {
                (u32::try_from(blocks.len()).map_err(|_| limit)? << 16)
                    | u32::try_from(inner).map_err(|_| limit)?
            } else {
                let outer = items.len().checked_add(private_blocks.len()).ok_or(limit)?;
                let columns = columns
                    .iter()
                    .filter_map(|(region, values)| {
                        let scaled = f64::from(*values.get(inner)?) * scale;
                        let rounded =
                            crate::instance::f64_to_i32(sofel_geom::float::hb_roundf(scaled));
                        (rounded != 0).then(|| (region.clone(), alloc::vec![rounded]))
                    })
                    .collect();
                private_blocks.push((1, columns));
                u32::try_from(outer).map_err(|_| limit)? << 16
            };
            pending_conditions.insert((index, default), (compact, default_value));
        }
        let mut block = Vec::new();
        for (region, values) in columns {
            let rounded: Vec<_> = values
                .into_iter()
                .map(|d| hb_clamp_to_i32(hb_roundf32(d)))
                .collect();
            if seen.insert(region.clone()) {
                order.push(region.clone());
            }
            if rounded.iter().any(|&d| d != 0) {
                used.insert(region.clone());
                long |= rounded.iter().any(|&v| i16::try_from(v).is_err());
                block.push((region, rounded));
            }
        }
        blocks.push((rows.len(), block));
    }
    blocks.extend(private_blocks);
    let mut regions = Vec::new();
    for region in original.iter().map(|t| key(t)).chain(order) {
        if used.remove(&region) {
            regions.push(region);
        }
    }
    let indices: BTreeMap<_, _> = regions.iter().enumerate().map(|(i, r)| (r, i)).collect();
    let mut out = Serializer::new();
    out.u16(1);
    let region_offset = out.u32(0);
    if regions.is_empty() {
        return Ok(Vec::new());
    }
    if optimize {
        *remap = super::optimize::optimize(&mut blocks, &regions, tag)?;
    }
    for (source, (compact, default)) in pending_conditions {
        let index = if compact == u32::MAX {
            compact
        } else {
            *remap.get(&compact).ok_or(limit)?
        };
        conditions.map.insert(source, (index, default));
    }
    out.u16(u16::try_from(blocks.len()).map_err(|_| limit)?);
    let offsets: Vec<_> = blocks.iter().map(|_| out.u32(0)).collect();
    for ((rows, columns), off) in blocks.iter().zip(offsets).rev() {
        out.set_u32(off, u32::try_from(out.current().len()).map_err(|_| limit)?);
        write_block(&mut out, *rows, columns, &indices, long, tag)?;
    }
    out.set_u32(
        region_offset,
        u32::try_from(out.current().len()).map_err(|_| limit)?,
    );
    out.u16(u16::try_from(kept.len()).map_err(|_| limit)?);
    out.u16(u16::try_from(regions.len()).map_err(|_| limit)?);
    for r in regions {
        for &axis in kept {
            for bits in r.get(axis).ok_or(bad)? {
                let value = hb_clamp_to_i32(hb_roundf32(f32::from_bits(*bits) * 16384.0));
                out.i16(i16::try_from(value).map_err(|_| limit)?);
            }
        }
    }
    Ok(out.current().to_vec())
}

pub(crate) fn write_block(
    out: &mut Serializer,
    rows: usize,
    columns: &[Column],
    indices: &BTreeMap<&Region, usize>,
    long: bool,
    tag: Tag,
) -> Result<(), SubsetError> {
    let limit = SubsetError::Limit { tag };
    let mut fields = Vec::new();
    for (region, values) in columns {
        let width = values
            .iter()
            .map(|d| match d {
                -128..=127 => 1,
                -32768..=32767 => 2,
                _ => 4,
            })
            .max()
            .unwrap_or(1);
        fields.push((width, *indices.get(region).ok_or(limit)?, values));
    }
    let short: usize = if long { 2 } else { 1 };
    fields.sort_by_key(|f| (f.0 <= short, f.1));
    let words = fields.iter().take_while(|f| f.0 > short).count();
    out.u16(u16::try_from(rows).map_err(|_| limit)?);
    out.u16(
        u16::try_from(words)
            .ok()
            .filter(|w| *w <= 0x7fff)
            .ok_or(limit)?
            | if long { 0x8000 } else { 0 },
    );
    out.u16(u16::try_from(fields.len()).map_err(|_| limit)?);
    for f in &fields {
        out.u16(u16::try_from(f.1).map_err(|_| limit)?);
    }
    for row in 0..rows {
        for (width, _, values) in &fields {
            let value = values.get(row).ok_or(limit)?.to_be_bytes();
            let bytes = if *width > short {
                short.saturating_mul(2)
            } else {
                short
            };
            out.extend(value.get(4usize.saturating_sub(bytes)..).ok_or(limit)?);
        }
    }
    Ok(())
}
