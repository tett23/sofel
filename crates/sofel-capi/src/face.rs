//! Owned common-face C handles (root ADR 0100).

use sofel_font::{Face, FaceData, FaceError, OpenedFace, read::GlyphId};
use sofel_geom::{Command, Fixed, FnPen, Point, div_fix, mul_fix};
use sofel_glyph_render::FaceRenderError;
use sofel_outline::{CBox, OutlineInfo, Scale};
use sofel_raster::PixelBox;

use crate::{
    BitmapOut, CPen, CloseFunc, CubicFunc, ERROR_FONT_INDEX, ERROR_INVALID_ARGUMENT,
    ERROR_INVALID_FONT, ERROR_INVALID_GLYPH, ERROR_NO_GLYPH_DATA, ERROR_NOT_FOUND, ERROR_NULL,
    ERROR_TOO_LARGE, OK, PointFunc, QuadFunc, SofelOutlineScratch, SofelRasterizer, Status, bytes,
    ints, outline_error, put_info, r, raster_error, some, status, w,
};

/// `sofel_face_t`: owns its source and optional Type 1 attachment.
pub struct SofelFace {
    // Field order is the drop order: parsed views first, then attachment,
    // then the bytes and decoded source they borrow.
    opened: OpenedFace<'static>,
    _metrics: Option<Box<[u8]>>,
    data: Box<FaceData>,
    index: u32,
}

pub(crate) fn face_error(error: FaceError) -> i32 {
    match error {
        FaceError::Invalid => ERROR_INVALID_FONT,
        FaceError::IndexOutOfRange => ERROR_FONT_INDEX,
        FaceError::InvalidGlyph => ERROR_INVALID_GLYPH,
        FaceError::Overflow => ERROR_TOO_LARGE,
        FaceError::InvalidStrike | FaceError::InvalidSize | FaceError::Unsupported => {
            ERROR_INVALID_ARGUMENT
        }
    }
}

impl SofelFace {
    fn new(data: FaceData, index: u32, metrics: Option<(u32, Vec<u8>)>) -> Result<Self, i32> {
        let data = Box::new(data);
        let metrics = metrics.map(|(format, bytes)| (format, bytes.into_boxed_slice()));
        // SAFETY: `data` is in a Box and cannot move. The opened face is
        // dropped before `data`, as declared in SofelFace above.
        let source: &'static FaceData = unsafe { &*(data.as_ref() as *const FaceData) };
        let mut opened = source.open(index).map_err(face_error)?;
        if let Some((format, bytes)) = &metrics {
            if *format > 1 {
                return Err(ERROR_INVALID_ARGUMENT);
            }
            // SAFETY: `bytes` is in a Box and cannot move. The opened face
            // is dropped before `_metrics` and the boxed allocation remains live.
            let attachment: &'static [u8] = unsafe { &*(bytes.as_ref() as *const [u8]) };
            match format {
                0 => opened.attach_afm(attachment),
                1 => opened.attach_pfm(attachment),
                _ => unreachable!(),
            }
            .map_err(face_error)?;
        }
        Ok(Self {
            opened,
            _metrics: metrics.map(|(_, bytes)| bytes),
            data,
            index,
        })
    }

    pub(crate) fn opened<T>(
        &self,
        query: impl FnOnce(Face<'_>) -> Result<T, i32>,
    ) -> Result<T, i32> {
        query(self.opened.as_face())
    }
}

/// `sofel_face_count`.
///
/// # Safety
/// `data` points to `len` readable bytes; `count` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_count(data: *const u8, len: usize, count: *mut u32) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (data, count) = unsafe { (bytes(data, len), w(count)) };
    status((|| {
        let (data, count) = (some(data)?, some(count)?);
        *count = FaceData::new(data)
            .and_then(|face| face.face_count())
            .map_err(face_error)?;
        Ok(())
    })())
}

/// `sofel_face_create`.
///
/// # Safety
/// `data` points to `len` readable bytes; `error` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_create(
    data: *const u8,
    len: usize,
    index: u32,
    error: *mut i32,
) -> *mut SofelFace {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (data, error) = unsafe { (bytes(data, len), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    let result = data.ok_or(ERROR_NULL).and_then(|data| {
        let data = FaceData::new(data).map_err(face_error)?;
        SofelFace::new(data, index, None)
    });
    match result {
        Ok(face) => {
            *error = OK;
            Box::into_raw(Box::new(face))
        }
        Err(code) => {
            *error = code;
            core::ptr::null_mut()
        }
    }
}

/// `sofel_face_destroy`.
///
/// # Safety
/// `face` is NULL or a live handle returned by `sofel_face_create` or `sofel_face_with_metrics`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_destroy(face: *mut SofelFace) {
    if !face.is_null() {
        // SAFETY: the caller owns this handle and destroys it once.
        drop(unsafe { Box::from_raw(face) });
    }
}

fn query_u32(
    face: Option<&SofelFace>,
    out: Option<&mut u32>,
    f: impl FnOnce(Face<'_>) -> Result<u32, i32>,
) -> Status {
    let (face, out) = (some(face)?, some(out)?);
    *out = face.opened(f)?;
    Ok(())
}

/// `sofel_face_num_glyphs`.
///
/// # Safety
/// `face` is live or NULL; `count` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_num_glyphs(face: *const SofelFace, count: *mut u32) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, count) = unsafe { (r(face), w(count)) };
    status(query_u32(face, count, |f| Ok(f.glyph_count())))
}

/// `sofel_face_nominal_glyph`.
///
/// # Safety
/// `face` is live or NULL; `glyph` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_nominal_glyph(
    face: *const SofelFace,
    codepoint: u32,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, glyph) = unsafe { (r(face), w(glyph)) };
    status(query_u32(face, glyph, |f| {
        f.nominal_glyph(codepoint)
            .map(|g| g.0)
            .ok_or(ERROR_NOT_FOUND)
    }))
}

/// `sofel_face_strike_count`.
///
/// # Safety
/// `face` is live or NULL; `count` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_strike_count(face: *const SofelFace, count: *mut u32) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, count) = unsafe { (r(face), w(count)) };
    status(query_u32(face, count, |f| Ok(f.strike_count())))
}

/// `sofel_face_strike`.
///
/// # Safety
/// `face` is live or NULL; both output pointers are writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_strike(
    face: *const SofelFace,
    index: u32,
    x: *mut u32,
    y: *mut u32,
) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, x, y) = unsafe { (r(face), w(x), w(y)) };
    status((|| {
        let (face, x, y) = (some(face)?, some(x)?, some(y)?);
        let pair = face.opened(|f| f.strike(index).ok_or(ERROR_INVALID_ARGUMENT))?;
        (*x, *y) = pair;
        Ok(())
    })())
}

pub(crate) fn selected(strike: u32) -> Option<u32> {
    (strike != u32::MAX).then_some(strike)
}

/// `sofel_face_h_advance`.
///
/// # Safety
/// `face` is live or NULL; `advance` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_h_advance(
    face: *const SofelFace,
    glyph: u32,
    ppem: u16,
    strike: u32,
    advance: *mut i32,
) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, advance) = unsafe { (r(face), w(advance)) };
    status((|| {
        let (face, advance) = (some(face)?, some(advance)?);
        *advance = face.opened(|f| {
            f.h_advance(GlyphId(glyph), ppem, selected(strike))
                .map_err(face_error)
        })?;
        Ok(())
    })())
}

/// `sofel_face_kerning`.
///
/// # Safety
/// `face` is live or NULL; both output pointers are writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_kerning(
    face: *const SofelFace,
    left: u32,
    right: u32,
    ppem: u16,
    strike: u32,
    x: *mut i32,
    y: *mut i32,
) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, x, y) = unsafe { (r(face), w(x), w(y)) };
    status((|| {
        let (face, x, y) = (some(face)?, some(x)?, some(y)?);
        let pair = face.opened(|f| {
            f.kerning(GlyphId(left), GlyphId(right), ppem, selected(strike))
                .map_err(face_error)
        })?;
        (*x, *y) = pair;
        Ok(())
    })())
}

/// `sofel_face_with_metrics`.
///
/// # Safety
/// `face` is live or NULL; `data` points to `len` readable bytes; `error` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_with_metrics(
    face: *const SofelFace,
    data: *const u8,
    len: usize,
    format: u32,
    error: *mut i32,
) -> *mut SofelFace {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, data, error) = unsafe { (r(face), bytes(data, len), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    let result = (|| {
        let (face, data) = (some(face)?, some(data)?);
        if format > 1 {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        if !matches!(face.opened.as_face(), Face::Type1(_)) {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        SofelFace::new(
            face.data.as_ref().clone(),
            face.index,
            Some((format, data.to_vec())),
        )
    })();
    match result {
        Ok(face) => {
            *error = OK;
            Box::into_raw(Box::new(face))
        }
        Err(code) => {
            *error = code;
            core::ptr::null_mut()
        }
    }
}

/// `sofel_face_render_glyph`.
///
/// # Safety
/// `face`, `scratch`, and `rasterizer` are live or NULL; the output pointers
/// are writable for their declared capacities and do not alias each other.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_render_glyph(
    face: *const SofelFace,
    glyph: u32,
    ppem: u16,
    strike: u32,
    scratch: *mut SofelOutlineScratch,
    rasterizer: *mut SofelRasterizer,
    pixels: *mut u8,
    capacity: usize,
    len: *mut usize,
    box_: *mut i32,
) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, scratch, rasterizer, pixels, len, b) = unsafe {
        (
            r(face),
            w(scratch),
            w(rasterizer),
            super::out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
        )
    };
    let out = BitmapOut { pixels, len, b };
    status((|| {
        let (face, scratch, rasterizer) = (some(face)?, some(scratch)?, some(rasterizer)?);
        out.check()?;
        let image = face.opened(|f| {
            sofel_glyph_render::render_face(
                f,
                GlyphId(glyph),
                ppem,
                selected(strike),
                &mut scratch.scratch,
                &mut rasterizer.raster,
            )
            .map_err(|e| match e {
                FaceRenderError::Face(e) => face_error(e),
                FaceRenderError::Render(super::RenderError::Outline(e)) => outline_error(e),
                FaceRenderError::Render(super::RenderError::Raster(e)) => raster_error(e),
                FaceRenderError::Bitmap => ERROR_INVALID_FONT,
            })
        })?;
        out.put_bytes(
            PixelBox {
                left: image.left,
                top: image.top,
                width: image.width,
                rows: image.rows,
            },
            &image.pixels,
        )
    })())
}

fn draw_face_outline(
    face: Face<'_>,
    glyph: GlyphId,
    ppem: u16,
    scratch: &mut SofelOutlineScratch,
    pen: &mut CPen,
) -> Result<OutlineInfo, i32> {
    face.h_advance(glyph, ppem, None).map_err(face_error)?;
    match face {
        Face::Sfnt(font) => sofel_outline::draw(
            font.default_instance(),
            glyph,
            Scale::pixels(font, ppem, ppem),
            &mut scratch.scratch,
            pen,
        )
        .map_err(outline_error),
        Face::BareCff(font) => {
            let units = i32::try_from(font.cff().top_matrix().units_per_em)
                .ok()
                .filter(|v| *v > 0)
                .ok_or(ERROR_INVALID_FONT)?;
            let fixed = Fixed(div_fix(i32::from(ppem).saturating_mul(64), units));
            sofel_outline::draw_bare_cff(
                font.cff(),
                glyph,
                Scale::Scaled { x: fixed, y: fixed },
                &mut scratch.scratch,
                pen,
            )
            .map_err(outline_error)
        }
        Face::Type1(font) => {
            let fixed = Fixed(div_fix(i32::from(ppem).saturating_mul(64), 1000));
            sofel_outline::draw_type1(
                font,
                usize::try_from(glyph.0).map_err(|_| ERROR_INVALID_GLYPH)?,
                Scale::Scaled { x: fixed, y: fixed },
                pen,
            )
            .map_err(outline_error)
        }
        Face::Cid(font) => {
            let fixed = Fixed(div_fix(
                i32::from(ppem).saturating_mul(64),
                i32::from(font.units_per_em()),
            ));
            sofel_outline::draw_cid(
                font,
                usize::try_from(glyph.0).map_err(|_| ERROR_INVALID_GLYPH)?,
                Scale::Scaled { x: fixed, y: fixed },
                pen,
            )
            .map_err(outline_error)
        }
        Face::Type42(font) => {
            let embedded = font.embedded_glyph(glyph).ok_or(ERROR_INVALID_GLYPH)?;
            let inner = font.inner_font();
            sofel_outline::draw(
                inner.default_instance(),
                embedded,
                Scale::pixels(&inner, ppem, ppem),
                &mut scratch.scratch,
                pen,
            )
            .map_err(outline_error)
        }
        Face::Pfr(font) => {
            let fixed = div_fix(
                i32::from(ppem).saturating_mul(64),
                i32::from(font.units_per_em()),
            );
            let mut commands = Vec::new();
            sofel_outline::draw_pfr(font, glyph, &mut FnPen(|cmd| commands.push(cmd)))
                .map_err(|_| crate::ERROR_INVALID_OUTLINE)?;
            let mut box_ = CBox {
                x_min: i32::MAX,
                y_min: i32::MAX,
                x_max: i32::MIN,
                y_max: i32::MIN,
            };
            let mut seen = false;
            let mut visit = |point: Point| {
                let point = Point {
                    x: mul_fix(point.x, fixed),
                    y: mul_fix(point.y, fixed),
                };
                box_.x_min = box_.x_min.min(point.x);
                box_.y_min = box_.y_min.min(point.y);
                box_.x_max = box_.x_max.max(point.x);
                box_.y_max = box_.y_max.max(point.y);
                seen = true;
                point
            };
            for command in commands {
                let transformed = match command {
                    Command::MoveTo(p) => Command::MoveTo(visit(p)),
                    Command::LineTo(p) => Command::LineTo(visit(p)),
                    Command::QuadTo(c, p) => Command::QuadTo(visit(c), visit(p)),
                    Command::CubicTo(a, b, c) => Command::CubicTo(visit(a), visit(b), visit(c)),
                    Command::Close => Command::Close,
                };
                transformed.apply(pen);
            }
            Ok(OutlineInfo {
                cbox: if seen { box_ } else { CBox::default() },
                overlap: false,
            })
        }
        Face::Bitmap(_) => Err(ERROR_NO_GLYPH_DATA),
    }
}

/// `sofel_face_outline_draw`.
///
/// # Safety
/// The face and scratch are live or NULL. Callbacks are callable or NULL;
/// `cbox` points to four writable integers and `overlap` is writable or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_face_outline_draw(
    face: *const SofelFace,
    glyph: u32,
    ppem: u16,
    scratch: *mut SofelOutlineScratch,
    move_to: PointFunc,
    line_to: PointFunc,
    quad_to: QuadFunc,
    cubic_to: CubicFunc,
    close: CloseFunc,
    user: *mut core::ffi::c_void,
    cbox: *mut i32,
    overlap: *mut u32,
) -> i32 {
    // SAFETY: the caller's pointer conditions are checked by the helpers.
    let (face, scratch, cbox, overlap) =
        unsafe { (r(face), w(scratch), ints::<4>(cbox), w(overlap)) };
    status((|| {
        let (face, scratch, cbox, overlap) =
            (some(face)?, some(scratch)?, some(cbox)?, some(overlap)?);
        let mut pen = CPen::new(move_to, line_to, quad_to, cubic_to, close, user)?;
        let info =
            face.opened(|f| draw_face_outline(f, GlyphId(glyph), ppem, scratch, &mut pen))?;
        put_info(info, cbox, overlap);
        Ok(())
    })())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn adr0100_i3_face_handle_shares_read_only_queries() {
        let bytes = include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf");
        let face = SofelFace::new(FaceData::new(bytes).expect("BDF"), 0, None).expect("face");
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..32 {
                        assert_eq!(
                            face.opened(|f| Ok(f.nominal_glyph(65).map(|g| g.0))),
                            Ok(Some(1))
                        );
                    }
                });
            }
        });
    }
}
