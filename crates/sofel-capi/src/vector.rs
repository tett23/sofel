//! Vector page output from positioned glyphs (ADR 0068).
use crate::{
    ERROR_BUFFER_TOO_SMALL, ERROR_INVALID_ARGUMENT, SofelFont, SofelLocation, color::CColor,
    out_array, outline_error, outlines_at, r, some, status, variation::input_array, w,
};
use sofel_font::read::GlyphId;
use sofel_geom::{Color, CompositeMode, Point, Transform};
use sofel_outline::{Scale, Scratch};
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
        let outlines = outlines_at(some(font)?, location);
        let (glyphs, output, len) = (some(glyphs)?, some(output)?, some(len)?);
        let background = background.copied().map(Color::from);
        let mut page = match format {
            0 => Page::Svg(SvgPage::new(x, y, width, height, background)),
            1 => Page::Pdf(PdfPage::new(x, y, width, height, background)),
            _ => return Err(ERROR_INVALID_ARGUMENT),
        };
        let mut scratch = Scratch::default();
        for g in glyphs {
            if !g.transform.iter().all(|v| v.is_finite()) {
                return Err(ERROR_INVALID_ARGUMENT);
            }
            outlines
                .load(GlyphId(g.glyph), Scale::Unscaled, &mut scratch)
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
            match &mut page {
                Page::Svg(page) => {
                    page.fill_path(t, c, |pen| scratch.decompose(Point::default(), 2, pen))
                }
                Page::Pdf(page) => page.fill_path(t, c, CompositeMode::SrcOver, |pen| {
                    scratch.decompose(Point::default(), 2, pen)
                }),
            }
            .map_err(outline_error)?;
        }
        let data = match page {
            Page::Svg(page) => page.finish().into_bytes(),
            Page::Pdf(page) => page.finish(),
        };
        *len = data.len();
        output
            .get_mut(..data.len())
            .ok_or(ERROR_BUFFER_TOO_SMALL)?
            .copy_from_slice(&data);
        Ok(())
    })())
}
