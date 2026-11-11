//! HarfBuzz's gvar phantom-point path for glyf metrics (ADR 0027).

use alloc::vec::Vec;
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32, i32_to_f32};
use sofel_read::{GlyphId, glyf};

use crate::var::gvar::{HbScratch, Point};
use crate::{GlyphExtents, var::Instance};

const MAX_DEPTH: usize = 64;
const USE_MY_METRICS: u16 = glyf::USE_MY_METRICS;
type OutlineWithPhantoms = (Vec<Point<f32>>, [Point<f32>; 4]);

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
    let mut budget = 1 << 24;
    let (outline, phantoms) = outline_at(instance, glyph, scratch, &mut budget)?;
    let mut points = outline.iter();
    let Some(first) = points.next() else {
        return Some(GlyphExtents::default());
    };
    let shift = phantoms[0].x;
    let first_x = first.x - shift;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first_x, first_x, first.y, first.y);
    for point in points {
        let x = point.x - shift;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    if min_x >= max_x || min_y >= max_y {
        return Some(GlyphExtents::default());
    }
    let x_min = hb_roundf32(min_x);
    let x_max = hb_roundf32(max_x);
    let y_min = hb_roundf32(min_y);
    let y_max = hb_roundf32(max_y);
    Some(GlyphExtents {
        x_bearing: hb_clamp_to_i32(x_min),
        y_bearing: hb_clamp_to_i32(y_max),
        width: hb_clamp_to_i32(x_max - x_min),
        height: hb_clamp_to_i32(y_min - y_max),
    })
}

fn outline_at(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut GlyphScratch,
    budget: &mut i64,
) -> Option<OutlineWithPhantoms> {
    if scratch.stack.len() > MAX_DEPTH || scratch.stack.contains(&glyph.0) || *budget < 16 {
        return None;
    }
    *budget = budget.saturating_sub(16);
    scratch.stack.push(glyph.0);
    let result = (|| {
        let font = instance.font();
        let glyf = font.glyf?;
        if glyph.0 >= glyf.num_glyphs() {
            return None;
        }
        let data = glyf.glyph(glyph);
        let mut components = Vec::new();
        let mut points = Vec::new();
        if data.contours > 0 {
            let parsed = glyf::Glyph::new(data.bytes).ok()?;
            let simple = parsed.simple().ok()?;
            let mut ends = simple.end_points().map(usize::from).peekable();
            for (index, point) in simple.points(glyf.extended()).enumerate() {
                points.push(Point {
                    x: i32_to_f32(point.x),
                    y: i32_to_f32(point.y),
                    end: ends.next_if(|end| *end == index).is_some(),
                });
            }
            if points.len() != usize::try_from(simple.num_points()).ok()? {
                return None;
            }
        } else if data.contours < 0 {
            let parsed = glyf::Glyph::new(data.bytes).ok()?;
            components = parsed
                .components(glyf.extended())
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            for component in &components {
                points.push(Point {
                    x: if component.args_are_xy() {
                        i32_to_f32(component.arg1)
                    } else {
                        0.0
                    },
                    y: if component.args_are_xy() {
                        i32_to_f32(component.arg2)
                    } else {
                        0.0
                    },
                    end: true,
                });
            }
        }
        let count = points.len();
        let left = f32::from(data.x_min) - f32::from(font.metrics.bearing(glyph));
        let right = left + f32::from(font.metrics.advance(glyph, font.hb_units_per_em() / 2));
        let top = f32::from(data.y_max) + f32::from(font.vmetrics.bearing(glyph));
        let bottom = top - f32::from(font.vmetrics.advance(glyph, font.hb_units_per_em()));
        points.extend([
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
            .apply_gvar_hb(glyph.0, &mut points, &mut scratch.gvar, false, budget)
            .ok()?;
        let mut phantoms: [Point<f32>; 4] =
            points.get(count..count.checked_add(4)?)?.try_into().ok()?;
        points.truncate(count);
        if data.contours < 0 {
            let component_positions = points;
            let mut outline: Vec<Point<f32>> = Vec::new();
            for (component, position) in components.into_iter().zip(component_positions) {
                if scratch.stack.contains(&component.glyph.0) {
                    continue;
                }
                let (mut child, child_phantoms) =
                    outline_at(instance, component.glyph, scratch, budget)?;
                if component.flags & USE_MY_METRICS != 0 {
                    phantoms = child_phantoms;
                }
                let xx = f32::from(component.xx) / 16384.0;
                let yx = f32::from(component.yx) / 16384.0;
                let xy = f32::from(component.xy) / 16384.0;
                let yy = f32::from(component.yy) / 16384.0;
                let scaled_offset = component.flags
                    & (glyf::SCALED_COMPONENT_OFFSET | glyf::UNSCALED_COMPONENT_OFFSET)
                    == glyf::SCALED_COMPONENT_OFFSET;
                for point in &mut child {
                    if scaled_offset {
                        point.x += position.x;
                        point.y += position.y;
                    }
                    let x = point.x * xx + point.y * xy;
                    let y = point.x * yx + point.y * yy;
                    point.x = x;
                    point.y = y;
                    if !scaled_offset {
                        point.x += position.x;
                        point.y += position.y;
                    }
                }
                if !component.args_are_xy() {
                    let parent = usize::try_from(component.arg1)
                        .ok()
                        .and_then(|i| outline.get(i))
                        .copied();
                    let anchor = usize::try_from(component.arg2)
                        .ok()
                        .and_then(|i| child.get(i))
                        .copied();
                    if let (Some(parent), Some(anchor)) = (parent, anchor) {
                        let dx = parent.x - anchor.x;
                        let dy = parent.y - anchor.y;
                        for point in &mut child {
                            point.x += dx;
                            point.y += dy;
                        }
                    }
                }
                outline.extend(child);
            }
            Some((outline, phantoms))
        } else {
            Some((points, phantoms))
        }
    })();
    scratch.stack.pop();
    result
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
        // Earlier USE_MY_METRICS components are overwritten by the last one.
        let selected = components.into_iter().rev().find(|(flags, component)| {
            flags & USE_MY_METRICS != 0 && !scratch.stack.contains(component)
        });
        let child = selected
            .map(|(_, component)| phantoms_at(instance, GlyphId(component), scratch, budget));
        scratch.stack.pop();
        if let Some(child) = child {
            phantoms = child?;
        }
    }
    Some(phantoms)
}
