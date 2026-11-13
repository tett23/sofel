//! Removes pinned axes from an unchanged avar version 1 map (ADR 0061).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::read::{Tag, avar::Avar};

use crate::error::SubsetError;
use crate::plan::Plan;

const TAG: Tag = Tag::new(b"avar");

#[allow(dead_code)] // Used by the avar2 planner once the serializer is wired.
pub(crate) struct Avar2Ranges {
    pub reachable: BTreeMap<Tag, crate::input::AxisRange>,
    pub self_contained: BTreeMap<Tag, f32>,
}

/// Bounds the original avar2 coordinates over the requested user box.
/// A pinned axis is removable only when its delta is constant on that box.
#[allow(dead_code, clippy::indexing_slicing, clippy::cast_possible_truncation)]
pub(crate) fn avar2_reachable_ranges(
    font: &sofel_font::Font<'_>,
    n: &crate::instance::Normalized,
    detect_self_contained: bool,
) -> Result<Avar2Ranges, SubsetError> {
    let bad = SubsetError::Malformed { tag: TAG };
    let source = font.table(TAG).ok_or(bad)?;
    let avar = Avar::new(source).map_err(|_| bad)?;
    let store = avar.variation_store().map_err(|_| bad)?;
    let index_map = avar.axis_index_map().map_err(|_| bad)?;
    let axes = crate::instance::axis_infos(font);
    let box_: Vec<[f64; 2]> = axes
        .iter()
        .map(|axis| {
            if let Some(range) = n.axes_location.get(&axis.tag) {
                [f64::from(range.min), f64::from(range.max)]
            } else if axis.hidden {
                [0.0, 0.0]
            } else {
                [-1.0, 1.0]
            }
        })
        .collect();
    let mut result = Avar2Ranges {
        reachable: BTreeMap::new(),
        self_contained: BTreeMap::new(),
    };
    for (axis, info) in axes.iter().enumerate() {
        let index = index_map.map_or(u32::try_from(axis).map_err(|_| bad)?, |m| {
            m.map(u32::try_from(axis).unwrap_or(u32::MAX))
        });
        let data = store.and_then(|s| u16::try_from(index >> 16).ok().and_then(|o| s.item_data(o)));
        let mut row = Vec::new();
        if let (Some(store), Some(data)) = (store, data)
            && let Ok(inner) = u16::try_from(index & 0xffff)
            && inner < data.item_count
        {
            for column in 0..data.region_index_count() {
                let delta = data.delta(inner, column).ok_or(bad)?;
                if delta == 0 {
                    continue;
                }
                let region_id = data.region_index(usize::from(column)).ok_or(bad)?;
                let region = store.region(region_id).ok_or(bad)?;
                let tents = (0..axes.len())
                    .map(|i| {
                        region
                            .axis(i)
                            .map(|(start, peak, end)| {
                                [start, peak, end].map(|v| f64::from(v) / 16384.0)
                            })
                            .ok_or(bad)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                row.push((delta, tents));
            }
        }
        let (delta, final_coord) = avar2_reachable_range(&box_, axis, &row);
        if detect_self_contained
            && delta[0] == delta[1]
            && n.user.get(&info.tag).is_some_and(|r| r.min == r.max)
        {
            let intermediate = box_[axis][0];
            let fixed = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(
                crate::instance::f64_to_f32(intermediate * 16384.0),
            ));
            let delta_fixed = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(
                crate::instance::f64_to_f32(delta[0]),
            ));
            let final_fixed = fixed.saturating_add(delta_fixed).clamp(-16384, 16384);
            result
                .self_contained
                .insert(info.tag, final_fixed as f32 / 16384.0);
            continue;
        }
        let lo =
            sofel_geom::float::floor((final_coord[0] - 1.0 / 16384.0).clamp(-1.0, 1.0) * 16384.0)
                / 16384.0;
        let hi =
            -sofel_geom::float::floor(-(final_coord[1] + 1.0 / 16384.0).clamp(-1.0, 1.0) * 16384.0)
                / 16384.0;
        if lo > -1.0 || hi < 1.0 {
            result.reachable.insert(
                info.tag,
                crate::input::AxisRange {
                    min: lo as f32,
                    default: 0.0_f64.clamp(lo, hi) as f32,
                    max: hi as f32,
                },
            );
        }
    }
    Ok(result)
}

/// Checks that the selected axes use identity knots and both v2 offsets are null.
pub(crate) fn is_empty_identity_v2(
    data: &[u8],
    axes: &[Tag],
    selected: impl Fn(Tag) -> bool,
) -> bool {
    Avar::new(data).is_ok_and(|table| {
        table.v2_offsets() == Some((0, 0))
            && table.axis_count() == axes.len()
            && axes
                .iter()
                .enumerate()
                .filter(|(_, tag)| selected(**tag))
                .all(|(axis, _)| {
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
    let axes = crate::instance::axis_infos(font);
    for &old in &plan.var.n.retained_axis_indices {
        let map = avar
            .segment_map(old)
            .ok_or(SubsetError::Malformed { tag: TAG })?;
        let info = axes.get(old).ok_or(SubsetError::Malformed { tag: TAG })?;
        let mut pairs = Vec::with_capacity(map.len());
        for i in 0..map.len() {
            let (from, to) = map.pair(i).ok_or(SubsetError::Malformed { tag: TAG })?;
            pairs.push((from.0, to.0));
        }
        if let Some(range) = plan.var.n.axes_location.get(&info.tag) {
            let mapped = [range.min, range.default, range.max];
            let unmapped = mapped.map(|value| sofel_font::var::unmap_float(&map, value));
            let distances = [info.default - info.min, info.max - info.default];
            let convert = |v, limits| {
                let n = crate::instance::renormalize(v, limits, distances).clamp(-1.0, 1.0);
                i16::try_from(crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(
                    n * 16384.0,
                )))
                .unwrap_or_default()
            };
            const REQUIRED: [(i16, i16); 3] = [(-16384, -16384), (0, 0), (16384, 16384)];
            pairs.retain_mut(|pair| {
                let from = f32::from(pair.0) / 16384.0;
                if !(unmapped[0]..=unmapped[2]).contains(&from) {
                    return false;
                }
                *pair = (
                    convert(from, unmapped),
                    convert(f32::from(pair.1) / 16384.0, mapped),
                );
                !REQUIRED.contains(pair)
            });
            pairs.extend(REQUIRED);
            pairs.sort_unstable();
        }
        let len = u16::try_from(pairs.len()).map_err(|_| SubsetError::Limit { tag: TAG })?;
        out.extend(len.to_be_bytes());
        for (from, to) in pairs {
            out.extend(from.to_be_bytes());
            out.extend(to.to_be_bytes());
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
    // The v1 map is renormalized in rounded intermediate coordinates,
    // while avar2 also retains the old default in user coordinates.
    let negative = info.default - info.min;
    let positive = info.max - info.default;
    let distance = |v: f32| v * if v < 0.0 { negative } else { positive };
    let mapped = sofel_font::var::normalize_axis_value(
        distance(original.min),
        distance(original.default),
        distance(original.max),
        0.0,
    );
    let mapped = i16::try_from(fixed(mapped)).map_err(|_| failed)?;
    let z = f64::from(mapped) / 16384.0;
    let default_user = f64::from(range.default);
    let user_at_knot = default_user
        + z * if z < 0.0 {
            default_user - f64::from(range.min)
        } else {
            f64::from(range.max) - default_user
        };
    let old = normalized(user_at_knot, [info.min, info.default, info.max]);
    let extra = (
        mapped,
        fixed(map_identity(old)).saturating_sub(i32::from(mapped)),
    );
    let regions = compensation_regions(
        [
            fixed(original.min),
            fixed(original.default),
            fixed(original.max),
        ],
        old_default,
        Some((
            extra,
            [info.min, info.default, info.max],
            [range.min, range.default, range.max],
        )),
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
    let avar = Avar::new(src).map_err(|_| failed)?;
    for i in 0..axes.len() {
        if i != axis {
            let segment = avar.segment_map(i).ok_or(failed)?;
            output.extend(
                u16::try_from(segment.len())
                    .map_err(|_| failed)?
                    .to_be_bytes(),
            );
            for j in 0..segment.len() {
                let (from, to) = segment.pair(j).ok_or(failed)?;
                output.extend(from.0.to_be_bytes());
                output.extend(to.0.to_be_bytes());
            }
            continue;
        }
        let mut knots = alloc::vec![-16384i16, 0, 16384];
        if i == axis && !knots.contains(&mapped) {
            knots.insert(knots.partition_point(|&v| v < mapped), mapped);
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

// SegmentMaps::map_float interpolates from the lower knot in f32, even
// on an identity segment. Preserve its rounding near the zero knot.
fn map_identity(value: f64) -> f32 {
    let value = crate::instance::f64_to_f32(value);
    if value < 0.0 {
        (value + 1.0) - 1.0
    } else {
        value
    }
}

fn normalized(user: f64, [min, default, max]: [f32; 3]) -> f64 {
    let [min, default, max] = [min, default, max].map(f64::from);
    let value = user.clamp(min, max) - default;
    if value == 0.0 {
        0.0
    } else {
        value
            / if value < 0.0 {
                default - min
            } else {
                max - default
            }
    }
}

type MappingBreakpoint = ((i16, i32), [f32; 3], [f32; 3]);

fn residual(knots: &[(i16, i32)], old: [f32; 3], new: [f32; 3], mapped: i16) -> f64 {
    let [min, default, max] = new.map(f64::from);
    let user_at = |v: i16| {
        default + f64::from(v) / 16384.0 * if v < 0 { default - min } else { max - default }
    };
    let mut largest = 0.0f64;
    let grid = (0..=256).map(|i| min + (max - min) * f64::from(i) / 256.0);
    for user in grid
        .chain(old.map(f64::from))
        .chain([user_at(mapped)])
        .chain(knots.iter().map(|k| user_at(k.0)))
    {
        if !(min..=max).contains(&user) {
            continue;
        }
        let z = normalized(user, new) * 16384.0;
        let Some(pair) = knots
            .windows(2)
            .find(|p| p.last().is_some_and(|k| f64::from(k.0) >= z))
        else {
            continue;
        };
        if let [a, b] = pair {
            let offset = f64::from(a.1)
                + (z - f64::from(a.0)) / (f64::from(b.0) - f64::from(a.0))
                    * (f64::from(b.1) - f64::from(a.1));
            let round = |v| sofel_geom::float::hb_roundf32(crate::instance::f64_to_f32(v));
            largest = largest.max(
                (round(f64::from(map_identity(normalized(user, old))) * 16384.0)
                    - round(z + offset))
                .abs(),
            );
        }
    }
    largest
}

/// Builds the bias and single-axis tent deltas in F2Dot14 units for
/// an identity segment map whose user range has changed.
fn compensation_regions(
    [min, default, max]: [i32; 3],
    old_default: i16,
    mapping: Option<MappingBreakpoint>,
) -> Vec<([i16; 3], i32)> {
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
    if let Some((extra, old, new)) = mapping
        && let Err(at) = knots.binary_search_by_key(&extra.0, |k| k.0)
    {
        let mut candidate = knots.clone();
        candidate.insert(at, extra);
        if residual(&candidate, old, new, extra.0) <= residual(&knots, old, new, extra.0) {
            knots = candidate;
        }
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

// The variation delta is piecewise multilinear. Its extrema and those of
// identity + delta occur on the grid of input bounds and active tent knots.
// HarfBuzz enumerates at most 16384 points and then uses interval bounds.
#[allow(dead_code, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn avar2_reachable_range(
    box_: &[[f64; 2]],
    target: usize,
    row: &[(i32, Vec<[f64; 3]>)],
) -> ([f64; 2], [f64; 2]) {
    let mut points = alloc::vec![alloc::vec![0.0]; box_.len()];
    if let (Some(&[lo, hi]), Some(grid)) = (box_.get(target), points.get_mut(target)) {
        *grid = alloc::vec![lo, hi];
    }
    for (_, region) in row {
        for (axis, &[start, peak, end]) in region.iter().enumerate() {
            if peak == 0.0 || start > peak || peak > end || start < 0.0 && end > 0.0 {
                continue;
            }
            if let (Some(&[lo, hi]), Some(grid)) = (box_.get(axis), points.get_mut(axis)) {
                grid.extend([lo, hi]);
                grid.extend([start.clamp(lo, hi), peak.clamp(lo, hi), end.clamp(lo, hi)]);
            }
        }
    }
    for grid in &mut points {
        grid.sort_by(f64::total_cmp);
        grid.dedup();
    }
    let exact = points
        .iter()
        .try_fold(1usize, |total, p| total.checked_mul(p.len()))
        .is_some_and(|count| count <= 16384);
    let tent = |value: f64, [start, peak, end]: [f64; 3]| {
        if peak == 0.0 || start > peak || peak > end || start < 0.0 && end > 0.0 || value == peak {
            1.0
        } else if value <= start || value >= end {
            0.0
        } else if value < peak {
            (value - start) / (peak - start)
        } else {
            (end - value) / (end - peak)
        }
    };
    if exact {
        let mut positions = alloc::vec![0usize; points.len()];
        let mut deltas = [f64::INFINITY, f64::NEG_INFINITY];
        let mut finals = [f64::INFINITY, f64::NEG_INFINITY];
        loop {
            let delta = row
                .iter()
                .map(|(d, region)| {
                    let scalar = region
                        .iter()
                        .enumerate()
                        .map(|(axis, &t)| {
                            let at = points
                                .get(axis)
                                .and_then(|p| p.get(positions[axis]))
                                .copied()
                                .unwrap_or(0.0);
                            tent(at, t)
                        })
                        .product::<f64>();
                    f64::from(*d) * scalar
                })
                .sum::<f64>();
            let identity = points
                .get(target)
                .and_then(|p| p.get(positions[target]))
                .copied()
                .unwrap_or(0.0);
            let final_coord = identity + delta / 16384.0;
            deltas = [deltas[0].min(delta), deltas[1].max(delta)];
            finals = [finals[0].min(final_coord), finals[1].max(final_coord)];
            let mut axis = 0;
            while axis < positions.len() {
                positions[axis] += 1;
                if positions[axis] < points[axis].len() {
                    break;
                }
                positions[axis] = 0;
                axis += 1;
            }
            if axis == positions.len() {
                break;
            }
        }
        (deltas, finals)
    } else {
        let mut deltas = [0.0; 2];
        for (delta, region) in row {
            let (low, high) =
                region
                    .iter()
                    .enumerate()
                    .fold((1.0, 1.0), |(low, high), (axis, &t)| {
                        let [a, b] = box_.get(axis).copied().unwrap_or([-1.0, 1.0]);
                        let x = tent(a, t);
                        let y = tent(b, t);
                        let maximum = if a <= t[1] && t[1] <= b {
                            1.0
                        } else {
                            x.max(y)
                        };
                        (low * x.min(y), high * maximum)
                    });
            let values = [f64::from(*delta) * low, f64::from(*delta) * high];
            deltas[0] += values[0].min(values[1]);
            deltas[1] += values[0].max(values[1]);
        }
        let [lo, hi] = box_.get(target).copied().unwrap_or([-1.0, 1.0]);
        (deltas, [lo + deltas[0] / 16384.0, hi + deltas[1] / 16384.0])
    }
}

#[cfg(test)]
mod tests {
    use super::compensation_regions;

    #[test]
    fn adr0061_i1_avar2_reachable_range_detects_driven_pins_and_culling() {
        use super::avar2_reachable_range;
        // The target axis is pinned at 0.5, while a free source axis drives
        // its final coordinate through a tent with a quarter-axis delta.
        let row = [(4096, alloc::vec![[0.0, 0.0, 0.0], [0.0, 1.0, 1.0]])];
        let (delta, final_range) = avar2_reachable_range(&[[0.5, 0.5], [-1.0, 1.0]], 0, &row);
        assert_eq!(delta, [0.0, 4096.0]);
        assert_eq!(final_range, [0.5, 0.75]);
        let (_, restricted) = avar2_reachable_range(&[[0.5, 0.5], [-1.0, 0.0]], 0, &row);
        assert_eq!(restricted, [0.5, 0.5]);
    }

    #[test]
    fn adr0061_i1_avar2_reachable_range_ignores_unreferenced_axes() {
        use super::avar2_reachable_range;
        let mut box_ = alloc::vec![[-1.0, 1.0]; 20];
        box_[0] = [0.5, 0.5];
        let mut region = alloc::vec![[0.0, 0.0, 0.0]; 20];
        region[1] = [0.0, 1.0, 1.0];
        let row = [(4096, region.clone()), (-4096, region)];
        let (delta, final_range) = avar2_reachable_range(&box_, 0, &row);
        assert_eq!(delta, [0.0, 0.0]);
        assert_eq!(final_range, [0.5, 0.5]);
    }

    #[test]
    fn i2_identity_range_compensation_uses_old_default_as_an_interior_knot() {
        assert_eq!(
            compensation_regions([-16384, 8192, 16384], -8192, None),
            [
                ([0, 0, 0], 8192),
                ([-16384, -16384, -8192], -8192),
                ([0, 16384, 16384], -8192)
            ]
        );
        assert_eq!(
            compensation_regions([0, 8192, 16384], -16384, None),
            [
                ([0, 0, 0], 8192),
                ([-16384, -16384, 0], 8192),
                ([0, 16384, 16384], -8192)
            ]
        );
    }
}
