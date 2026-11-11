//! HarfBuzz's gvar phantom-point path for glyf metrics (ADR 0027).

use alloc::vec::Vec;
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32};
use sofel_read::{GlyphId, glyf};

use crate::var::gvar::{HbScratch, Point};
use crate::{GlyphExtents, var::Instance};

const MAX_DEPTH: usize = 64;
const USE_MY_METRICS: u16 = glyf::USE_MY_METRICS;

/// Reused point and gvar buffers for glyph metric queries.
#[derive(Default)]
pub struct GlyphScratch {
    points: Vec<Point<f32>>,
    gvar: HbScratch,
    stack: Vec<u32>,
}

pub(crate) fn phantoms(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut GlyphScratch,
) -> Option<[Point<f32>; 4]> {
    let mut budget = 1 << 24;
    phantoms_at(instance, glyph, scratch, &mut budget)
}

pub(crate) fn extents(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut GlyphScratch,
) -> Option<GlyphExtents> {
    let font = instance.font();
    let glyf = font.glyf?;
    if glyph.0 >= glyf.num_glyphs() {
        return None;
    }
    let data = glyf.glyph(glyph);
    if data.contours == 0 {
        return Some(GlyphExtents::default());
    }
    if data.contours < 0 {
        return None;
    }
    let parsed = glyf::Glyph::new(data.bytes).ok()?;
    let simple = parsed.simple().ok()?;
    scratch.points.clear();
    let mut ends = simple.end_points().map(usize::from).peekable();
    for (index, point) in simple.points(glyf.extended()).enumerate() {
        scratch.points.push(Point {
            x: point.x as f32,
            y: point.y as f32,
            end: ends.next_if(|end| *end == index).is_some(),
        });
    }
    let count = scratch.points.len();
    if count != usize::try_from(simple.num_points()).ok()? {
        return None;
    }
    let left = f32::from(data.x_min) - f32::from(font.metrics.bearing(glyph));
    let right = left + f32::from(font.metrics.advance(glyph, font.hb_units_per_em() / 2));
    let top = f32::from(data.y_max) + f32::from(font.vmetrics.bearing(glyph));
    let bottom = top - f32::from(font.vmetrics.advance(glyph, font.hb_units_per_em()));
    scratch.points.extend([
        Point {
            x: left,
            y: 0.0,
            end: false,
        },
        Point {
            x: right,
            y: 0.0,
            end: false,
        },
        Point {
            x: 0.0,
            y: top,
            end: false,
        },
        Point {
            x: 0.0,
            y: bottom,
            end: false,
        },
    ]);
    let mut budget = 1 << 24;
    instance
        .apply_gvar_hb(
            glyph.0,
            &mut scratch.points,
            &mut scratch.gvar,
            false,
            &mut budget,
        )
        .ok()?;
    let shift = scratch.points.get(count)?.x;
    let mut points = scratch.points.get(..count)?.iter();
    let first = points.next()?;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    if min_x >= max_x || min_y >= max_y {
        return Some(GlyphExtents::default());
    }
    let x_min = hb_roundf32(min_x - shift);
    let x_max = hb_roundf32(max_x - shift);
    let y_min = hb_roundf32(min_y);
    let y_max = hb_roundf32(max_y);
    Some(GlyphExtents {
        x_bearing: hb_clamp_to_i32(x_min),
        y_bearing: hb_clamp_to_i32(y_max),
        width: hb_clamp_to_i32(x_max - x_min),
        height: hb_clamp_to_i32(y_min - y_max),
    })
}

fn phantoms_at(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut GlyphScratch,
    budget: &mut i64,
) -> Option<[Point<f32>; 4]> {
    if scratch.stack.len() > MAX_DEPTH || scratch.stack.contains(&glyph.0) || *budget < 16 {
        return None;
    }
    *budget = budget.saturating_sub(16);
    let font = instance.font();
    let glyf = font.glyf?;
    if glyph.0 >= glyf.num_glyphs() {
        return None;
    }
    let data = glyf.glyph(glyph);
    let mut components = Vec::new();
    let count = if data.contours > 0 {
        let parsed = glyf::Glyph::new(data.bytes).ok()?;
        usize::try_from(parsed.simple().ok()?.num_points()).ok()?
    } else if data.contours < 0 {
        components.extend(glyf.components(&data));
        components.len()
    } else {
        0
    };
    scratch.points.clear();
    scratch.points.resize(count, Point::default());
    if data.contours < 0 {
        for point in &mut scratch.points {
            point.end = true;
        }
    }
    let left = f32::from(data.x_min) - f32::from(font.metrics.bearing(glyph));
    let right = left + f32::from(font.metrics.advance(glyph, font.hb_units_per_em() / 2));
    let top = f32::from(data.y_max) + f32::from(font.vmetrics.bearing(glyph));
    let bottom = top - f32::from(font.vmetrics.advance(glyph, font.hb_units_per_em()));
    scratch.points.extend([
        Point {
            x: left,
            y: 0.0,
            end: false,
        },
        Point {
            x: right,
            y: 0.0,
            end: false,
        },
        Point {
            x: 0.0,
            y: top,
            end: false,
        },
        Point {
            x: 0.0,
            y: bottom,
            end: false,
        },
    ]);
    instance
        .apply_gvar_hb(
            glyph.0,
            &mut scratch.points,
            &mut scratch.gvar,
            data.contours > 0,
            budget,
        )
        .ok()?;
    let mut phantoms = scratch
        .points
        .get(count..count.checked_add(4)?)?
        .try_into()
        .ok()?;
    if data.contours < 0 {
        scratch.stack.push(glyph.0);
        for (flags, component) in components {
            if flags & USE_MY_METRICS != 0
                && let Some(child) = phantoms_at(instance, GlyphId(component), scratch, budget)
            {
                phantoms = child;
            }
        }
        scratch.stack.pop();
    }
    Some(phantoms)
}
