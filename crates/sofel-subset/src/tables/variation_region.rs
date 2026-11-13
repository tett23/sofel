//! Rebase a tuple's support on a restricted axis, following HarfBuzz's
//! hb-subset-instancer-solver.cc (Copyright © 2023 Behdad Esfahbod).
use crate::instance::renormalize;
use alloc::vec::Vec;

pub(super) type Tent = [f32; 3];

type ScaledRegion = (Vec<Tent>, Vec<f32>);
pub(super) type Change = (usize, Tent, [f32; 2]);
pub(super) fn changes(font: &sofel_font::Font<'_>, plan: &crate::plan::Plan) -> Vec<Change> {
    let axis_infos = crate::instance::axis_infos(font);
    plan.var
        .n
        .axes_location
        .iter()
        .filter_map(|(tag, range)| {
            let axis = axis_infos.iter().position(|info| info.tag == *tag)?;
            let info = axis_infos.get(axis)?;
            Some((
                axis,
                [range.min, range.default, range.max],
                [info.default - info.min, info.max - info.default],
            ))
        })
        .collect()
}
pub(super) fn split(
    source: Vec<Tent>,
    changed: &[Change],
    limit: usize,
    tag: sofel_font::read::Tag,
) -> Result<Vec<ScaledRegion>, crate::error::SubsetError> {
    use crate::error::SubsetError;
    let bad = SubsetError::Malformed { tag };
    let mut parts = alloc::vec![(source, Vec::<f32>::new())];
    for &(axis, limits, distances) in changed {
        let mut next = Vec::new();
        for (tents, scales) in parts {
            let tent = *tents.get(axis).ok_or(bad)?;
            let solutions = rebase(tent, limits, distances);
            for (weight, region) in solutions {
                if next.len() >= limit {
                    return Err(SubsetError::Limit { tag });
                }
                let mut tents = tents.clone();
                *tents.get_mut(axis).ok_or(bad)? = region;
                let mut scales = scales.clone();
                if weight != 1.0 {
                    scales.push(weight);
                }
                next.push((tents, scales));
            }
        }
        parts = next;
    }
    Ok(parts)
}

fn reverse([a, b, c]: Tent) -> Tent {
    [-c, -b, -a]
}

fn support(value: f32, [lower, peak, upper]: Tent) -> f32 {
    if value == peak {
        1.0
    } else if value <= lower || value >= upper {
        0.0
    } else if value < peak {
        (value - lower) / (peak - lower)
    } else {
        (upper - value) / (upper - peak)
    }
}

fn solve(tent: Tent, limits: Tent) -> Vec<(f32, Tent)> {
    let [mut lower, peak, mut upper] = tent;
    let [min, default, max] = limits;
    if default > peak {
        return solve(reverse(tent), reverse(limits))
            .into_iter()
            .map(|(weight, t)| (weight, reverse(t)))
            .collect();
    }
    if max <= lower && max < peak {
        return Vec::new();
    }
    if max < peak {
        let mult = support(max, tent);
        return solve([lower, max, max], limits)
            .into_iter()
            .map(|(weight, t)| (weight * mult, t))
            .collect();
    }
    let gain = support(default, tent);
    let edge = support(max, tent);
    let mut out = alloc::vec![(gain, [0.0; 3])];
    if gain >= edge {
        let crossing = peak + (1.0 - gain) * (upper - peak);
        out.push((1.0 - gain, [lower.max(default), peak, crossing]));
        if upper >= max {
            out.push((edge - gain, [crossing, max, max]));
        } else {
            if upper == default {
                upper += 1.0 / 16384.0;
            }
            out.push((-gain, [crossing, upper, max]));
            out.push((-gain, [upper, max, max]));
        }
    } else {
        out.push((1.0 - gain, [default.max(lower), peak, max]));
        if peak < max {
            out.push((edge - gain, [peak, max, max]));
        }
    }
    if lower <= min {
        out.push((support(min, tent) - gain, [min, min, default]));
    } else {
        if lower == default {
            lower -= 1.0 / 16384.0;
        }
        out.push((-gain, [min, lower, default]));
        out.push((-gain, [min, min, lower]));
    }
    out
}

pub(super) fn rebase(tent: Tent, limits: Tent, distances: [f32; 2]) -> Vec<(f32, Tent)> {
    let [lower, peak, upper] = tent;
    if peak == 0.0 {
        return alloc::vec![(1.0, [0.0; 3])];
    }
    if lower > peak || peak > upper || lower < 0.0 && upper > 0.0 {
        return Vec::new();
    }
    solve(tent, limits)
        .into_iter()
        .filter(|(weight, _)| *weight != 0.0)
        .map(|(weight, t)| {
            (
                weight,
                if t == [0.0; 3] {
                    t
                } else {
                    t.map(|v| renormalize(v, limits, distances))
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0061_rebase_preserves_the_default_gain_and_both_sides() {
        for (tent, limit) in [
            ([0.0, 1.0, 1.0], [-1.0, 0.0, 0.5]),
            ([0.0, 0.25, 1.0], [-1.0, 0.0, 0.5]),
            ([0.1, 0.5, 0.8], [0.25, 0.4, 0.9]),
            ([-1.0, -0.25, 0.0], [-0.8, -0.4, 0.0]),
            ([0.25, 0.5, 0.75], [0.5, 0.5, 0.5]),
        ] {
            let parts = rebase(tent, limit, [300.0, 500.0]);
            for step in 0i16..=128 {
                let value = limit[0] + (limit[2] - limit[0]) * f32::from(step) / 128.0;
                let mapped = renormalize(value, limit, [300.0, 500.0]);
                let actual: f32 = parts
                    .iter()
                    .map(|(scalar, region)| {
                        scalar
                            * if *region == [0.0; 3] {
                                1.0
                            } else {
                                support(mapped, *region)
                            }
                    })
                    .sum();
                assert!((support(value, tent) - actual).abs() < 0.00001);
            }
        }
    }
}
