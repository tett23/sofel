//! Execute the COLRv1 transform, clip and gradient commands on a BGRA surface.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::arithmetic_side_effects
)]
use sofel_color::{PaintCommand, PaintOptions, Painted, RawPaintGraph, Recording};
use sofel_geom::{Pen, PointF, Transform};
use sofel_outline::{HbOutlineCommand, Outlines, Scale, Scratch};
use sofel_raster::{BgraImage, ColorGradient, PaintMask};
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
/// Returns a complete image only when every command can be executed.
pub(crate) fn render(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    ppem: u16,
    options: &PaintOptions<'_>,
    scratch: &mut Scratch,
) -> Option<BgraImage> {
    RawPaintGraph::new(outlines.font())?.base_paint(glyph.0)?;
    let mut paint_scratch = sofel_color::Scratch::default();
    let ext = sofel_color::glyph_extents(outlines, glyph.0, &mut paint_scratch)?;
    let mut rec = Recording::new();
    if sofel_color::paint_recording_at(
        outlines.instance(),
        glyph.0,
        options,
        &mut paint_scratch,
        &mut rec,
    ) != Painted::ColrV1
    {
        return None;
    }
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
    let mut clips = vec![vec![255u8; n]];
    let mut matrices = vec![Transform::scale(scale, scale)];
    // HarfBuzz hb-limits.hh: HB_BUDGET_RASTER_PIXELS and HB_BUDGET_RASTER_PAINT_PASSES.
    let mut budget = (1usize << 26).max(4 * n);
    let mut outline_budget = 1usize << 24;
    for command in rec.commands() {
        let t = *matrices.last()?;
        match command {
            PaintCommand::PushTransform(other) => matrices.push(t.multiply(other)),
            PaintCommand::PopTransform => {
                matrices.pop()?;
            }
            PaintCommand::PopClip => {
                clips.pop()?;
            }
            PaintCommand::PushClipRectangle(rect) => {
                if t.xy != 0. || t.yx != 0. {
                    return None;
                }
                budget = budget.checked_sub(n)?;
                let a = t.apply(PointF::new(rect.x_min, rect.y_min));
                let b = t.apply(PointF::new(rect.x_max, rect.y_max));
                let mut mask = clips.last()?.clone();
                for (i, p) in mask.iter_mut().enumerate() {
                    let x = left as f32 + (i % width) as f32;
                    let y = top as f32 - 1. - (i / width) as f32;
                    if x < a.x.min(b.x).floor()
                        || x >= a.x.max(b.x).ceil()
                        || y < a.y.min(b.y).floor()
                        || y >= a.y.max(b.y).ceil()
                    {
                        *p = 0;
                    }
                }
                clips.push(mask);
            }
            PaintCommand::PushClipGlyph(id) => {
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
                clips.push(mask);
            }
            PaintCommand::Fill(brush) => {
                budget = budget.checked_sub(n)?;
                let brush = rec.brush(brush);
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
