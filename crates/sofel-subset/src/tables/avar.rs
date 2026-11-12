//! Removes pinned axes from an unchanged avar version 1 map (ADR 0061).

use alloc::vec::Vec;

use sofel_font::read::{Tag, avar::Avar};

use crate::error::SubsetError;
use crate::plan::Plan;

const TAG: Tag = Tag::new(b"avar");

/// Checks for the three identity knots on every axis and both null v2 offsets.
pub(crate) fn is_empty_identity_v2(data: &[u8], axes: usize) -> bool {
    Avar::new(data).is_ok_and(|table| {
        table.v2_offsets() == Some((0, 0))
            && table.axis_count() == axes
            && (0..axes).all(|axis| {
                table.segment_map(axis).is_some_and(|map| {
                    map.len() == 3
                        && [-16384, 0, 16384]
                            .into_iter()
                            .enumerate()
                            .all(|(index, value)| {
                                map.pair(index)
                                    .is_some_and(|(from, to)| from.0 == value && to.0 == value)
                            })
                })
            })
    })
}

/// `avar::subset` for a plan that only pins axes at their defaults.
pub(crate) fn subset(
    plan: &Plan,
    font: &sofel_font::Font<'_>,
    src: &[u8],
) -> Result<Vec<u8>, SubsetError> {
    let avar = Avar::new(src).map_err(|_| SubsetError::Malformed { tag: TAG })?;
    if avar.major_version() == 2 {
        return subset_identity_range(plan, font, src);
    }
    if avar.major_version() != 1 {
        return Err(SubsetError::Unsupported {
            tag: TAG,
            reason: "avar version 2 instancing",
        });
    }
    let count = u16::try_from(plan.var.n.retained_axis_indices.len())
        .map_err(|_| SubsetError::Failed { tag: TAG })?;
    let mut out = Vec::from([0, 1, 0, 0, 0, 0]);
    out.extend_from_slice(&count.to_be_bytes());
    for &old in &plan.var.n.retained_axis_indices {
        let map = avar
            .segment_map(old)
            .ok_or(SubsetError::Malformed { tag: TAG })?;
        let len = u16::try_from(map.len()).map_err(|_| SubsetError::Failed { tag: TAG })?;
        out.extend_from_slice(&len.to_be_bytes());
        for index in 0..map.len() {
            let (from, to) = map.pair(index).ok_or(SubsetError::Malformed { tag: TAG })?;
            out.extend_from_slice(&from.0.to_be_bytes());
            out.extend_from_slice(&to.0.to_be_bytes());
        }
    }
    Ok(out)
}

/// Serialize one restricted axis in an otherwise unchanged identity mapping.
fn subset_identity_range(
    plan: &Plan,
    font: &sofel_font::Font<'_>,
    src: &[u8],
) -> Result<Vec<u8>, SubsetError> {
    let failed = SubsetError::Failed { tag: TAG };
    let (tag, range) = plan.var.user().first_key_value().ok_or(failed)?;
    let axes = crate::instance::axis_infos(font);
    let (axis, info) = axes
        .iter()
        .enumerate()
        .find(|(_, a)| a.tag == *tag)
        .ok_or(failed)?;
    let original = plan.var.n.axes_location.get(tag).ok_or(failed)?;
    let fixed = |v: f32| crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(v * 16384.0));
    let old_default =
        sofel_font::var::normalize_axis_value(range.min, range.default, range.max, info.default);
    let old_default = i16::try_from(fixed(old_default)).map_err(|_| failed)?;
    let regions = compensation_regions(
        [
            fixed(original.min),
            fixed(original.default),
            fixed(original.max),
        ],
        old_default,
    );
    let count = u16::try_from(regions.len()).map_err(|_| failed)?;
    let axes_count = u16::try_from(axes.len()).map_err(|_| failed)?;
    let mut indices = alloc::vec![u32::MAX; axes.len()];
    *indices.get_mut(axis).ok_or(failed)? = 0;
    let map = super::hvar::encode_index_map(&indices, TAG)?;
    let mut data = Vec::new();
    let mut columns: Vec<_> = regions.iter().enumerate().collect();
    let wide = columns.iter().any(|(_, (_, d))| i16::try_from(*d).is_err());
    let needs_word = |d: i32| {
        if wide {
            i16::try_from(d).is_err()
        } else {
            i8::try_from(d).is_err()
        }
    };
    columns.sort_by_key(|(_, (_, d))| !needs_word(*d));
    let words = u16::try_from(columns.iter().filter(|(_, (_, d))| needs_word(*d)).count())
        .map_err(|_| failed)?;
    data.extend(1u16.to_be_bytes());
    data.extend((words | if wide { 0x8000 } else { 0 }).to_be_bytes());
    data.extend(count.to_be_bytes());
    for (index, _) in &columns {
        data.extend(u16::try_from(*index).map_err(|_| failed)?.to_be_bytes());
    }
    for (_, (_, delta)) in columns {
        let width = match (wide, needs_word(*delta)) {
            (true, true) => 4,
            (false, false) => 1,
            _ => 2,
        };
        data.extend(
            delta
                .to_be_bytes()
                .iter()
                .skip(4usize.saturating_sub(width)),
        );
    }
    let mut store = Vec::new();
    store.extend(1u16.to_be_bytes());
    store.extend(
        u32::try_from(data.len().checked_add(12).ok_or(failed)?)
            .map_err(|_| failed)?
            .to_be_bytes(),
    );
    store.extend(1u16.to_be_bytes());
    store.extend(12u32.to_be_bytes());
    store.extend(data);
    store.extend(axes_count.to_be_bytes());
    store.extend(count.to_be_bytes());
    for (region, _) in regions {
        for i in 0..axes.len() {
            for coord in if i == axis { region } else { [0; 3] } {
                store.extend(coord.to_be_bytes());
            }
        }
    }
    let mut output = src.get(..8).ok_or(failed)?.to_vec();
    for i in 0..axes.len() {
        let mut knots = alloc::vec![-16384i16, 0, 16384];
        if i == axis && !knots.contains(&old_default) {
            knots.insert(knots.partition_point(|&v| v < old_default), old_default);
        }
        output.extend(
            u16::try_from(knots.len())
                .map_err(|_| failed)?
                .to_be_bytes(),
        );
        for knot in knots {
            output.extend(knot.to_be_bytes());
            output.extend(knot.to_be_bytes());
        }
    }
    let store_offset = output.len().checked_add(8).ok_or(failed)?;
    output.extend(
        u32::try_from(store_offset.checked_add(store.len()).ok_or(failed)?)
            .map_err(|_| failed)?
            .to_be_bytes(),
    );
    output.extend(
        u32::try_from(store_offset)
            .map_err(|_| failed)?
            .to_be_bytes(),
    );
    output.extend(store);
    output.extend(map);
    Ok(output)
}

/// Builds the bias and single-axis tent deltas in F2Dot14 units for
/// an identity segment map whose user range has changed.
fn compensation_regions([min, default, max]: [i32; 3], old_default: i16) -> Vec<([i16; 3], i32)> {
    const UNIT: i16 = 16384;
    let mut knots = alloc::vec![
        (-UNIT, min.saturating_add(i32::from(UNIT))),
        (0, default),
        (UNIT, max.saturating_sub(i32::from(UNIT)))
    ];
    if old_default != 0 && old_default.abs_diff(0) < UNIT.cast_unsigned() {
        let index = knots.partition_point(|&(at, _)| at < old_default);
        knots.insert(
            index,
            (old_default, i32::from(old_default).saturating_neg()),
        );
    }
    let mut regions = Vec::new();
    if default != 0 {
        regions.push(([0; 3], default));
    }
    for (index, &(peak, offset)) in knots.iter().enumerate() {
        if peak == 0 || offset == default {
            continue;
        }
        let start = index
            .checked_sub(1)
            .and_then(|i| knots.get(i))
            .map_or(peak, |k| k.0);
        let end = knots.get(index.saturating_add(1)).map_or(peak, |k| k.0);
        regions.push(([start, peak, end], offset.saturating_sub(default)));
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::compensation_regions;

    #[test]
    fn i2_identity_range_compensation_uses_old_default_as_an_interior_knot() {
        assert_eq!(
            compensation_regions([-16384, 8192, 16384], -8192),
            [
                ([0, 0, 0], 8192),
                ([-16384, -16384, -8192], -8192),
                ([0, 16384, 16384], -8192)
            ]
        );
        assert_eq!(
            compensation_regions([0, 8192, 16384], -16384),
            [
                ([0, 0, 0], 8192),
                ([-16384, -16384, 0], 8192),
                ([0, 16384, 16384], -8192)
            ]
        );
    }
}
