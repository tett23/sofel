//! Region transforms for CFF2 blends (HarfBuzz's cff2_instancing_plan_t).
use super::cs2::Store;
use crate::tables::variation_region;
use crate::{Plan, SubsetError};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use sofel_font::Font;
use sofel_font::read::{Tag, item_variation::ItemVariationStore};

const TAG: Tag = Tag::new(b"CFF2");
type Region = Vec<[u32; 3]>;

pub(super) struct Transform {
    pub gains: Vec<f32>,
    // Each output column lists its nonzero coefficients in source order.
    pub columns: Vec<(Region, Vec<(usize, f32)>)>,
}

pub(super) struct Projection {
    pub transforms: Vec<Transform>,
    pub original: Vec<Region>,
}

fn key(region: &[[f32; 3]]) -> Region {
    region
        .iter()
        .map(|t| t.map(|v| if v == 0.0 { 0 } else { v.to_bits() }))
        .collect()
}

impl Projection {
    pub fn new(source: Store<'_>, plan: &Plan, font: &Font<'_>) -> Result<Self, SubsetError> {
        let bad = SubsetError::Malformed { tag: TAG };
        let start = source.at.and_then(|at| at.checked_add(2)).ok_or(bad)?;
        let store =
            ItemVariationStore::new(source.data.get(start..).ok_or(bad)?).map_err(|_| bad)?;
        let changes = variation_region::changes(font, plan);
        let mut tents = Vec::new();
        for index in 0..store.region_count() {
            let region = store.region(index).ok_or(bad)?;
            let mut axes = Vec::new();
            for axis in 0..region.len() {
                let (start, peak, end) = region.axis(axis).ok_or(bad)?;
                axes.push(if peak == 0 {
                    [0.0; 3]
                } else {
                    [start, peak, end].map(|v| f32::from(v) / 16384.0)
                });
            }
            tents.push(axes);
        }
        let mut transforms = Vec::new();
        for major in 0..store.data_count() {
            let data = store.item_data(major).ok_or(bad)?;
            let count = data.region_index_count();
            let gains = source.scalars(
                u32::from(major),
                &plan.var.n.normalized_coords,
                u32::from(count),
            );
            let mut positions = BTreeMap::new();
            let mut columns: Vec<(Region, Vec<(usize, f32)>)> = Vec::new();
            for column in 0..usize::from(count) {
                let index = data.region_index(column).ok_or(bad)?;
                let old = tents.get(usize::from(index)).ok_or(bad)?;
                for (region, scales) in
                    variation_region::split(old.clone(), &changes, usize::from(u16::MAX), TAG)?
                {
                    if region.iter().all(|t| t[1] == 0.0) {
                        continue;
                    }
                    let key = key(&region);
                    let pos = *positions.entry(key.clone()).or_insert_with(|| {
                        let next = columns.len();
                        columns.push((key, Vec::new()));
                        next
                    });
                    let weights = &mut columns.get_mut(pos).ok_or(bad)?.1;
                    let weight = scales.into_iter().product::<f32>();
                    if let Some(last) = weights.last_mut().filter(|v| v.0 == column) {
                        last.1 += weight;
                    } else {
                        weights.push((column, weight));
                    }
                }
            }
            transforms.push(Transform { gains, columns });
        }
        Ok(Self {
            transforms,
            original: tents.iter().map(|r| key(r)).collect(),
        })
    }
}

impl Transform {
    pub fn fold(&self, deltas: &[f64]) -> Option<f64> {
        (deltas.len() == self.gains.len()).then(|| {
            self.gains
                .iter()
                .zip(deltas)
                .map(|(&gain, delta)| f64::from(gain) * delta)
                .sum()
        })
    }

    pub fn deltas(&self, old: &[f64]) -> Option<Vec<f64>> {
        self.columns
            .iter()
            .map(|(_, weights)| {
                weights
                    .iter()
                    .map(|&(i, w)| Some(f64::from(w) * old.get(i)?))
                    .sum()
            })
            .collect()
    }
}

impl Projection {
    pub fn store(
        &self,
        used: &alloc::collections::BTreeSet<u32>,
        axes: &[usize],
    ) -> Result<(Vec<u8>, BTreeMap<u32, u16>), SubsetError> {
        let limit = SubsetError::Limit { tag: TAG };
        let word = |n: usize| u16::try_from(n).map_err(|_| limit);
        let offset = |n: usize| u32::try_from(n).map_err(|_| limit);
        let mut majors = BTreeMap::new();
        let mut columns = Vec::new();
        for &major in used {
            let transform = self
                .transforms
                .get(major as usize)
                .ok_or(SubsetError::Malformed { tag: TAG })?;
            if !transform.columns.is_empty() {
                majors.insert(major, word(columns.len())?);
                columns.push(&transform.columns);
            }
        }
        if columns.is_empty() {
            return Ok((Vec::new(), majors));
        }
        let mut regions: BTreeMap<&Region, Option<u16>> = columns
            .iter()
            .flat_map(|c| c.iter().map(|(r, _)| (r, None)))
            .collect();
        let mut order = Vec::new();
        for r in self
            .original
            .iter()
            .chain(columns.iter().flat_map(|c| c.iter().map(|(r, _)| r)))
        {
            if let Some(idx) = regions.get_mut(r)
                && idx.is_none()
            {
                *idx = Some(word(order.len())?);
                order.push(r);
            }
        }
        if order.len() > 0x7fff {
            return Err(limit);
        }
        let header = 8usize.saturating_add(4usize.saturating_mul(columns.len()));
        let mut offsets = Vec::new();
        let mut blocks = Vec::new();
        for cols in columns {
            offsets.extend(offset(header.saturating_add(blocks.len()))?.to_be_bytes());
            blocks.extend([0; 4]);
            blocks.extend(word(cols.len())?.to_be_bytes());
            for (r, _) in cols {
                blocks.extend(
                    regions
                        .get(r)
                        .copied()
                        .flatten()
                        .ok_or(limit)?
                        .to_be_bytes(),
                );
            }
        }
        let region_offset = offset(header.saturating_add(blocks.len()))?;
        let mut bytes = Vec::from([0, 0, 0, 1]);
        bytes.extend(region_offset.to_be_bytes());
        bytes.extend(word(majors.len())?.to_be_bytes());
        bytes.extend(offsets);
        bytes.extend(blocks);
        bytes.extend(word(axes.len())?.to_be_bytes());
        bytes.extend(word(order.len())?.to_be_bytes());
        for r in order {
            for &axis in axes {
                for &coord in r.get(axis).ok_or(limit)? {
                    let rounded = sofel_geom::float::hb_clamp_to_i32(
                        sofel_geom::float::hb_roundf32(f32::from_bits(coord) * 16384.0),
                    );
                    bytes.extend(i16::try_from(rounded).map_err(|_| limit)?.to_be_bytes());
                }
            }
        }
        let length = u16::try_from(bytes.len().saturating_sub(2)).unwrap_or(u16::MAX);
        bytes
            .get_mut(..2)
            .ok_or(limit)?
            .copy_from_slice(&length.to_be_bytes());
        Ok((bytes, majors))
    }
}
