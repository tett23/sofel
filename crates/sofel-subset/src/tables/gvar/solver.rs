//! Rebase a tuple's support on a restricted axis, following HarfBuzz's
//! hb-subset-instancer-solver.cc (Copyright © 2023 Behdad Esfahbod).
use alloc::vec::Vec;

pub(super) type Tent = [f32; 3];

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

fn normalize(value: f32, limits: Tent, distances: [f32; 2]) -> f32 {
    let [low, default, high] = limits;
    let [negative, positive] = distances;
    if value == default {
        return 0.0;
    }
    if default < 0.0 {
        return -normalize(-value, reverse(limits), [positive, negative]);
    }
    if value > default {
        return if high == default {
            1.0
        } else {
            (value - default) / (high - default)
        };
    }
    if low >= 0.0 {
        return if default == low {
            -1.0
        } else {
            (value - default) / (default - low)
        };
    }
    let total = negative * (-low) + positive * default;
    if total == 0.0 {
        return 0.0;
    }
    let distance = if value >= 0.0 {
        (default - value) * positive
    } else {
        (-value) * negative + positive * default
    };
    -distance / total
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
                    t.map(|v| normalize(v, limits, distances))
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
                let mapped = normalize(value, limit, [300.0, 500.0]);
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
