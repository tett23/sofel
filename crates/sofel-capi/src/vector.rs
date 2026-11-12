//! Vector page output from positioned glyphs (ADR 0068).
use crate::{
    ERROR_BUFFER_TOO_SMALL, ERROR_INVALID_ARGUMENT, SofelFont, SofelLocation, color::CColor,
    out_array, outline_error, outlines_at, r, some, status, variation::input_array, w,
};
use sofel_font::read::GlyphId;
use sofel_geom::{Color, CompositeMode, GlyphExtents, Image, ImageFormat, Point, Rect, Transform};
use sofel_outline::{Outlines, Scale, Scratch};
use sofel_vector::{PdfPage, SvgPage};

/// C representation of a positioned glyph.
#[repr(C)]
pub struct CVectorGlyph {
    glyph: u32,
    transform: [f32; 6],
    color: CColor,
}

enum Page {
    Svg(SvgPage),
    Pdf(PdfPage),
}

fn decode_image(image: &Image<'_>) -> Option<(u32, u32, Vec<u8>)> {
    let limit = (image.width as usize)
        .saturating_mul(image.height as usize)
        .min((16 << 20) / 4);
    let decoded = sofel_png::decode_rgba(image.data, limit).ok()?;
    Some((decoded.width, decoded.height, decoded.rgba))
}

/// Writes a complete page in the requested format.
///
/// # Safety
/// Font and optional location are live handles. Non-null background and
/// glyph pointers are readable for one color and `count` glyphs respectively.
/// Output is writable for `capacity` bytes and len for one usize. Mutable
/// outputs do not alias any other argument.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_vector_page(
    font: *const SofelFont,
    location: *const SofelLocation,
    format: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    background: *const CColor,
    glyphs: *const CVectorGlyph,
    count: usize,
    output: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees each pointer and range described above.
    let (font, location, background, glyphs, output, len) = unsafe {
        (
            r(font),
            r(location),
            r(background),
            input_array(glyphs, count),
            out_array(output, capacity),
            w(len),
        )
    };
    status((|| {
        let font = some(font)?;
        let outlines = outlines_at(font, location);
        let loc = location
            .map(|l| &l.location)
            .unwrap_or(sofel_font::var::Location::default_ref());
        let instance = font.font.instance(loc);
        let (glyphs, output, len) = (some(glyphs)?, some(output)?, some(len)?);
        let background = background.copied().map(Color::from);
        let mut page = match format {
            0 => Page::Svg(SvgPage::new(x, y, width, height, background)),
            1 => Page::Pdf(PdfPage::new(x, y, width, height, background)),
            _ => return Err(ERROR_INVALID_ARGUMENT),
        };
        let mut scratch = Scratch::default();
        let mut color_scratch = sofel_color::Scratch::default();
        let mut recording = sofel_color::Recording::new();
        for g in glyphs {
            if !g.transform.iter().all(|v| v.is_finite()) {
                return Err(ERROR_INVALID_ARGUMENT);
            }
            let [xx, yx, xy, yy, dx, dy] = g.transform;
            // Decompose at double resolution to preserve implied half-unit points.
            let t = Transform {
                xx: xx * 0.5,
                yx: yx * 0.5,
                xy: xy * 0.5,
                yy: yy * 0.5,
                dx,
                dy,
            };
            let c = g.color.into();
            let options = sofel_color::PaintOptions {
                palette: sofel_color::Palette::Font(0),
                foreground: c,
                ppem: 0,
            };
            if matches!(
                sofel_color::glyph_color_format(&font.font, g.glyph),
                Some(sofel_color::ColorFormat::Cbdt | sofel_color::ColorFormat::Sbix)
            ) {
                recording.clear();
                sofel_color::paint_recording_at(
                    instance,
                    g.glyph,
                    &options,
                    &mut color_scratch,
                    &mut recording,
                );
                if let [sofel_color::PaintCommand::Image(image)] = recording.commands() {
                    let placement = Transform {
                        xx,
                        yx,
                        xy,
                        yy,
                        dx,
                        dy,
                    };
                    let painted = match &mut page {
                        Page::Svg(svg) => svg.draw_image(image, placement, None),
                        Page::Pdf(pdf) => pdf.paint_with_images(
                            placement,
                            |_, _| false,
                            decode_image,
                            |p| recording.replay(p),
                        ),
                    };
                    if painted {
                        continue;
                    }
                }
            }
            if sofel_color::glyph_color_format(&font.font, g.glyph)
                == Some(sofel_color::ColorFormat::ColrV1)
            {
                recording.clear();
                sofel_color::paint_recording_at(
                    instance,
                    g.glyph,
                    &options,
                    &mut color_scratch,
                    &mut recording,
                );
                let scale = sofel_font::HbScale::unscaled(&font.font);
                let placement = Transform {
                    xx,
                    yx,
                    xy,
                    yy,
                    dx,
                    dy,
                };
                let mut draw = |gid, emit: &mut dyn FnMut(sofel_geom::FloatCommand)| {
                    let glyph = GlyphId(gid);
                    if let Some(commands) = outlines
                        .varc_glyph_path(glyph, scale)
                        .or_else(|| outlines.hb_extents().cff_glyph_path(glyph, &scale))
                    {
                        for cmd in commands {
                            emit(cmd);
                        }
                        true
                    } else {
                        outlines.load(glyph, Scale::Unscaled, &mut scratch).is_ok()
                            && scratch.hb_decompose(1., 1., emit).is_ok()
                    }
                };
                let rendered = match &mut page {
                    Page::Svg(svg) => svg.paint(
                        placement,
                        |gid, pen| draw(gid, &mut |cmd| pen.command(cmd)),
                        |p| recording.replay(p),
                    ),
                    Page::Pdf(pdf) => pdf.paint_with_images(
                        placement,
                        |gid, pen| draw(gid, &mut |cmd| pen.command(cmd)),
                        decode_image,
                        |p| recording.replay(p),
                    ),
                };
                if rendered {
                    continue;
                }
            }
            let mut fill = |gid, c| {
                if let Some(commands) = outlines
                    .varc_glyph_path(GlyphId(gid), sofel_font::HbScale::unscaled(&font.font))
                {
                    let placement = Transform {
                        xx,
                        yx,
                        xy,
                        yy,
                        dx,
                        dy,
                    };
                    return match &mut page {
                        Page::Svg(page) => page.fill_path(placement, c, |pen| {
                            for cmd in commands {
                                pen.command(cmd);
                            }
                            Ok(())
                        }),
                        Page::Pdf(page) => {
                            page.fill_path(placement, c, CompositeMode::SrcOver, |pen| {
                                for cmd in commands {
                                    pen.command(cmd);
                                }
                                Ok(())
                            })
                        }
                    };
                }
                outlines
                    .load(GlyphId(gid), Scale::Unscaled, &mut scratch)
                    .map_err(outline_error)?;
                match &mut page {
                    Page::Svg(page) => {
                        page.fill_path(t, c, |pen| scratch.decompose(Point::default(), 2, pen))
                    }
                    Page::Pdf(page) => page.fill_path(t, c, CompositeMode::SrcOver, |pen| {
                        scratch.decompose(Point::default(), 2, pen)
                    }),
                }
                .map_err(outline_error)
            };
            if sofel_color::layers(&font.font, g.glyph).next().is_some() {
                recording.clear();
                if sofel_color::paint_recording_at(
                    instance,
                    g.glyph,
                    &options,
                    &mut color_scratch,
                    &mut recording,
                ) == sofel_color::Painted::ColrV0
                {
                    for cmd in recording.commands() {
                        if let sofel_color::PaintCommand::FillGlyph(gid, solid) = cmd {
                            fill(*gid, solid.color)?;
                        }
                    }
                    continue;
                }
            }
            fill(g.glyph, c)?;
        }
        page.write_to(output, len)
    })())
}

impl Page {
    fn draw_glyph(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        scratch: &mut Scratch,
        g: &CVectorGlyph,
    ) -> Result<(), i32> {
        if !g.transform.iter().all(|v| v.is_finite()) {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        outlines
            .load(GlyphId(g.glyph), Scale::Unscaled, scratch)
            .map_err(outline_error)?;
        let [xx, yx, xy, yy, dx, dy] = g.transform;
        // Decompose at double resolution to preserve implied half-unit points.
        let t = Transform {
            xx: xx * 0.5,
            yx: yx * 0.5,
            xy: xy * 0.5,
            yy: yy * 0.5,
            dx,
            dy,
        };
        let c = g.color.into();
        match self {
            Page::Svg(page) => {
                page.fill_path(t, c, |pen| scratch.decompose(Point::default(), 2, pen))
            }
            Page::Pdf(page) => page.fill_path(t, c, CompositeMode::SrcOver, |pen| {
                scratch.decompose(Point::default(), 2, pen)
            }),
        }
        .map_err(outline_error)
    }

    fn write_to(self, output: &mut [u8], len: &mut usize) -> Result<(), i32> {
        let data = match self {
            Page::Svg(page) => page.finish().into_bytes(),
            Page::Pdf(page) => page.finish(),
        };
        *len = data.len();
        output
            .get_mut(..data.len())
            .ok_or(ERROR_BUFFER_TOO_SMALL)?
            .copy_from_slice(&data);
        Ok(())
    }
}

/// Embedded PNG in the C SVG layer list.
#[repr(C)]
pub struct CSvgPng {
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
    extents: [i32; 4],
    clip: *const [f32; 4],
}

/// A transformed outline, or a PNG when the image pointer is present.
#[repr(C)]
pub struct CSvgLayer {
    glyph: CVectorGlyph,
    png: *const CSvgPng,
}

/// Writes glyph outlines and clipped PNG images in caller-specified order.
///
/// # Safety
/// The font, location, optional background, input and output ranges follow
/// `sofel_vector_page`. Each non-null PNG pointer addresses one descriptor,
/// whose data is readable for len bytes and whose optional clip has four floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_svg_page_layers(
    font: *const SofelFont,
    location: *const SofelLocation,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    background: *const CColor,
    layers: *const CSvgLayer,
    count: usize,
    output: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: all pointers and lengths meet the caller contract above.
    let (font, location, background, layers, output, len) = unsafe {
        (
            r(font),
            r(location),
            r(background),
            input_array(layers, count),
            out_array(output, capacity),
            w(len),
        )
    };
    status((|| {
        let layers = some(layers)?;
        let mut page = Page::Svg(SvgPage::new(
            x,
            y,
            width,
            height,
            background.copied().map(Color::from),
        ));
        let outlines = font.map(|f| outlines_at(f, location));
        let mut scratch = Scratch::default();
        for layer in layers {
            // SAFETY: each non-null descriptor and its pointed-to data are readable.
            if let Some(png) = unsafe { r(layer.png) } {
                // SAFETY: the descriptor supplies len readable bytes and an optional four-float clip.
                let (data, clip) = unsafe { (input_array(png.data, png.len), r(png.clip)) };
                let [x_bearing, y_bearing, width, height] = png.extents;
                let image = Image {
                    format: ImageFormat::Png,
                    data: some(data)?,
                    width: png.width,
                    height: png.height,
                    slant: 0.0,
                    extents: Some(GlyphExtents {
                        x_bearing,
                        y_bearing,
                        width,
                        height,
                    }),
                };
                let clip = clip.copied().map(|[x_min, y_min, x_max, y_max]| Rect {
                    x_min,
                    y_min,
                    x_max,
                    y_max,
                });
                let [xx, yx, xy, yy, dx, dy] = layer.glyph.transform;
                if !layer.glyph.transform.iter().all(|v| v.is_finite())
                    || clip.is_some_and(|r| {
                        ![r.x_min, r.y_min, r.x_max, r.y_max]
                            .iter()
                            .all(|v| v.is_finite())
                            || r.x_min > r.x_max
                            || r.y_min > r.y_max
                    })
                {
                    return Err(ERROR_INVALID_ARGUMENT);
                }
                let Page::Svg(svg) = &mut page else {
                    return Err(ERROR_INVALID_ARGUMENT);
                };
                if !svg.draw_image(
                    &image,
                    Transform {
                        xx,
                        yx,
                        xy,
                        yy,
                        dx,
                        dy,
                    },
                    clip,
                ) {
                    return Err(ERROR_INVALID_ARGUMENT);
                }
            } else {
                page.draw_glyph(some(outlines.as_ref())?, &mut scratch, &layer.glyph)?;
            }
        }
        page.write_to(some(output)?, some(len)?)
    })())
}
