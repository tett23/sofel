//! Packed point numbers for the references retained by IUP optimization.
use alloc::vec::Vec;

pub(super) fn pack_points(selected: &[bool]) -> Option<Vec<u8>> {
    let count = selected.iter().filter(|&&keep| keep).count();
    if count == 0 {
        return Some(Vec::new());
    }
    if count == selected.len() {
        return Some(alloc::vec![0]);
    }
    let count = u16::try_from(count).ok().filter(|n| *n <= 0x7fff)?;
    let mut out = Vec::new();
    if count < 128 {
        out.push(u8::try_from(count).ok()?);
    } else {
        out.extend((count | 0x8000).to_be_bytes());
    }
    let mut previous = 0;
    let mut points = selected
        .iter()
        .enumerate()
        .filter_map(|(i, &keep)| keep.then_some(i))
        .peekable();
    while let Some(&first) = points.peek() {
        let wide = first.checked_sub(previous)? > 255;
        let header = out.len();
        out.push(0);
        let mut length = 0u8;
        while let Some(&index) = points.peek() {
            let delta = u16::try_from(index.checked_sub(previous)?).ok()?;
            if !wide && delta > 255 {
                break;
            }
            if wide {
                out.extend(delta.to_be_bytes());
            } else {
                out.push(u8::try_from(delta).ok()?);
            }
            points.next();
            previous = index;
            length = length.checked_add(1)?;
            if length == 128 {
                break;
            }
        }
        *out.get_mut(header)? = length.checked_sub(1)? | if wide { 0x80 } else { 0 };
    }
    Some(out)
}

const TOLERANCE: f64 = 0.5 + 1e-10;

#[derive(Clone, Copy)]
pub(super) struct Sample {
    pub position: [f64; 2],
    pub delta: [i16; 2],
}

#[allow(
    clippy::indexing_slicing,
    reason = "Axis indices come from the two coordinates of a Sample."
)]
fn ordered(left: Sample, right: Sample, axis: usize) -> ([f64; 2], [f64; 2]) {
    let (left, right) = if left.position[axis] <= right.position[axis] {
        (left, right)
    } else {
        (right, left)
    };
    (
        [left.position[axis], right.position[axis]],
        [f64::from(left.delta[axis]), f64::from(right.delta[axis])],
    )
}

#[allow(
    clippy::indexing_slicing,
    reason = "Axis indices come from the two coordinates of a Sample."
)]
fn forced(left: Sample, point: Sample, right: Sample) -> bool {
    (0..2).any(|axis| {
        let ([lo, hi], [a, b]) = ordered(left, right, axis);
        let x = point.position[axis];
        let d = f64::from(point.delta[axis]);
        if lo == hi {
            (a - b).abs() > TOLERANCE && d.abs() > TOLERANCE
        } else if (lo..=hi).contains(&x) {
            d < a.min(b) - TOLERANCE || d > a.max(b) + TOLERANCE
        } else if a == b || d.abs() <= TOLERANCE {
            false
        } else if x < lo {
            (d - a).abs() > TOLERANCE && ((d - TOLERANCE < a) != (a < b))
        } else {
            (d - b).abs() > TOLERANCE && ((b < d + TOLERANCE) != (a < b))
        }
    })
}

#[allow(
    clippy::indexing_slicing,
    reason = "Axis indices come from the two coordinates of a Sample."
)]
fn inferable(left: Sample, middle: &[Sample], right: Sample) -> bool {
    let bounds = [ordered(left, right, 0), ordered(left, right, 1)];
    middle.iter().all(|p| {
        let mut squared = 0.0;
        for (axis, ([lo, hi], [a, b])) in bounds.iter().copied().enumerate() {
            let x = p.position[axis];
            let delta = if lo == hi {
                if a == b { a } else { 0.0 }
            } else if x <= lo {
                a
            } else if x >= hi {
                b
            } else {
                a + (x - lo) * ((b - a) / (hi - lo))
            };
            let error = delta - f64::from(p.delta[axis]);
            squared += error * error;
        }
        squared <= TOLERANCE * TOLERANCE
    })
}

// Node zero is the virtual reference immediately before the first point.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "The two equal-length input slices contain at most 1024 samples. Nodes run from 1 through their length and every predecessor is smaller."
)]
fn chains(points: &[Sample], mandatory: &[bool], lookback: usize) -> (Vec<usize>, Vec<usize>) {
    let mut costs = alloc::vec![0; points.len()+1];
    let mut previous = alloc::vec![0; points.len()+1];
    for i in 1..=points.len() {
        costs[i] = costs[i - 1] + 1;
        previous[i] = i - 1;
        if i < 2 || mandatory[i - 2] {
            continue;
        }
        for j in (i.saturating_sub(lookback.min(8).saturating_sub(1))..=i - 2).rev() {
            let left = if j == 0 { points.len() - 1 } else { j - 1 };
            if costs[j] + 1 < costs[i] && inferable(points[left], &points[j..i - 1], points[i - 1])
            {
                costs[i] = costs[j] + 1;
                previous[i] = j;
            }
            if j > 1 && mandatory[j - 1] {
                break;
            }
        }
    }
    (costs, previous)
}

#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "After the early returns the contour has 1 to 512 points. The doubled contour has 2*n points and its node arrays have 2*n+1 entries."
)]
pub(super) fn select(points: &[Sample]) -> Vec<bool> {
    let n = points.len();
    let mut selected = alloc::vec![n > 512; n];
    if n > 512 || points.iter().all(|p| p.delta == [0, 0]) {
        return selected;
    }
    if points.iter().all(|p| p.delta == points[0].delta) {
        selected[0] = true;
        return selected;
    }
    let mut mandatory: Vec<_> = (0..n)
        .map(|i| forced(points[(i + n - 1) % n], points[i], points[(i + 1) % n]))
        .collect();
    if let Some(last) = mandatory.iter().rposition(|p| *p) {
        let rotation = n - 1 - last;
        let mut rotated = points.to_vec();
        rotated.rotate_right(rotation);
        mandatory.rotate_right(rotation);
        let (_, previous) = chains(&rotated, &mandatory, n);
        let mut at = n;
        while at > 0 {
            selected[at - 1] = true;
            at = previous[at];
        }
        selected.rotate_left(rotation);
    } else {
        let doubled = [points, points].concat();
        mandatory.resize(doubled.len(), false);
        let (costs, previous) = chains(&doubled, &mandatory, n);
        let mut best = n + 1;
        for end in n..=2 * n {
            let start = end - n;
            let cost = costs[end].saturating_sub(costs[start]);
            if cost > best {
                continue;
            }
            let mut at = end;
            while at > start {
                at = previous[at];
            }
            if at == start {
                best = cost;
                selected.fill(false);
                at = end;
                while at > start {
                    selected[(at - 1) % n] = true;
                    at = previous[at];
                }
            }
        }
    }
    selected
}
