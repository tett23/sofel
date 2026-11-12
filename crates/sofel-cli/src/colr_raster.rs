//! Execute the COLRv1 transform, clip and gradient commands on a BGRA surface.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::arithmetic_side_effects
)]
use sofel_color::{PaintCommand, PaintOptions, Painted, Recording};
use sofel_geom::{Brush, CompositeMode, Pen, PointF, Rect, Transform};
use sofel_outline::{HbOutlineCommand, Outlines, Scale, Scratch};
use sofel_raster::{BgraImage, ColorGradient, PaintMask, composite_pixel};
use sofel_read::GlyphId;

fn inverse(t: Transform) -> Option<Transform> {
    let det = t.xx * t.yy - t.xy * t.yx;
    if !det.is_finite() || det.abs() < 1e-10 {
        return None;
    }
    let d = 1. / det;
    Some(Transform {
        xx: t.yy * d,
        xy: -t.xy * d,
        yx: -t.yx * d,
        yy: t.xx * d,
        dx: (t.xy * t.dy - t.yy * t.dx) * d,
        dy: (t.yx * t.dx - t.xx * t.dy) * d,
    })
}
fn mul(a: u8, b: u8) -> u8 {
    ((u32::from(a) * u32::from(b) + 255) >> 8) as u8
}
// HarfBuzz raster rounds an axis-aligned clip outwards. For other transforms
// it tests pixel centers against the four directed edges in surface coordinates.
fn intersect_rect(
    rect: Rect,
    transform: Transform,
    left: i32,
    top: i32,
    width: usize,
    mask: &mut [u8],
) {
    if rect.x_min >= rect.x_max || rect.y_min >= rect.y_max {
        mask.fill(0);
        return;
    }
    let rows = mask.len() / width;
    let bottom = top as f32 - rows as f32;
    let corners = [
        PointF::new(rect.x_min, rect.y_min),
        PointF::new(rect.x_max, rect.y_min),
        PointF::new(rect.x_max, rect.y_max),
        PointF::new(rect.x_min, rect.y_max),
    ]
    .map(|p| {
        let p = transform.apply(p);
        PointF::new(p.x - left as f32, p.y - bottom)
    });
    let [a, b, c, d] = corners;
    let mut twice_area = 0.;
    let edges = [(a, b), (b, c), (c, d), (d, a)].map(|(a, b)| {
        twice_area += a.x * b.y - b.x * a.y;
        let nx = b.y - a.y;
        let ny = a.x - b.x;
        (nx, ny, nx * a.x + ny * a.y)
    });
    if twice_area == 0. || !twice_area.is_finite() {
        mask.fill(0);
        return;
    }
    let xmin = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::INFINITY, f32::min)
        .floor();
    let xmax = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil();
    let ymin = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min)
        .floor();
    let ymax = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil();
    let aligned = transform.xy == 0. && transform.yx == 0.;
    for (i, coverage) in mask.iter_mut().enumerate() {
        let x = (i % width) as f32 + 0.5;
        let y = (rows - 1 - i / width) as f32 + 0.5;
        if x < xmin
            || y < ymin
            || x >= xmax
            || y >= ymax
            || (!aligned
                && edges.iter().any(|&(nx, ny, limit)| {
                    let pos = nx * x + ny * y;
                    if twice_area > 0. {
                        pos > limit
                    } else {
                        pos < limit
                    }
                }))
        {
            *coverage = 0;
        }
    }
}
fn fill_solid(pixels: &mut [u8], mask: &[u8], c: sofel_geom::Color) {
    let color = [mul(c.b, c.a), mul(c.g, c.a), mul(c.r, c.a), c.a];
    for (dst, &coverage) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(mask) {
        let src = color.map(|v| mul(v, coverage));
        *dst = composite_pixel(src, *dst, CompositeMode::SrcOver, f32::sqrt);
    }
}
/// Returns a complete image only when every command can be executed.
pub(crate) fn render(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    ppem: u16,
    options: &PaintOptions<'_>,
    scratch: &mut Scratch,
) -> Option<BgraImage> {
    if !matches!(
        sofel_color::glyph_color_format(outlines.font(), glyph.0),
        Some(
            sofel_color::ColorFormat::ColrV1
                | sofel_color::ColorFormat::Cbdt
                | sofel_color::ColorFormat::Sbix
        )
    ) {
        return None;
    }
    let mut paint_scratch = sofel_color::Scratch::default();
    let mut rec = Recording::new();
    let painted = sofel_color::paint_recording_at(
        outlines.instance(),
        glyph.0,
        options,
        &mut paint_scratch,
        &mut rec,
    );
    let ext = match painted {
        Painted::ColrV1 => sofel_color::glyph_extents(outlines, glyph.0, &mut paint_scratch)?,
        Painted::Cbdt | Painted::Sbix => rec.commands().iter().find_map(|cmd| match cmd {
            PaintCommand::Image(image) => image.extents,
            _ => None,
        })?,
        _ => return None,
    };
    let scale = f32::from(ppem) / f32::from(outlines.font().units_per_em());
    let left = (ext.x_bearing as f32 * scale).floor() as i32;
    let top = (ext.y_bearing as f32 * scale).ceil() as i32;
    let right = (ext.x_bearing.saturating_add(ext.width) as f32 * scale).ceil() as i32;
    let bottom = (ext.y_bearing.saturating_add(ext.height) as f32 * scale).floor() as i32;
    let width = u32::try_from(right.checked_sub(left)?).ok()?;
    let rows = u32::try_from(top.checked_sub(bottom)?).ok()?;
    let n = usize::try_from(width.checked_mul(rows)?).ok()?;
    if n == 0 || width > 4096 || rows > 4096 {
        return None;
    }
    let mut image = BgraImage {
        left,
        top,
        width,
        rows,
        pixels: vec![0; n.checked_mul(4)?],
    };
    let width = usize::try_from(width).ok()?;
    let mut groups = Vec::new();
    let mut clips = vec![vec![255u8; n]];
    let mut matrices = vec![Transform::scale(scale, scale)];
    // HarfBuzz hb-limits.hh: HB_BUDGET_RASTER_PIXELS and HB_BUDGET_RASTER_PAINT_PASSES.
    let mut budget = (1usize << 26).max(4 * n);
    let mut outline_budget = 1usize << 24;
    for command in rec.commands() {
        let t = *matrices.last()?;
        match command {
            PaintCommand::Image(encoded) if encoded.format == sofel_geom::ImageFormat::Png => {
                budget = budget.checked_sub(n)?;
                let decoded =
                    sofel_png::decode_for_paint(encoded.data, budget.min(4096 * 4096)).ok()?;
                budget = budget.checked_sub(decoded.bgra.len() / 4)?;
                let src = BgraImage {
                    width: decoded.width,
                    rows: decoded.height,
                    pixels: decoded.bgra,
                    ..Default::default()
                };
                image.paint_bitmap(&src, encoded.extents?, t, clips.last()?)?;
            }
            PaintCommand::PushTransform(other) => matrices.push(t.multiply(other)),
            PaintCommand::PopTransform => {
                matrices.pop()?;
            }
            PaintCommand::PopClip => {
                clips.pop()?;
            }
            PaintCommand::PushClipRectangle(rect) => {
                budget = budget.checked_sub(n)?;
                let mut mask = clips.last()?.clone();
                intersect_rect(*rect, t, left, top, width, &mut mask);
                clips.push(mask);
            }
            PaintCommand::PushClipGlyph(id) | PaintCommand::FillGlyph(id, _) => {
                budget = budget.checked_sub(n)?;
                outlines.load(GlyphId(*id), Scale::Unscaled, scratch).ok()?;
                let mut pen = PaintMask::new(t, outline_budget.min(1 << 20));
                scratch
                    .hb_decompose(1., 1., |cmd| match cmd {
                        HbOutlineCommand::MoveTo(p) => pen.move_to_float(p),
                        HbOutlineCommand::LineTo(p) => pen.line_to_float(p),
                        HbOutlineCommand::QuadTo(a, b) => pen.quad_to_float(a, b),
                        HbOutlineCommand::CubicTo(a, b, c) => pen.cubic_to_float(a, b, c),
                        HbOutlineCommand::Close => pen.close(),
                    })
                    .ok()?;
                outline_budget = outline_budget.checked_sub(pen.work_used())?;
                let mut mask = clips.last()?.clone();
                pen.intersect(left, top, width, &mut mask, &mut budget)?;
                if let PaintCommand::FillGlyph(_, solid) = command {
                    budget = budget.checked_sub(n)?;
                    fill_solid(&mut image.pixels, &mask, solid.color);
                } else {
                    clips.push(mask);
                }
            }
            PaintCommand::PushGroup(_) => {
                budget = budget.checked_sub(n)?;
                groups.push(core::mem::replace(
                    &mut image.pixels,
                    vec![0; n.checked_mul(4)?],
                ));
            }
            PaintCommand::PopGroup(mode) => {
                budget = budget.checked_sub(n)?;
                let mut backdrop = groups.pop()?;
                for (dst, src) in backdrop
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(image.pixels.as_chunks::<4>().0)
                {
                    *dst = composite_pixel(*src, *dst, *mode, f32::sqrt);
                }
                image.pixels = backdrop;
            }
            PaintCommand::Fill(brush) => {
                budget = budget.checked_sub(n)?;
                let brush = rec.brush(brush);
                if let Brush::Solid(solid) = brush {
                    fill_solid(&mut image.pixels, clips.last()?, solid.color);
                    continue;
                }
                let inv = inverse(t)?;
                let mask = clips.last()?;
                let mut first = width;
                let mut last = 0usize;
                let mut rows = 0usize;
                let mut start_y = None;
                for (y, line) in mask.chunks_exact(width).enumerate() {
                    if let Some(x) = line.iter().position(|a| *a != 0) {
                        first = first.min(x);
                        last = last.max(line.iter().rposition(|a| *a != 0)? + 1);
                        rows = y - *start_y.get_or_insert(y) + 1;
                    }
                }
                if rows == 0 {
                    continue;
                }
                let gradient =
                    ColorGradient::with_atan2(&brush, (last - first) * rows, f32::atan2)?;
                for (j, row) in image.pixels.chunks_exact_mut(width * 4).enumerate() {
                    let alpha = mask.get(j * width..(j + 1) * width)?;

                    let mut p = inv.apply(PointF::new(
                        left as f32 + first as f32 + 0.5,
                        top as f32 - j as f32 - 0.5,
                    ));
                    for (pixel, &coverage) in
                        row.as_chunks_mut::<4>().0.iter_mut().zip(alpha).skip(first)
                    {
                        if coverage != 0 {
                            let src = gradient.sample(p, f32::sqrt).map(|c| mul(c, coverage));
                            let inv_alpha = 255u8.saturating_sub(src[3]);
                            for (d, s) in pixel.iter_mut().zip(src) {
                                *d = s.saturating_add(mul(*d, inv_alpha));
                            }
                        }
                        p.x += inv.xx;
                        p.y += inv.yx;
                    }
                }
            }
            _ => return None,
        }
    }
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sofel_geom::Rect;

    #[test]
    fn i2_rotated_clips_keep_parent_coverage_in_either_winding() {
        let rect = Rect {
            x_min: 0.,
            y_min: 0.,
            x_max: 2.,
            y_max: 2.,
        };
        for (xy, yy, dx, dy) in [(-1., 1., 2., 0.), (1., -1., 0., 2.)] {
            let t = Transform {
                xx: 1.,
                yx: 1.,
                xy,
                yy,
                dx,
                dy,
            };
            let mut mask = [128; 16];
            intersect_rect(rect, t, 0, 4, 4, &mut mask);
            assert_eq!(
                mask,
                [
                    0, 128, 128, 0, 128, 128, 128, 128, 128, 128, 128, 128, 0, 128, 128, 0
                ]
            );
        }
    }
}
