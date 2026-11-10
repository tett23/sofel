//! The C ABI of sofel (ADR 0015). The declarations are in
//! `include/sofel.h`, written by hand; this crate implements them.
//!
//! Every exported function first turns its pointer arguments into
//! references or slices in one `unsafe` block, with the conditions the
//! caller must meet written as `// SAFETY:`. The rest is done by safe
//! functions that take `Option`s of those references, so that a NULL
//! pointer becomes [`ERROR_NULL`] instead of undefined behavior. No
//! function panics; a panic would abort the process at the `extern "C"`
//! boundary instead of unwinding into C.

#![warn(missing_docs)]
// The functions mirror the C declarations in sofel.h, argument for argument.
#![allow(clippy::too_many_arguments)]

use core::ffi::{c_char, c_void};
use core::fmt::Write as _;
use std::ffi::CString;

use sofel_cli::glyph::RenderError;
use sofel_font::read::sfnt::FontFile;
use sofel_font::read::{DetailValue, Diagnostic, GlyphId, Severity, Tag, validate};
use sofel_font::{Font, FontError};
use sofel_geom::{Fixed, Matrix, Pen, Point};
use sofel_outline::{OutlineError, OutlineInfo, Scale, Scratch};
use sofel_raster::{Bitmap, FillRule, PixelBox, RasterError, Rasterizer, RenderOptions};

/// `SOFEL_VERSION_MAJOR`.
pub const VERSION_MAJOR: u32 = 0;
/// `SOFEL_VERSION_MINOR`.
pub const VERSION_MINOR: u32 = 0;
/// `SOFEL_VERSION_PATCH`.
pub const VERSION_PATCH: u32 = 0;

/// `SOFEL_OK`.
pub const OK: i32 = 0;
/// `SOFEL_ERROR_NULL`.
pub const ERROR_NULL: i32 = -1;
/// `SOFEL_ERROR_INVALID_FONT`.
pub const ERROR_INVALID_FONT: i32 = -2;
/// `SOFEL_ERROR_NO_HEAD`.
pub const ERROR_NO_HEAD: i32 = -3;
/// `SOFEL_ERROR_FONT_INDEX`.
pub const ERROR_FONT_INDEX: i32 = -4;
/// `SOFEL_ERROR_NOT_FOUND`.
pub const ERROR_NOT_FOUND: i32 = -5;
/// `SOFEL_ERROR_INVALID_GLYPH`.
pub const ERROR_INVALID_GLYPH: i32 = -6;
/// `SOFEL_ERROR_NO_GLYPH_DATA`.
pub const ERROR_NO_GLYPH_DATA: i32 = -7;
/// `SOFEL_ERROR_INVALID_OUTLINE`.
pub const ERROR_INVALID_OUTLINE: i32 = -8;
/// `SOFEL_ERROR_INVALID_COMPOSITE`.
pub const ERROR_INVALID_COMPOSITE: i32 = -9;
/// `SOFEL_ERROR_TOO_LARGE`.
pub const ERROR_TOO_LARGE: i32 = -10;
/// `SOFEL_ERROR_BUFFER_TOO_SMALL`.
pub const ERROR_BUFFER_TOO_SMALL: i32 = -11;
/// `SOFEL_ERROR_INVALID_ARGUMENT`.
pub const ERROR_INVALID_ARGUMENT: i32 = -12;

/// `sofel_font_t`: a font over data the caller keeps alive.
pub struct SofelFont {
    font: Font<'static>,
}

/// `sofel_outline_scratch_t`.
pub struct SofelOutlineScratch {
    scratch: Scratch,
}

/// `sofel_rasterizer_t`.
pub struct SofelRasterizer {
    raster: Rasterizer,
}

/// `sofel_diagnostic_t`: one problem, with the index of its font in a
/// collection.
pub struct SofelDiagnostic {
    font: Option<u32>,
    diagnostic: Diagnostic,
}

/// `sofel_move_to_func_t` and `sofel_line_to_func_t`.
pub type PointFunc = Option<extern "C" fn(*mut c_void, i32, i32)>;
/// `sofel_quad_to_func_t`.
pub type QuadFunc = Option<extern "C" fn(*mut c_void, i32, i32, i32, i32)>;
/// `sofel_cubic_to_func_t`.
pub type CubicFunc = Option<extern "C" fn(*mut c_void, i32, i32, i32, i32, i32, i32)>;
/// `sofel_close_func_t`.
pub type CloseFunc = Option<extern "C" fn(*mut c_void)>;
/// `sofel_diagnostic_func_t`.
pub type DiagnosticFunc = Option<extern "C" fn(*mut c_void, *const SofelDiagnostic)>;
/// `sofel_detail_func_t`.
pub type DetailFunc = Option<extern "C" fn(*mut c_void, *const c_char, i32, i64)>;

type Status = Result<(), i32>;

fn status(s: Status) -> i32 {
    s.err().unwrap_or(OK)
}

fn some<T>(v: Option<T>) -> Result<T, i32> {
    v.ok_or(ERROR_NULL)
}

// ------------------------------------------------------------ pointers

/// A reference from a pointer, `None` for NULL.
///
/// # Safety
///
/// `p` is NULL or points to a live, aligned `T` that nothing mutates
/// during `'a`.
unsafe fn r<'a, T>(p: *const T) -> Option<&'a T> {
    // SAFETY: the caller's conditions are those of `as_ref`.
    unsafe { p.as_ref() }
}

/// A mutable reference from a pointer, `None` for NULL.
///
/// # Safety
///
/// `p` is NULL or points to a live, aligned `T` that nothing else
/// accesses during `'a`.
unsafe fn w<'a, T>(p: *mut T) -> Option<&'a mut T> {
    // SAFETY: the caller's conditions are those of `as_mut`.
    unsafe { p.as_mut() }
}

/// `len` bytes at `p`, `None` for NULL.
///
/// # Safety
///
/// `p` is NULL or valid for reads of `len` bytes that nothing mutates
/// during `'a`.
unsafe fn bytes<'a>(p: *const u8, len: usize) -> Option<&'a [u8]> {
    if p.is_null() {
        return None;
    }
    // SAFETY: `p` is not NULL, and the caller guarantees the rest.
    Some(unsafe { core::slice::from_raw_parts(p, len) })
}

/// The caller's output array of `capacity` elements: empty for NULL
/// with capacity 0, `None` for NULL otherwise.
///
/// # Safety
///
/// `p` is NULL or valid for writes of `capacity` elements that nothing
/// else accesses during `'a`.
unsafe fn out_array<'a, T>(p: *mut T, capacity: usize) -> Option<&'a mut [T]> {
    if p.is_null() {
        return (capacity == 0).then_some(&mut []);
    }
    // SAFETY: `p` is not NULL, and the caller guarantees the rest.
    Some(unsafe { core::slice::from_raw_parts_mut(p, capacity) })
}

/// The caller's array of `N` integers.
///
/// # Safety
///
/// `p` is NULL or valid for writes of `N` `i32`s that nothing else
/// accesses during `'a`.
unsafe fn ints<'a, const N: usize>(p: *mut i32) -> Option<&'a mut [i32; N]> {
    // SAFETY: the caller's conditions; `[i32; N]` has the alignment of `i32`.
    unsafe { p.cast::<[i32; N]>().as_mut() }
}

// ------------------------------------------------------------- version

/// `sofel_version`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_version() -> u32 {
    (VERSION_MAJOR << 16) | (VERSION_MINOR << 8) | VERSION_PATCH
}

// -------------------------------------------------------------- errors

fn error_text(error: i32) -> &'static core::ffi::CStr {
    match error {
        OK => c"success",
        ERROR_NULL => c"null pointer argument",
        ERROR_INVALID_FONT => c"not a readable sfnt or collection",
        ERROR_NO_HEAD => c"no readable head table",
        ERROR_FONT_INDEX => c"no font with this index",
        ERROR_NOT_FOUND => c"not found",
        ERROR_INVALID_GLYPH => c"glyph id out of range",
        ERROR_NO_GLYPH_DATA => c"no glyf and loca tables",
        ERROR_INVALID_OUTLINE => c"invalid outline",
        ERROR_INVALID_COMPOSITE => c"invalid composite glyph",
        ERROR_TOO_LARGE => c"bitmap too large",
        ERROR_BUFFER_TOO_SMALL => c"buffer too small",
        ERROR_INVALID_ARGUMENT => c"invalid argument",
        _ => c"unknown error",
    }
}

/// `sofel_error_string`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_error_string(error: i32) -> *const c_char {
    error_text(error).as_ptr()
}

fn font_error(e: FontError) -> i32 {
    match e {
        FontError::Read(_) => ERROR_INVALID_FONT,
        FontError::NoHead => ERROR_NO_HEAD,
        FontError::IndexOutOfRange => ERROR_FONT_INDEX,
    }
}

fn outline_error(e: OutlineError) -> i32 {
    match e {
        OutlineError::InvalidGlyphIndex => ERROR_INVALID_GLYPH,
        OutlineError::NoGlyphData => ERROR_NO_GLYPH_DATA,
        OutlineError::InvalidOutline => ERROR_INVALID_OUTLINE,
        OutlineError::InvalidComposite => ERROR_INVALID_COMPOSITE,
    }
}

fn raster_error(e: RasterError) -> i32 {
    match e {
        RasterError::TooLarge => ERROR_TOO_LARGE,
    }
}

// ------------------------------------------------------------ geometry

/// `sofel_mul_fix`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_mul_fix(a: i32, b: i32) -> i32 {
    sofel_geom::mul_fix(a, b)
}

/// `sofel_div_fix`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_div_fix(a: i32, b: i32) -> i32 {
    sofel_geom::div_fix(a, b)
}

/// `sofel_mul_div`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_mul_div(a: i32, b: i32, c: i32) -> i32 {
    sofel_geom::mul_div(a, b, c)
}

/// `sofel_mul_div_no_round`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_mul_div_no_round(a: i32, b: i32, c: i32) -> i32 {
    sofel_geom::mul_div_no_round(a, b, c)
}

fn transform(x: Option<&mut i32>, y: Option<&mut i32>, m: Matrix) -> Status {
    let (x, y) = (some(x)?, some(y)?);
    let p = sofel_geom::transform(Point { x: *x, y: *y }, &m);
    (*x, *y) = (p.x, p.y);
    Ok(())
}

/// `sofel_transform`.
///
/// # Safety
///
/// `x` and `y` are NULL or point to writable `int32_t`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_transform(
    x: *mut i32,
    y: *mut i32,
    xx: i32,
    xy: i32,
    yx: i32,
    yy: i32,
) -> i32 {
    // SAFETY: the caller passes NULL or writable, distinct integers.
    let (x, y) = unsafe { (w(x), w(y)) };
    let m = Matrix {
        xx: Fixed(xx),
        xy: Fixed(xy),
        yx: Fixed(yx),
        yy: Fixed(yy),
    };
    status(transform(x, y, m))
}

/// `sofel_vector_length`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_vector_length(x: i32, y: i32) -> i32 {
    sofel_geom::vector_length(x, y)
}

// --------------------------------------------------------------- fonts

fn font_create(data: Option<&'static [u8]>, index: u32) -> Result<SofelFont, i32> {
    let font = Font::new(some(data)?, index).map_err(font_error)?;
    Ok(SofelFont { font })
}

/// `sofel_font_create`.
///
/// # Safety
///
/// `data` is NULL or valid for reads of `len` bytes, which stay alive
/// and unchanged until the font is destroyed; `error` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_create(
    data: *const u8,
    len: usize,
    index: u32,
    error: *mut i32,
) -> *mut SofelFont {
    // SAFETY: the caller keeps the data alive and unchanged until
    // sofel_font_destroy, which is the only place the font (and the
    // 'static borrow in it) is dropped; `error` is NULL or writable.
    let (data, error) = unsafe { (bytes(data, len), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    match font_create(data, index) {
        Ok(f) => {
            *error = OK;
            Box::into_raw(Box::new(f))
        }
        Err(e) => {
            *error = e;
            core::ptr::null_mut()
        }
    }
}

/// `sofel_font_destroy`.
///
/// # Safety
///
/// `font` is NULL or a font from `sofel_font_create` not yet destroyed,
/// that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_destroy(font: *mut SofelFont) {
    if !font.is_null() {
        // SAFETY: the pointer came from Box::into_raw in sofel_font_create
        // and is dropped once.
        drop(unsafe { Box::from_raw(font) });
    }
}

/// The font of a `sofel_font_t` pointer.
///
/// # Safety
///
/// `font` is NULL or a live font from `sofel_font_create`.
unsafe fn font_ref<'a>(font: *const SofelFont) -> Option<&'a Font<'static>> {
    // SAFETY: the caller's conditions.
    unsafe { r(font) }.map(|f| &f.font)
}

fn get<T>(font: Option<&Font<'_>>, out: Option<&mut T>, f: impl FnOnce(&Font<'_>) -> T) -> Status {
    let (font, out) = (some(font)?, some(out)?);
    *out = f(font);
    Ok(())
}

fn find<T>(
    font: Option<&Font<'_>>,
    out: Option<&mut T>,
    f: impl FnOnce(&Font<'_>) -> Option<T>,
) -> Status {
    let (font, out) = (some(font)?, some(out)?);
    *out = f(font).ok_or(ERROR_NOT_FOUND)?;
    Ok(())
}

/// `sofel_font_units_per_em`.
///
/// # Safety
///
/// `font` is NULL or a live font; `upem` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_units_per_em(font: *const SofelFont, upem: *mut u16) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, upem) = unsafe { (font_ref(font), w(upem)) };
    status(get(font, upem, |f| f.units_per_em()))
}

/// `sofel_font_num_glyphs`.
///
/// # Safety
///
/// `font` is NULL or a live font; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_num_glyphs(font: *const SofelFont, count: *mut u32) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, |f| f.num_glyphs()))
}

/// `sofel_font_nominal_glyph`.
///
/// # Safety
///
/// `font` is NULL or a live font; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_nominal_glyph(
    font: *const SofelFont,
    codepoint: u32,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, glyph) = unsafe { (font_ref(font), w(glyph)) };
    status(find(font, glyph, |f| {
        f.nominal_glyph(codepoint).map(|g| g.0)
    }))
}

/// `sofel_font_variation_glyph`.
///
/// # Safety
///
/// `font` is NULL or a live font; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_variation_glyph(
    font: *const SofelFont,
    codepoint: u32,
    selector: u32,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, glyph) = unsafe { (font_ref(font), w(glyph)) };
    status(find(font, glyph, |f| {
        f.variation_glyph(codepoint, selector).map(|g| g.0)
    }))
}

/// `sofel_font_h_advance`.
///
/// # Safety
///
/// `font` is NULL or a live font; `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_h_advance(
    font: *const SofelFont,
    glyph: u32,
    advance: *mut u16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, advance) = unsafe { (font_ref(font), w(advance)) };
    status(get(font, advance, |f| f.advance(GlyphId(glyph))))
}

fn font_table(
    font: Option<&Font<'static>>,
    tag: u32,
    data: Option<&mut *const u8>,
    len: Option<&mut usize>,
) -> Status {
    let (font, data, len) = (some(font)?, some(data)?, some(len)?);
    let t = font.table(Tag::from_u32(tag)).ok_or(ERROR_NOT_FOUND)?;
    (*data, *len) = (t.as_ptr(), t.len());
    Ok(())
}

/// `sofel_font_table`.
///
/// # Safety
///
/// `font` is NULL or a live font; `data` and `len` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_table(
    font: *const SofelFont,
    tag: u32,
    data: *mut *const u8,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, data, len) = unsafe { (font_ref(font), w(data), w(len)) };
    status(font_table(font, tag, data, len))
}

// ------------------------------------------------------------ outlines

/// The C pen functions as a [`Pen`].
struct CPen {
    move_to: extern "C" fn(*mut c_void, i32, i32),
    line_to: extern "C" fn(*mut c_void, i32, i32),
    quad_to: extern "C" fn(*mut c_void, i32, i32, i32, i32),
    cubic_to: extern "C" fn(*mut c_void, i32, i32, i32, i32, i32, i32),
    close: extern "C" fn(*mut c_void),
    user: *mut c_void,
}

impl CPen {
    fn new(
        move_to: PointFunc,
        line_to: PointFunc,
        quad_to: QuadFunc,
        cubic_to: CubicFunc,
        close: CloseFunc,
        user: *mut c_void,
    ) -> Result<CPen, i32> {
        Ok(CPen {
            move_to: some(move_to)?,
            line_to: some(line_to)?,
            quad_to: some(quad_to)?,
            cubic_to: some(cubic_to)?,
            close: some(close)?,
            user,
        })
    }
}

impl Pen for CPen {
    fn move_to(&mut self, p: Point) {
        (self.move_to)(self.user, p.x, p.y);
    }
    fn line_to(&mut self, p: Point) {
        (self.line_to)(self.user, p.x, p.y);
    }
    fn quad_to(&mut self, c: Point, p: Point) {
        (self.quad_to)(self.user, c.x, c.y, p.x, p.y);
    }
    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        (self.cubic_to)(self.user, c1.x, c1.y, c2.x, c2.y, p.x, p.y);
    }
    fn close(&mut self) {
        (self.close)(self.user);
    }
}

/// `sofel_outline_scratch_create`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_outline_scratch_create() -> *mut SofelOutlineScratch {
    Box::into_raw(Box::new(SofelOutlineScratch {
        scratch: Scratch::default(),
    }))
}

/// `sofel_outline_scratch_destroy`.
///
/// # Safety
///
/// `scratch` is NULL or from `sofel_outline_scratch_create`, not yet
/// destroyed and not in use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_outline_scratch_destroy(scratch: *mut SofelOutlineScratch) {
    if !scratch.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(scratch) });
    }
}

fn pixel_scale(
    font: Option<&Font<'_>>,
    x_ppem: u16,
    y_ppem: u16,
    x_scale: Option<&mut i32>,
    y_scale: Option<&mut i32>,
) -> Status {
    let (font, xs, ys) = (some(font)?, some(x_scale)?, some(y_scale)?);
    if let Scale::Scaled { x, y } = Scale::pixels(font, x_ppem, y_ppem) {
        (*xs, *ys) = (x.0, y.0);
    }
    Ok(())
}

/// `sofel_outline_pixel_scale`.
///
/// # Safety
///
/// `font` is NULL or a live font; `x_scale` and `y_scale` are NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_outline_pixel_scale(
    font: *const SofelFont,
    x_ppem: u16,
    y_ppem: u16,
    x_scale: *mut i32,
    y_scale: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, xs, ys) = unsafe { (font_ref(font), w(x_scale), w(y_scale)) };
    status(pixel_scale(font, x_ppem, y_ppem, xs, ys))
}

fn scale(mode: u32, x: i32, y: i32) -> Result<Scale, i32> {
    match mode {
        0 => Ok(Scale::Unscaled),
        1 => Ok(Scale::Scaled {
            x: Fixed(x),
            y: Fixed(y),
        }),
        _ => Err(ERROR_INVALID_ARGUMENT),
    }
}

fn put_info(info: OutlineInfo, cbox: &mut [i32; 4], overlap: &mut u32) {
    let c = info.cbox;
    *cbox = [c.x_min, c.y_min, c.x_max, c.y_max];
    *overlap = u32::from(info.overlap);
}

fn outline_load(
    font: Option<&Font<'_>>,
    glyph: u32,
    scale: Result<Scale, i32>,
    scratch: Option<&mut SofelOutlineScratch>,
    cbox: Option<&mut [i32; 4]>,
    overlap: Option<&mut u32>,
) -> Status {
    let (font, scratch, cbox, overlap) = (some(font)?, some(scratch)?, some(cbox)?, some(overlap)?);
    let info = sofel_outline::load(font, GlyphId(glyph), scale?, &mut scratch.scratch)
        .map_err(outline_error)?;
    put_info(info, cbox, overlap);
    Ok(())
}

/// `sofel_outline_load`.
///
/// # Safety
///
/// `font` is NULL or a live font; `scratch` is NULL or a live work area
/// no other thread uses; `cbox` is NULL or writable for 4 integers;
/// `overlap` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_outline_load(
    font: *const SofelFont,
    glyph: u32,
    mode: u32,
    x_scale: i32,
    y_scale: i32,
    scratch: *mut SofelOutlineScratch,
    cbox: *mut i32,
    overlap: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, scratch, cbox, overlap) =
        unsafe { (font_ref(font), w(scratch), ints::<4>(cbox), w(overlap)) };
    status(outline_load(
        font,
        glyph,
        scale(mode, x_scale, y_scale),
        scratch,
        cbox,
        overlap,
    ))
}

fn outline_decompose(
    scratch: Option<&SofelOutlineScratch>,
    offset: Point,
    multiplier: i32,
    pen: Result<CPen, i32>,
) -> Status {
    let (scratch, mut pen) = (some(scratch)?, pen?);
    scratch
        .scratch
        .decompose(offset, multiplier, &mut pen)
        .map_err(outline_error)
}

/// `sofel_outline_decompose`.
///
/// # Safety
///
/// `scratch` is NULL or a live work area that no other thread changes.
/// The pen functions are NULL or functions of the declared types.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_outline_decompose(
    scratch: *const SofelOutlineScratch,
    dx: i32,
    dy: i32,
    multiplier: i32,
    move_to: PointFunc,
    line_to: PointFunc,
    quad_to: QuadFunc,
    cubic_to: CubicFunc,
    close: CloseFunc,
    user: *mut c_void,
) -> i32 {
    // SAFETY: the caller's conditions.
    let scratch = unsafe { r(scratch) };
    let pen = CPen::new(move_to, line_to, quad_to, cubic_to, close, user);
    status(outline_decompose(
        scratch,
        Point { x: dx, y: dy },
        multiplier,
        pen,
    ))
}

fn outline_draw(
    font: Option<&Font<'_>>,
    glyph: u32,
    scale: Result<Scale, i32>,
    scratch: Option<&mut SofelOutlineScratch>,
    pen: Result<CPen, i32>,
    cbox: Option<&mut [i32; 4]>,
    overlap: Option<&mut u32>,
) -> Status {
    let (font, scratch, cbox, overlap) = (some(font)?, some(scratch)?, some(cbox)?, some(overlap)?);
    let (scale, mut pen) = (scale?, pen?);
    let info = sofel_outline::draw(font, GlyphId(glyph), scale, &mut scratch.scratch, &mut pen)
        .map_err(outline_error)?;
    put_info(info, cbox, overlap);
    Ok(())
}

/// `sofel_outline_draw`.
///
/// # Safety
///
/// As [`sofel_outline_load`] and [`sofel_outline_decompose`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_outline_draw(
    font: *const SofelFont,
    glyph: u32,
    mode: u32,
    x_scale: i32,
    y_scale: i32,
    scratch: *mut SofelOutlineScratch,
    move_to: PointFunc,
    line_to: PointFunc,
    quad_to: QuadFunc,
    cubic_to: CubicFunc,
    close: CloseFunc,
    user: *mut c_void,
    cbox: *mut i32,
    overlap: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, scratch, cbox, overlap) =
        unsafe { (font_ref(font), w(scratch), ints::<4>(cbox), w(overlap)) };
    let pen = CPen::new(move_to, line_to, quad_to, cubic_to, close, user);
    status(outline_draw(
        font,
        glyph,
        scale(mode, x_scale, y_scale),
        scratch,
        pen,
        cbox,
        overlap,
    ))
}

// ------------------------------------------------------- rasterization

fn new_rasterizer(raster: Rasterizer) -> *mut SofelRasterizer {
    Box::into_raw(Box::new(SofelRasterizer { raster }))
}

/// `sofel_rasterizer_create`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_rasterizer_create() -> *mut SofelRasterizer {
    new_rasterizer(Rasterizer::new())
}

/// `sofel_rasterizer_create_with_band_cells`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_rasterizer_create_with_band_cells(cells: usize) -> *mut SofelRasterizer {
    new_rasterizer(Rasterizer::with_band_cells(cells))
}

/// `sofel_rasterizer_destroy`.
///
/// # Safety
///
/// `rasterizer` is NULL or from `sofel_rasterizer_create*`, not yet
/// destroyed and not in use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_destroy(rasterizer: *mut SofelRasterizer) {
    if !rasterizer.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(rasterizer) });
    }
}

/// `sofel_rasterizer_clear`.
///
/// # Safety
///
/// `rasterizer` is NULL or a live rasterizer no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_clear(rasterizer: *mut SofelRasterizer) -> i32 {
    // SAFETY: the caller's conditions.
    let rasterizer = unsafe { w(rasterizer) };
    status(some(rasterizer).map(|r| r.raster.clear()))
}

/// The rasterizer behind the `void *` of its pen functions.
///
/// # Safety
///
/// `p` is NULL or a live rasterizer no other thread uses.
unsafe fn raster_pen<'a>(p: *mut c_void) -> Option<&'a mut Rasterizer> {
    // SAFETY: the caller's conditions.
    unsafe { w(p.cast::<SofelRasterizer>()) }.map(|r| &mut r.raster)
}

/// `sofel_rasterizer_move_to`.
///
/// # Safety
///
/// `rasterizer` is NULL or a live `sofel_rasterizer_t` no other thread
/// uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_move_to(rasterizer: *mut c_void, x: i32, y: i32) {
    // SAFETY: the caller's conditions.
    if let Some(r) = unsafe { raster_pen(rasterizer) } {
        r.move_to(Point { x, y });
    }
}

/// `sofel_rasterizer_line_to`.
///
/// # Safety
///
/// As [`sofel_rasterizer_move_to`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_line_to(rasterizer: *mut c_void, x: i32, y: i32) {
    // SAFETY: the caller's conditions.
    if let Some(r) = unsafe { raster_pen(rasterizer) } {
        r.line_to(Point { x, y });
    }
}

/// `sofel_rasterizer_quad_to`.
///
/// # Safety
///
/// As [`sofel_rasterizer_move_to`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_quad_to(
    rasterizer: *mut c_void,
    cx: i32,
    cy: i32,
    x: i32,
    y: i32,
) {
    // SAFETY: the caller's conditions.
    if let Some(r) = unsafe { raster_pen(rasterizer) } {
        r.quad_to(Point { x: cx, y: cy }, Point { x, y });
    }
}

/// `sofel_rasterizer_cubic_to`.
///
/// # Safety
///
/// As [`sofel_rasterizer_move_to`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_cubic_to(
    rasterizer: *mut c_void,
    c1x: i32,
    c1y: i32,
    c2x: i32,
    c2y: i32,
    x: i32,
    y: i32,
) {
    // SAFETY: the caller's conditions.
    if let Some(r) = unsafe { raster_pen(rasterizer) } {
        r.cubic_to(
            Point { x: c1x, y: c1y },
            Point { x: c2x, y: c2y },
            Point { x, y },
        );
    }
}

/// `sofel_rasterizer_close`.
///
/// # Safety
///
/// As [`sofel_rasterizer_move_to`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_close(rasterizer: *mut c_void) {
    // SAFETY: the caller's conditions.
    if let Some(r) = unsafe { raster_pen(rasterizer) } {
        r.close();
    }
}

fn to_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

fn pixel_box(c: [i32; 4], b: Option<&mut [i32; 4]>, shift: Option<&mut [i32; 2]>) -> Status {
    let (b, shift) = (some(b)?, some(shift)?);
    let pb = sofel_raster::pixel_box(c[0], c[1], c[2], c[3]);
    *b = [pb.left, pb.top, to_i32(pb.width), to_i32(pb.rows)];
    let s = pb.shift();
    *shift = [s.x, s.y];
    Ok(())
}

/// `sofel_pixel_box`.
///
/// # Safety
///
/// `box_` is NULL or writable for 4 integers, `shift` for 2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_pixel_box(
    x_min: i32,
    y_min: i32,
    x_max: i32,
    y_max: i32,
    box_: *mut i32,
    shift: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, shift) = unsafe { (ints::<4>(box_), ints::<2>(shift)) };
    status(pixel_box([x_min, y_min, x_max, y_max], b, shift))
}

/// Where a bitmap goes: the caller's array, the length and the box.
struct BitmapOut<'a> {
    pixels: Option<&'a mut [u8]>,
    len: Option<&'a mut usize>,
    b: Option<&'a mut [i32; 4]>,
}

impl BitmapOut<'_> {
    fn check(&self) -> Status {
        match (&self.pixels, &self.len, &self.b) {
            (Some(_), Some(_), Some(_)) => Ok(()),
            _ => Err(ERROR_NULL),
        }
    }

    /// Writes the length and box, then the pixels if they fit.
    fn put(self, bitmap: Result<Bitmap<'_>, i32>) -> Status {
        let (pixels, len, b) = (some(self.pixels)?, some(self.len)?, some(self.b)?);
        let bitmap = bitmap?;
        *len = bitmap.pixels.len();
        *b = [
            bitmap.left,
            bitmap.top,
            to_i32(bitmap.width),
            to_i32(bitmap.rows),
        ];
        let dest = pixels
            .get_mut(..bitmap.pixels.len())
            .ok_or(ERROR_BUFFER_TOO_SMALL)?;
        dest.copy_from_slice(bitmap.pixels);
        Ok(())
    }
}

fn options(fill: u32, overlap: u32, x_ppem: u16, y_ppem: u16) -> Result<RenderOptions, i32> {
    let fill = match fill {
        0 => FillRule::NonZero,
        1 => FillRule::EvenOdd,
        _ => return Err(ERROR_INVALID_ARGUMENT),
    };
    Ok(RenderOptions {
        fill,
        overlap: overlap != 0,
        ppem: (x_ppem != 0 || y_ppem != 0).then_some((x_ppem, y_ppem)),
    })
}

fn rasterizer_render(
    rasterizer: Option<&mut SofelRasterizer>,
    pixel_box: Option<PixelBox>,
    options: Result<RenderOptions, i32>,
    out: BitmapOut<'_>,
) -> Status {
    let r = some(rasterizer)?;
    out.check()?;
    let options = options?;
    let bitmap = match pixel_box {
        None => r.raster.render(options),
        Some(b) => r.raster.render_placed(b, options),
    };
    out.put(bitmap.map_err(raster_error))
}

/// `sofel_rasterizer_render`.
///
/// # Safety
///
/// `rasterizer` is NULL or a live rasterizer no other thread uses;
/// `pixels` is NULL or writable for `capacity` bytes; `len` is NULL or
/// writable; `box_` is NULL or writable for 4 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_render(
    rasterizer: *mut SofelRasterizer,
    fill: u32,
    overlap: u32,
    x_ppem: u16,
    y_ppem: u16,
    pixels: *mut u8,
    capacity: usize,
    len: *mut usize,
    box_: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (rasterizer, pixels, len, b) = unsafe {
        (
            w(rasterizer),
            out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
        )
    };
    let out = BitmapOut { pixels, len, b };
    status(rasterizer_render(
        rasterizer,
        None,
        options(fill, overlap, x_ppem, y_ppem),
        out,
    ))
}

/// `sofel_rasterizer_render_placed`.
///
/// # Safety
///
/// As [`sofel_rasterizer_render`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_render_placed(
    rasterizer: *mut SofelRasterizer,
    left: i32,
    top: i32,
    width: u32,
    rows: u32,
    fill: u32,
    overlap: u32,
    x_ppem: u16,
    y_ppem: u16,
    pixels: *mut u8,
    capacity: usize,
    len: *mut usize,
    box_: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (rasterizer, pixels, len, b) = unsafe {
        (
            w(rasterizer),
            out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
        )
    };
    let out = BitmapOut { pixels, len, b };
    let pb = PixelBox {
        left,
        top,
        width,
        rows,
    };
    status(rasterizer_render(
        rasterizer,
        Some(pb),
        options(fill, overlap, x_ppem, y_ppem),
        out,
    ))
}

fn render_glyph(
    font: Option<&Font<'_>>,
    glyph: u32,
    ppem: u16,
    scratch: Option<&mut SofelOutlineScratch>,
    rasterizer: Option<&mut SofelRasterizer>,
    out: BitmapOut<'_>,
) -> Status {
    let (font, scratch, r) = (some(font)?, some(scratch)?, some(rasterizer)?);
    out.check()?;
    let bitmap = sofel_cli::glyph::render(
        font,
        GlyphId(glyph),
        ppem,
        &mut scratch.scratch,
        &mut r.raster,
    )
    .map_err(|e| match e {
        RenderError::Outline(e) => outline_error(e),
        RenderError::Raster(e) => raster_error(e),
    });
    out.put(bitmap)
}

/// `sofel_render_glyph`.
///
/// # Safety
///
/// `font` is NULL or a live font; `scratch` and `rasterizer` are NULL or
/// live work areas no other thread uses; the outputs as in
/// [`sofel_rasterizer_render`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_render_glyph(
    font: *const SofelFont,
    glyph: u32,
    ppem: u16,
    scratch: *mut SofelOutlineScratch,
    rasterizer: *mut SofelRasterizer,
    pixels: *mut u8,
    capacity: usize,
    len: *mut usize,
    box_: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, scratch, rasterizer, pixels, len, b) = unsafe {
        (
            font_ref(font),
            w(scratch),
            w(rasterizer),
            out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
        )
    };
    let out = BitmapOut { pixels, len, b };
    status(render_glyph(font, glyph, ppem, scratch, rasterizer, out))
}

// ---------------------------------------------------------- validation

fn validate_data(data: Option<&[u8]>, report: DiagnosticFunc, user: *mut c_void) -> Status {
    let (data, report) = (some(data)?, some(report)?);
    let emit = |font: Option<u32>, diagnostic: Diagnostic| {
        let d = SofelDiagnostic { font, diagnostic };
        report(user, &d);
    };
    match FontFile::new(data).map_err(|_| ERROR_INVALID_FONT)? {
        FontFile::Sfnt(s) => validate::sfnt(&s, &mut |d| emit(None, d)),
        FontFile::Collection(c) => {
            validate::collection(&c, &mut |d| emit(None, d));
            (0..c.num_fonts())
                .filter_map(|i| c.font(i).ok().map(|s| (i, s)))
                .for_each(|(i, s)| validate::sfnt(&s, &mut |d| emit(Some(i), d)));
        }
    }
    Ok(())
}

/// `sofel_validate`.
///
/// # Safety
///
/// `data` is NULL or valid for reads of `len` bytes; `report` is NULL or
/// a function of the declared type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_validate(
    data: *const u8,
    len: usize,
    report: DiagnosticFunc,
    user: *mut c_void,
) -> i32 {
    // SAFETY: the caller's conditions.
    let data = unsafe { bytes(data, len) };
    status(validate_data(data, report, user))
}

fn diag<T>(
    d: Option<&SofelDiagnostic>,
    out: Option<&mut T>,
    f: impl FnOnce(&SofelDiagnostic) -> Option<T>,
) -> Status {
    let (d, out) = (some(d)?, some(out)?);
    *out = f(d).ok_or(ERROR_NOT_FOUND)?;
    Ok(())
}

/// `sofel_diagnostic_font`.
///
/// # Safety
///
/// `diagnostic` is NULL or the one given to the running callback;
/// `index` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_font(
    diagnostic: *const SofelDiagnostic,
    index: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (d, index) = unsafe { (r(diagnostic), w(index)) };
    status(diag(d, index, |d| d.font))
}

/// `sofel_diagnostic_table`.
///
/// # Safety
///
/// As [`sofel_diagnostic_font`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_table(
    diagnostic: *const SofelDiagnostic,
    tag: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (d, tag) = unsafe { (r(diagnostic), w(tag)) };
    status(diag(d, tag, |d| Some(d.diagnostic.table.to_u32())))
}

/// `sofel_diagnostic_offset`.
///
/// # Safety
///
/// As [`sofel_diagnostic_font`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_offset(
    diagnostic: *const SofelDiagnostic,
    offset: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (d, offset) = unsafe { (r(diagnostic), w(offset)) };
    status(diag(d, offset, |d| Some(d.diagnostic.offset)))
}

/// `sofel_diagnostic_severity`.
///
/// # Safety
///
/// As [`sofel_diagnostic_font`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_severity(
    diagnostic: *const SofelDiagnostic,
    severity: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (d, severity) = unsafe { (r(diagnostic), w(severity)) };
    status(diag(d, severity, |d| {
        Some(match d.diagnostic.kind.severity() {
            Severity::Error => 0,
            Severity::Warning => 1,
        })
    }))
}

/// Writes `s` and a NUL to `buffer` if they fit, and its length to `len`.
fn put_str(s: &str, buffer: Option<&mut [u8]>, len: Option<&mut usize>) -> Status {
    let (buffer, len) = (some(buffer)?, some(len)?);
    *len = s.len();
    let (text, nul) = buffer
        .get_mut(..=s.len())
        .and_then(|b| b.split_last_mut())
        .map(|(nul, text)| (text, nul))
        .ok_or(ERROR_BUFFER_TOO_SMALL)?;
    text.copy_from_slice(s.as_bytes());
    *nul = 0;
    Ok(())
}

fn diag_str(
    d: Option<&SofelDiagnostic>,
    buffer: Option<&mut [u8]>,
    len: Option<&mut usize>,
    f: impl FnOnce(&SofelDiagnostic) -> String,
) -> Status {
    let d = some(d)?;
    put_str(&f(d), buffer, len)
}

/// `sofel_diagnostic_kind`.
///
/// # Safety
///
/// `diagnostic` as in [`sofel_diagnostic_font`]; `buffer` is NULL or
/// writable for `capacity` bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_kind(
    diagnostic: *const SofelDiagnostic,
    buffer: *mut c_char,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions; `c_char` and `u8` have the same layout.
    let (d, buffer, len) = unsafe {
        (
            r(diagnostic),
            out_array(buffer.cast::<u8>(), capacity),
            w(len),
        )
    };
    status(diag_str(d, buffer, len, |d| {
        String::from(d.diagnostic.kind.id())
    }))
}

/// `sofel_diagnostic_message`.
///
/// # Safety
///
/// As [`sofel_diagnostic_kind`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_message(
    diagnostic: *const SofelDiagnostic,
    buffer: *mut c_char,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions; `c_char` and `u8` have the same layout.
    let (d, buffer, len) = unsafe {
        (
            r(diagnostic),
            out_array(buffer.cast::<u8>(), capacity),
            w(len),
        )
    };
    status(diag_str(d, buffer, len, |d| {
        let mut s = String::new();
        // Writing to a String does not fail.
        let _ = write!(s, "{}", d.diagnostic.kind);
        s
    }))
}

fn details(d: Option<&SofelDiagnostic>, f: DetailFunc, user: *mut c_void) -> Status {
    let (d, f) = (some(d)?, some(f)?);
    d.diagnostic.kind.details(&mut |name, v| {
        let (kind, value) = match v {
            DetailValue::Uint(n) => (0, i64::from(n)),
            DetailValue::Int(n) => (1, i64::from(n)),
            DetailValue::Tag(t) => (2, i64::from(t.to_u32())),
        };
        // Names are identifiers without NUL, so this always succeeds.
        if let Ok(name) = CString::new(name) {
            f(user, name.as_ptr(), kind, value);
        }
    });
    Ok(())
}

/// `sofel_diagnostic_details`.
///
/// # Safety
///
/// `diagnostic` as in [`sofel_diagnostic_font`]; `detail` is NULL or a
/// function of the declared type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_diagnostic_details(
    diagnostic: *const SofelDiagnostic,
    detail: DetailFunc,
    user: *mut c_void,
) -> i32 {
    // SAFETY: the caller's conditions.
    let d = unsafe { r(diagnostic) };
    status(details(d, detail, user))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn version_follows_the_crate() {
        assert_eq!(
            [VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH].map(|v| v.to_string()),
            [
                env!("CARGO_PKG_VERSION_MAJOR"),
                env!("CARGO_PKG_VERSION_MINOR"),
                env!("CARGO_PKG_VERSION_PATCH"),
            ]
        );
    }

    #[test]
    fn strings_follow_snprintf() {
        let mut buf = [7u8; 4];
        let mut len = 0;
        assert_eq!(
            put_str("abcd", Some(&mut buf), Some(&mut len)),
            Err(ERROR_BUFFER_TOO_SMALL)
        );
        assert_eq!((len, buf), (4, [7; 4]));
        assert_eq!(put_str("abc", Some(&mut buf), Some(&mut len)), Ok(()));
        assert_eq!((len, buf), (3, *b"abc\0"));
    }
}
