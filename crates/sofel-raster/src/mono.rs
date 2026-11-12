//! Packed monochrome rasterization (sofel ADR 0064).

use super::PixelBox;

// ft_glyphslot_preset_bitmap rounds ties toward the outside. If an
// interval collapses, keep the pixel containing the larger part of it.
#[allow(
    clippy::arithmetic_side_effects,
    reason = "shifting by six leaves room for rounding and one extra pixel"
)]
fn mono_axis(start: i32, end: i32) -> (i32, i32) {
    let mut lower = (start >> 6) + (((start & 63) + 31) >> 6);
    let mut upper = (end >> 6) + (((end & 63) + 32) >> 6);
    if lower == upper {
        let remainder = (((start & 63) + 31) & 63) - 31 + (((end & 63) + 32) & 63) - 32;
        if remainder < 0 {
            lower -= 1;
        } else {
            upper += 1;
        }
    }
    (lower, upper)
}

/// Grid-fit the 26.6 control box for FreeType's monochrome rasterizer.
pub fn mono_pixel_box(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> PixelBox {
    let (left, right) = mono_axis(x_min, x_max);
    let (bottom, top) = mono_axis(y_min, y_max);
    PixelBox {
        left,
        top,
        width: u32::try_from(right.saturating_sub(left)).unwrap_or(0),
        rows: u32::try_from(top.saturating_sub(bottom)).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::mono_axis;

    #[test]
    fn adr0064_i2_mono_bounds_include_centres_and_preserve_collapsed_axes() {
        for (start, end, expected) in [
            (0, 0, (0, 1)),
            (-1, -1, (-1, 0)),
            (1, 1, (0, 1)),
            (31, 31, (0, 1)),
            (32, 32, (0, 1)),
            (33, 33, (0, 1)),
            (-32, 32, (-1, 1)),
            (32, 96, (0, 2)),
            (33, 95, (1, 2)),
            (0, 448, (0, 7)),
            (0, 512, (0, 8)),
            (0, 576, (0, 9)),
        ] {
            assert_eq!(mono_axis(start, end), expected, "{start}..{end}");
        }
    }
}

// Line_Up and Line_Down sample in traversal order. Inputs have already
// been translated by half a pixel, as in FreeType's SCALED macro.
#[allow(
    clippy::arithmetic_side_effects,
    reason = "coordinates come from scaled i32 outline points and precision is 64 or 4096"
)]
fn line_profile(
    (x1, y1): (i64, i64),
    (x2, y2): (i64, i64),
    (bottom, top): (i64, i64),
    precision: i64,
    mut visit: impl FnMut(i64, i64),
) {
    if y1 == y2 {
        return;
    }
    let ascending = y1 < y2;
    let (start, end, low, high) = if ascending {
        (y1, y2, bottom, top)
    } else {
        (-y1, -y2, -top, -bottom)
    };
    let first = (start.div_euclid(precision) + 1).max(low);
    let last = end.div_euclid(precision).min(high);
    for row in first..=last {
        let offset =
            i128::from(row * precision - start) * i128::from(x2 - x1) / i128::from(end - start);
        // The intersection is between the two original x coordinates.
        let x = (i128::from(x1) + offset) as i64;
        visit(if ascending { row } else { -row }, x);
    }
}

#[cfg(test)]
mod profile_tests {
    use super::line_profile;

    #[test]
    fn adr0064_i2_lines_exclude_the_start_include_the_end_and_clip_rows() {
        let samples = |from, to, clip| {
            let mut values = alloc::vec::Vec::new();
            line_profile(from, to, clip, 64, |row, x| values.push((row, x)));
            values
        };
        assert_eq!(
            samples((0, 0), (100, 192), (0, 3)),
            [(1, 33), (2, 66), (3, 100)]
        );
        assert_eq!(
            samples((100, 192), (0, 0), (0, 3)),
            [(2, 67), (1, 34), (0, 0)]
        );
        assert_eq!(samples((10, -32), (-90, 160), (0, 1)), [(0, -6), (1, -40)]);
        assert!(samples((0, 64), (128, 64), (0, 3)).is_empty());
    }
}

#[cfg(test)]
mod curve_tests {
    use super::{Arc, curve_profile};

    #[test]
    fn adr0064_i2_curves_match_reference_intersections_in_both_directions() {
        let mut samples = alloc::vec::Vec::new();
        curve_profile(
            Arc::Quad([(-23, -65), (115, 13), (19, 130)]),
            (-3, 3),
            64,
            |row, x| samples.push((row, x)),
        );
        assert_eq!(samples, [(-1, -21), (0, 48), (1, 53), (2, 20)]);
        samples.clear();
        curve_profile(
            Arc::Cubic([(-35, 155), (105, 123), (-73, 33), (11, -100)]),
            (-3, 3),
            64,
            |row, x| samples.push((row, x)),
        );
        assert_eq!(samples, [(2, 14), (1, 8), (0, -13), (-1, -6)]);
    }
}

type MonoPoint = (i64, i64);

#[derive(Clone, Copy)]
enum Arc {
    Quad([MonoPoint; 3]),
    Cubic([MonoPoint; 4]),
}

impl Arc {
    fn ends(self) -> (MonoPoint, MonoPoint) {
        match self {
            Self::Quad([a, _, b]) | Self::Cubic([a, _, _, b]) => (a, b),
        }
    }

    // Conic_To and Cubic_To allow the control points to extend to the
    // neighboring grid line before subdividing the arc at its midpoint.
    fn directed(self, precision: i64, mut visit: impl FnMut(Self)) {
        let mut pending = alloc::vec![self];
        while let Some(arc) = pending.pop() {
            let (start, end) = arc.ends();
            let low = start.1.min(end.1) & -precision;
            let high = (start.1.max(end.1) + precision - 1) & -precision;
            let outside = |point: MonoPoint| !(low..=high).contains(&point.1);
            let turns = match arc {
                Self::Quad([_, control, _]) => outside(control),
                Self::Cubic([_, first, second, _]) => outside(first) || outside(second),
            };
            if turns {
                let (first, second) = arc.split();
                pending.push(second);
                pending.push(first);
            } else if start.1 != end.1 {
                visit(arc);
            }
        }
    }

    fn flip_y(self) -> Self {
        let flip = |(x, y): MonoPoint| (x, -y);
        match self {
            Self::Quad(p) => Self::Quad(p.map(flip)),
            Self::Cubic(p) => Self::Cubic(p.map(flip)),
        }
    }

    // Accumulate the Bernstein coefficients before shifting. Rounding
    // intermediate midpoints separately changes the intersection by one.
    fn split(self) -> (Self, Self) {
        let add = |a: MonoPoint, b: MonoPoint| (a.0 + b.0, a.1 + b.1);
        let shift = |p: MonoPoint, n: u32| (p.0 >> n, p.1 >> n);
        match self {
            Self::Quad([a, b, c]) => {
                let (ab, bc) = (add(a, b), add(b, c));
                let mid = shift(add(ab, bc), 2);
                (
                    Self::Quad([a, shift(ab, 1), mid]),
                    Self::Quad([mid, shift(bc, 1), c]),
                )
            }
            Self::Cubic([a, b, c, d]) => {
                let (ab, bc, cd) = (add(a, b), add(b, c), add(c, d));
                let (abc, bcd) = (add(ab, bc), add(bc, cd));
                let mid = shift(add(abc, bcd), 3);
                (
                    Self::Cubic([a, shift(ab, 1), shift(abc, 2), mid]),
                    Self::Cubic([mid, shift(bcd, 2), shift(cd, 1), d]),
                )
            }
        }
    }
}

// FreeType's Bezier_Up/Down, for one directed arc after extrema splitting.
fn curve_profile(
    arc: Arc,
    (bottom, top): (i64, i64),
    precision: i64,
    mut visit: impl FnMut(i64, i64),
) {
    let (start, end) = arc.ends();
    let down = start.1 > end.1;
    let (arc, low, high) = if down {
        (arc.flip_y(), -top, -bottom)
    } else {
        (arc, bottom, top)
    };
    let (start, end) = arc.ends();
    let mut row = (start.1.div_euclid(precision) + 1).max(low);
    let last = end.1.div_euclid(precision).min(high);
    let step = if precision == 64 { 32 } else { 256 };
    let mut pending = alloc::vec![arc];
    while row <= last {
        let Some(segment) = pending.pop() else { break };
        let ((x1, y1), (x2, y2)) = segment.ends();
        let y = row * precision;
        if y2 < y {
            continue;
        }
        let x = if y2 == y {
            x2
        } else {
            let (dx, dy) = (x2 - x1, y2 - y1);
            if dy > step || dx.abs() > step {
                let (first, second) = segment.split();
                pending.push(second);
                pending.push(first);
                continue;
            }
            x2 - (y2 - y) * dx / dy
        };
        visit(if down { -row } else { row }, x);
        row += 1;
    }
}

#[cfg(test)]
mod turning_tests {
    use super::{Arc, curve_profile};

    #[test]
    fn adr0064_i2_curve_returning_to_its_start_height_visits_both_slopes() {
        let arc = Arc::Quad([(0, 0), (128, 256), (256, 0)]);
        let mut crossings = alloc::vec::Vec::new();
        arc.directed(64, |part| {
            curve_profile(part, (0, 3), 64, |row, x| crossings.push((row, x)));
        });
        assert_eq!(crossings, [(1, 38), (2, 128), (1, 219), (0, 256)]);
    }
}

#[cfg(test)]
mod span_tests {
    use super::paint_span;

    #[test]
    fn adr0064_i2_mono_spans_clip_pack_and_preserve_existing_ink() {
        for (width, expected) in [(7usize, &[0xfe][..]), (8, &[0xff]), (9, &[0xff, 0x80])] {
            let mut row = alloc::vec![0; width.div_ceil(8)];
            paint_span(&mut row, width, -64, 640, 64);
            assert_eq!(row, expected);
        }
        let mut row = [0x80, 1, 0];
        paint_span(&mut row, 24, 3 * 4096 + 1, 18 * 4096 - 1, 4096);
        assert_eq!(row, [0x8f, 0xff, 0xc0]);
        paint_span(&mut row, 24, 1, 63, 64);
        assert_eq!(row, [0x8f, 0xff, 0xc0]);
    }
}

// Vertical_Sweep_Span. The interval includes both pixel centres and the
// last partial byte is clipped to the logical width.
fn paint_span(row: &mut [u8], width: usize, left: i64, right: i64, precision: i64) {
    let first = ((left + precision - 1).div_euclid(precision)).max(0);
    let last = right.div_euclid(precision).min(width as i64 - 1);
    if first > last {
        return;
    }
    let first = first as usize;
    let last = last as usize;
    let bytes = &mut row[first / 8..=last / 8];
    let start_mask = u8::MAX >> (first % 8);
    let end_mask = u8::MAX << (7 - last % 8);
    if let [byte] = bytes {
        *byte |= start_mask & end_mask;
    } else if let [start, middle @ .., end] = bytes {
        *start |= start_mask;
        middle.fill(u8::MAX);
        *end |= end_mask;
    }
}
