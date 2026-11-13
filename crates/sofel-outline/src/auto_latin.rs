//! Vertical Latin segment links and point interpolation for the automatic hinter.
//! Ported from FreeType 2.15.0's `aflatin.c` and `afhints.c` under the FTL.

// Contour bounds come from the validated outline loader; point coordinates
// are signed font units, and the fixed-point operations follow FreeType.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use alloc::vec::Vec;

use sofel_font::read::{GlyphId, Tag};
use sofel_geom::{div_fix, mul_fix};

use super::{Format, HintTarget, Outlines, Scale, Scratch, TAG_ON};

#[derive(Clone, Copy)]
struct Segment {
    first: usize,
    last: usize,
    fpos: i32,
    min_x: i32,
    max_x: i32,
    dir: i32,
}

// The six segments form a cap stem, a rounded bowl bottom, and a rounded
// bowl top.  Segment links are selected by opposite direction and the
// nearest overlapping X interval.  Unsupported configurations stay out of
// this path until their FreeType edge behavior is implemented.
#[allow(clippy::arithmetic_side_effects)]
pub(super) fn linked_stems(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    scale: Scale,
    target: HintTarget,
    scratch: &Scratch,
) -> Option<Vec<i32>> {
    if !matches!(target, HintTarget::Light | HintTarget::Lcd)
        || !matches!(outlines.format, Format::Glyf)
        || outlines.font.table(Tag::new(b"gvar")).is_some()
        || outlines.font.table(Tag::new(b"GLYF")).is_some()
        || (outlines.font.nominal_glyph(u32::from('b')) != Some(glyph)
            && outlines.font.nominal_glyph(u32::from('d')) != Some(glyph))
        || scratch.ends.len() != 3
    {
        return None;
    }
    let Scale::Scaled { y, .. } = scale else {
        return None;
    };
    let points = &scratch.points;
    let mut segments = Vec::new();
    let mut start = 0;
    for &end in &scratch.ends {
        let mut i = start;
        while i <= end {
            let mut j = i + 1;
            while j <= end && points[j].y == points[i].y {
                j += 1;
            }
            if j - i >= 2 {
                let (mut min_x, mut max_x) = (i32::MAX, i32::MIN);
                for point in &points[i..j] {
                    min_x = min_x.min(point.x);
                    max_x = max_x.max(point.x);
                }
                let dir = (points[j - 1].x - points[i].x).signum();
                if dir != 0 && max_x - min_x >= 20 * i32::from(outlines.font.units_per_em()) / 2048
                {
                    segments.push(Segment {
                        first: i,
                        last: j - 1,
                        fpos: points[i].y,
                        min_x,
                        max_x,
                        dir,
                    });
                }
            }
            i = j;
        }
        start = end + 1;
    }
    if segments.len() != 6 {
        return None;
    }
    segments.sort_by_key(|s| s.fpos);
    if segments.windows(2).any(|pair| pair[0].fpos == pair[1].fpos) {
        return None;
    }
    let mut links = [usize::MAX; 6];
    for (i, segment) in segments.iter().enumerate() {
        links[i] = segments
            .iter()
            .enumerate()
            .filter(|(j, other)| {
                *j != i
                    && other.dir != segment.dir
                    && other.min_x <= segment.max_x
                    && segment.min_x <= other.max_x
            })
            .min_by_key(|(_, other)| (i64::from(other.fpos) - i64::from(segment.fpos)).abs())?
            .0;
    }
    if links != [2, 5, 0, 4, 3, 1] {
        return None;
    }

    let mut sample = Scratch::default();
    let cap = segments[5].fpos;
    let xheight = sample_max(outlines, u32::from('a'), &mut sample)?;
    let bottom = sample_min(outlines, u32::from('c'), &mut sample)?;
    let ascender_ref = sample_max(outlines, u32::from('i'), &mut sample)
        .map_or(cap, |dot_top| (cap + dot_top) / 2);
    if segments[0].fpos != bottom
        || segments[1].fpos != 0
        || segments[4].fpos != xheight
        || bottom >= 0
    {
        return None;
    }
    let scaled = mul_fix(xheight, y.0);
    if scaled <= 0 {
        return None;
    }
    let xfit = (scaled + 40) & !63;
    let new_scale = i32::try_from(
        (i64::from(y.0) * i64::from(xfit) + i64::from(scaled / 2)) / i64::from(scaled),
    )
    .ok()?;
    if mul_fix(i32::from(outlines.font.units_per_em()), new_scale - y.0).abs() >= 128 {
        return None;
    }
    let blue_threshold = mul_fix(i32::from(outlines.font.units_per_em()) / 40, new_scale).min(32);
    if mul_fix((cap - ascender_ref).abs(), new_scale) >= blue_threshold {
        return None;
    }
    let capfit = (mul_fix(ascender_ref, new_scale) + 32) & !63;
    let bottom_opos = mul_fix(bottom, new_scale);
    let standard_width = standard_vertical_width(outlines, &mut sample).unwrap_or(50);
    let edge_threshold = div_fix(mul_fix(standard_width / 5, new_scale).min(16), new_scale);
    let merge_bottom =
        segments[0].dir == segments[1].dir && segments[1].fpos - segments[0].fpos < edge_threshold;
    let bottom_fit = if merge_bottom {
        0
    } else {
        (bottom_opos + 32) & !63
    };
    let baseline_fit = if merge_bottom || -bottom_opos <= 32 {
        bottom_fit
    } else {
        capfit - mul_fix(cap, new_scale)
    };
    let positions = [
        bottom_fit,
        baseline_fit,
        bottom_fit + mul_fix(segments[2].fpos, new_scale)
            - if merge_bottom { 0 } else { bottom_opos },
        xfit + mul_fix(segments[3].fpos, new_scale) - mul_fix(segments[4].fpos, new_scale),
        xfit,
        capfit,
    ];
    let original: Vec<i32> = points
        .iter()
        .map(|point| mul_fix(point.y, new_scale))
        .collect();
    let mut hinted = original.clone();
    let mut touched = alloc::vec![false; points.len()];
    for (segment, &position) in segments.iter().zip(&positions) {
        for i in segment.first..=segment.last {
            hinted[i] = position;
            touched[i] = true;
        }
    }

    // `af_glyph_hints_align_strong_points`: interpolate untouched on-curve
    // corners between neighboring sorted edges in font units.
    let near = 20 * i32::from(outlines.font.units_per_em()) / 2048;
    let near2 = 2 * near - 1;
    let mut contour_start = 0;
    for &end in &scratch.ends {
        for i in contour_start..=end {
            if touched[i] || scratch.tags[i] != TAG_ON {
                continue;
            }
            let prev = if i == contour_start { end } else { i - 1 };
            let next = if i == end { contour_start } else { i + 1 };
            let near = [prev, next].into_iter().any(|j| {
                scratch.tags[j] == TAG_ON
                    && (points[i].x - points[j].x).abs() + (points[i].y - points[j].y).abs() < near2
            });
            if near {
                continue;
            }
            let incoming = direction(points[i].x - points[prev].x, points[i].y - points[prev].y);
            let outgoing = direction(points[next].x - points[i].x, points[next].y - points[i].y);
            if incoming != 0 && incoming == outgoing {
                continue;
            }
            let fy = points[i].y;
            let upper = segments.partition_point(|edge| edge.fpos < fy);
            hinted[i] = if upper == 0 {
                positions[0] - (mul_fix(segments[0].fpos, new_scale) - original[i])
            } else if upper == segments.len() {
                positions[5] + (original[i] - mul_fix(segments[5].fpos, new_scale))
            } else if segments[upper].fpos == fy {
                positions[upper]
            } else {
                let lo = upper - 1;
                let factor = div_fix(
                    positions[upper] - positions[lo],
                    segments[upper].fpos - segments[lo].fpos,
                );
                positions[lo] + mul_fix(fy - segments[lo].fpos, factor)
            };
            touched[i] = true;
        }
        contour_start = end + 1;
    }

    // `af_glyph_hints_align_weak_points`: IUP on each contour.  Original
    // coordinates use the adjusted Y scale, exactly as `af_glyph_hints_reload`.
    let mut contour_start = 0;
    for &end in &scratch.ends {
        let refs: Vec<usize> = (contour_start..=end).filter(|&i| touched[i]).collect();
        if refs.is_empty() {
            return None;
        }
        for (idx, &left) in refs.iter().enumerate() {
            let right = refs[(idx + 1) % refs.len()];
            let mut i = if left == end { contour_start } else { left + 1 };
            while i != right {
                hinted[i] = iup(
                    original[i],
                    original[left],
                    original[right],
                    hinted[left],
                    hinted[right],
                );
                i = if i == end { contour_start } else { i + 1 };
            }
        }
        contour_start = end + 1;
    }
    Some(hinted)
}

fn sample_max(outlines: &Outlines<'_, '_, '_>, cp: u32, sample: &mut Scratch) -> Option<i32> {
    outlines
        .load(outlines.font.nominal_glyph(cp)?, Scale::Unscaled, sample)
        .ok()?;
    sample.points.iter().map(|point| point.y).max()
}

#[allow(clippy::arithmetic_side_effects)]
fn direction(dx: i32, dy: i32) -> i8 {
    let ax = i64::from(dx).abs();
    let ay = i64::from(dy).abs();
    if ay > 14 * ax {
        if dy > 0 { 1 } else { -1 }
    } else if ax > 14 * ay {
        if dx > 0 { 2 } else { -2 }
    } else {
        0
    }
}

fn sample_min(outlines: &Outlines<'_, '_, '_>, cp: u32, sample: &mut Scratch) -> Option<i32> {
    outlines
        .load(outlines.font.nominal_glyph(cp)?, Scale::Unscaled, sample)
        .ok()?;
    sample.points.iter().map(|point| point.y).min()
}

fn standard_vertical_width(outlines: &Outlines<'_, '_, '_>, sample: &mut Scratch) -> Option<i32> {
    let gid = outlines.font.nominal_glyph(u32::from('o'))?;
    outlines.load(gid, Scale::Unscaled, sample).ok()?;
    let [first_end, second_end] = sample.ends.as_slice() else {
        return None;
    };
    let extents = |points: &[sofel_geom::Point]| {
        Some((
            points.iter().map(|p| p.y).min()?,
            points.iter().map(|p| p.y).max()?,
        ))
    };
    let a = extents(&sample.points[..=*first_end])?;
    let b = extents(&sample.points[*first_end + 1..=*second_end])?;
    let (outer, inner) = if a.1 - a.0 >= b.1 - b.0 {
        (a, b)
    } else {
        (b, a)
    };
    let width = (outer.1 - inner.1).min(inner.0 - outer.0);
    (width > 0).then_some(width)
}

#[allow(clippy::arithmetic_side_effects)]
fn iup(value: i32, a_original: i32, b_original: i32, a_hinted: i32, b_hinted: i32) -> i32 {
    let (v1, v2, u1, u2) = if a_original <= b_original {
        (a_original, b_original, a_hinted, b_hinted)
    } else {
        (b_original, a_original, b_hinted, a_hinted)
    };
    if value <= v1 {
        return value + (u1 - v1);
    }
    if value >= v2 {
        return value + (u2 - v2);
    }
    if u1 == u2 || v1 == v2 {
        return u1;
    }
    u1 + mul_fix(value - v1, div_fix(u2 - u1, v2 - v1))
}
