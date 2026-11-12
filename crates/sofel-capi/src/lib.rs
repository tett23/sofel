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

mod color;
mod gpu;
mod layout_query;
mod shape;
mod subset;
mod variation;
mod vector;

pub use variation::*;

use core::ffi::{c_char, c_void};
use core::fmt::Write as _;
use std::ffi::CString;

use sofel_cli::glyph::RenderError;
use sofel_font::read::cff::{CharsetIndex, Version};
use sofel_font::read::os2::Os2;
use sofel_font::read::sfnt::FontFile;
use sofel_font::read::{DetailValue, Diagnostic, GlyphId, Severity, Tag, validate};
use sofel_font::{Cff, CidError, Direction, Font, FontError, GlyphExtents, HbScale, MetricsTag};
use sofel_geom::{Fixed, Matrix, Pen, Point};
use sofel_outline::{OutlineError, OutlineInfo, Outlines, Scale, Scratch};
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
/// `SOFEL_ERROR_INVALID_CFF`.
pub const ERROR_INVALID_CFF: i32 = -13;
/// `SOFEL_ERROR_BUFFER_CONTENT`.
pub const ERROR_BUFFER_CONTENT: i32 = -14;

/// `sofel_font_t`: a font over data the caller keeps alive, with
/// HarfBuzz's CFF and CFF2 accelerators for its glyph boxes (ADR 0021)
/// and the index of the CFF charset (ADR 0025), built once at creation
/// (a broken CFF only leaves them out).
pub struct SofelFont {
    font: Font<'static>,
    /// The outline format, the opened CFF and HarfBuzz's glyph boxes
    /// (ADR 0021); a broken CFF does not fail the font.
    outlines: Outlines<'static, 'static, 'static>,
    /// FreeType's CFF view, when FreeType can open the face as CFF.
    cff: Option<Cff<'static>>,
    /// The index of the CFF charset (ADR 0025), over `starts`; `None`
    /// once the font is being dropped.
    charset: Option<CharsetIndex<'static>>,
    /// The array `charset` borrows, from `Box::into_raw`; freed in
    /// `Drop` after `charset` is cleared.
    starts: *mut [u32],
}

impl Drop for SofelFont {
    fn drop(&mut self) {
        self.charset = None;
        // SAFETY: `starts` came from Box::into_raw in font_create, is
        // freed only here, and nothing borrows it any more.
        drop(unsafe { Box::from_raw(self.starts) });
    }
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
        ERROR_INVALID_CFF => c"invalid CFF table or no CFF",
        ERROR_BUFFER_CONTENT => c"the buffer holds glyphs or characters where the other is needed",
        subset::ERROR_NO_GLYPHS => c"the font has no glyphs",
        subset::ERROR_INVALID_INPUT => c"invalid subsetting input",
        subset::ERROR_MALFORMED_TABLE => c"a table cannot be read",
        subset::ERROR_UNSUPPORTED => c"not supported yet",
        subset::ERROR_OFFSET_OVERFLOW => c"offset overflow",
        subset::ERROR_LIMIT => c"a limit of HarfBuzz's was reached",
        subset::ERROR_SUBSET_FAILED => c"subsetting a table failed",
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
        OutlineError::UnsupportedFormat => ERROR_NO_GLYPH_DATA,
        OutlineError::InvalidCff => ERROR_INVALID_CFF,
        OutlineError::InvalidCharString => ERROR_INVALID_OUTLINE,
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
    let outlines = Outlines::new(font.default_instance());
    let cff = outlines.cff().copied();
    let ranges = cff.and_then(|c| c.charset()).map_or(0, |c| c.num_ranges());
    let starts = Box::into_raw(vec![0u32; ranges].into_boxed_slice());
    // SAFETY: `starts` is a fresh allocation; the index borrowing it lives
    // in the font, which frees the array only after dropping the index.
    let array: &'static mut [u32] = unsafe { &mut *starts };
    let charset = cff
        .and_then(|c| c.charset())
        .and_then(|c| c.index_into(array).ok());
    Ok(SofelFont {
        font,
        outlines,
        cff,
        charset,
        starts,
    })
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
    // sofel_font_destroy and every location made from the font has been
    // destroyed; `error` is NULL or writable.
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

/// The `sofel_font_t` of a pointer, for the queries that need its glyph
/// boxes.
///
/// # Safety
///
/// `font` is NULL or a live font.
unsafe fn sofel_ref<'a>(font: *const SofelFont) -> Option<&'a SofelFont> {
    // SAFETY: the caller's conditions.
    unsafe { r(font) }
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

/// `sofel_font_nominal_glyph_raw`.
///
/// # Safety
///
/// `font` is NULL or a live font; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_nominal_glyph_raw(
    font: *const SofelFont,
    codepoint: u32,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, glyph) = unsafe { (font_ref(font), w(glyph)) };
    status(find(font, glyph, |f| {
        f.nominal_glyph_raw(codepoint).map(|g| g.0)
    }))
}

/// `sofel_font_variation_glyph_raw`.
///
/// # Safety
///
/// `font` is NULL or a live font; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_variation_glyph_raw(
    font: *const SofelFont,
    codepoint: u32,
    selector: u32,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, glyph) = unsafe { (font_ref(font), w(glyph)) };
    status(find(font, glyph, |f| {
        f.variation_glyph_raw(codepoint, selector).map(|g| g.0)
    }))
}

/// `sofel_font_h_advance`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_h_advance(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    advance: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, advance) = unsafe { (font_ref(font), r(location), w(advance)) };
    status((|| {
        let (font, advance) = (some(font)?, some(advance)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *advance = font.instance(location).advance(GlyphId(glyph));
        Ok(())
    })())
}

/// `sofel_font_ft_left_side_bearing`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `bearing` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_left_side_bearing(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    bearing: *mut i16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, bearing) = unsafe { (font_ref(font), r(location), w(bearing)) };
    status((|| {
        let (font, bearing) = (some(font)?, some(bearing)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *bearing = font.instance(location).ft_left_side_bearing(GlyphId(glyph));
        Ok(())
    })())
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

// ------------------------------------------------------------- metrics

/// `sofel_scale_t`: HarfBuzz's scale of a font.
pub struct SofelScale {
    scale: HbScale,
    x_ppem: u32,
    y_ppem: u32,
}

/// The value of an `SOFEL_OS2_*` field, `None` past the table's version.
fn os2_field(o: &Os2<'_>, field: i32) -> Option<i64> {
    let i = |v: i16| Some(i64::from(v));
    let u = |v: u16| Some(i64::from(v));
    let r = o.ul_unicode_range();
    let c = o.ul_code_page_range();
    match field {
        0 => u(o.version()),
        1 => i(o.x_avg_char_width()),
        2 => u(o.us_weight_class()),
        3 => u(o.us_width_class()),
        4 => u(o.fs_type()),
        5 => i(o.y_subscript_x_size()),
        6 => i(o.y_subscript_y_size()),
        7 => i(o.y_subscript_x_offset()),
        8 => i(o.y_subscript_y_offset()),
        9 => i(o.y_superscript_x_size()),
        10 => i(o.y_superscript_y_size()),
        11 => i(o.y_superscript_x_offset()),
        12 => i(o.y_superscript_y_offset()),
        13 => i(o.y_strikeout_size()),
        14 => i(o.y_strikeout_position()),
        15 => i(o.s_family_class()),
        16..=19 => r
            .get(usize::try_from(field.wrapping_sub(16)).ok()?)
            .map(|&v| i64::from(v)),
        20 => Some(i64::from(o.ach_vend_id().to_u32())),
        21 => u(o.fs_selection()),
        22 => u(o.us_first_char_index()),
        23 => u(o.us_last_char_index()),
        24 => i(o.s_typo_ascender()),
        25 => i(o.s_typo_descender()),
        26 => i(o.s_typo_line_gap()),
        27 => u(o.us_win_ascent()),
        28 => u(o.us_win_descent()),
        29 | 30 => c?
            .get(usize::try_from(field.wrapping_sub(29)).ok()?)
            .map(|&v| i64::from(v)),
        31 => o.sx_height().map(i64::from),
        32 => o.s_cap_height().map(i64::from),
        33 => o.us_default_char().map(i64::from),
        34 => o.us_break_char().map(i64::from),
        35 => o.us_max_context().map(i64::from),
        36 => o.us_lower_optical_point_size().map(i64::from),
        37 => o.us_upper_optical_point_size().map(i64::from),
        38..=47 => o
            .panose()
            .get(usize::try_from(field.wrapping_sub(38)).ok()?)
            .map(|&v| i64::from(v)),
        _ => None,
    }
}

fn font_os2(font: Option<&Font<'static>>, field: i32, value: Option<&mut i64>) -> Status {
    let (font, value) = (some(font)?, some(value)?);
    if !(0..=47).contains(&field) {
        return Err(ERROR_INVALID_ARGUMENT);
    }
    let os2 = font.os2().ok_or(ERROR_NOT_FOUND)?;
    *value = os2_field(&os2, field).ok_or(ERROR_NOT_FOUND)?;
    Ok(())
}

/// `sofel_font_os2_field`.
///
/// # Safety
///
/// `font` is NULL or a live font; `value` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_os2_field(
    font: *const SofelFont,
    field: i32,
    value: *mut i64,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, value) = unsafe { (font_ref(font), w(value)) };
    status(font_os2(font, field, value))
}

/// `sofel_font_fs_type`.
///
/// # Safety
///
/// `font` is NULL or a live font; `fs_type` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_fs_type(font: *const SofelFont, fs_type: *mut u16) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, fs_type) = unsafe { (font_ref(font), w(fs_type)) };
    status(get(font, fs_type, |f| f.fs_type()))
}

/// `sofel_font_hb_units_per_em`.
///
/// # Safety
///
/// `font` is NULL or a live font; `upem` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_hb_units_per_em(font: *const SofelFont, upem: *mut u16) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, upem) = unsafe { (font_ref(font), w(upem)) };
    status(get(font, upem, |f| f.hb_units_per_em()))
}

fn extents3(e: sofel_font::FontExtents) -> [i32; 3] {
    [e.ascender, e.descender, e.line_gap]
}

fn extents4(e: GlyphExtents) -> [i32; 4] {
    [e.x_bearing, e.y_bearing, e.width, e.height]
}

/// The box of a glyph at `scale` for the queries that need one, as
/// HarfBuzz gives it: sbix, CBDT and COLR, then glyf, CFF2 and CFF
/// (ADR 0023). The work memory is made for each glyph.
fn outlines_at<'a>(
    font: &SofelFont,
    location: Option<&'a SofelLocation>,
) -> Outlines<'static, 'a, 'static> {
    match location {
        Some(loc) => font.outlines.with_location(&loc.location),
        None => font.outlines.clone(),
    }
}

fn glyph_box<'f>(
    font: &'f SofelFont,
    location: Option<&'f SofelLocation>,
    scale: &'f HbScale,
) -> impl Fn(GlyphId) -> Option<GlyphExtents> + 'f {
    let outlines = outlines_at(font, location);
    move |g| {
        sofel_color::scaled_glyph_extents(
            &outlines,
            g.0,
            scale,
            &mut sofel_color::Scratch::default(),
        )
    }
}

/// The scale of a `sofel_scale_t` pointer, or the default scale of the
/// font for NULL.
fn scale_of(scale: Option<&SofelScale>, font: &Font<'_>) -> HbScale {
    scale.map_or_else(|| HbScale::unscaled(font), |s| s.scale)
}

/// `sofel_font_h_extents` and `sofel_scale_h_extents`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `extents` is NULL or writable for 3
/// integers.
unsafe fn h_or_v_extents(
    scale: Option<&SofelScale>,
    font: *const SofelFont,
    location: Option<&SofelLocation>,
    extents: *mut i32,
    vertical: bool,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, extents) = unsafe { (font_ref(font), ints::<3>(extents)) };
    status(get(font, extents, |f| {
        let s = scale_of(scale, f);
        let location = match location {
            Some(value) => &value.location,
            None => sofel_font::var::Location::default_ref(),
        };
        let instance = f.instance(location);
        extents3(if vertical {
            instance.scaled_v_extents(&s)
        } else {
            instance.scaled_h_extents(&s)
        })
    }))
}

/// `sofel_font_h_extents`.
///
/// # Safety
///
/// `font` is NULL or a live font; `extents` is NULL or writable for 3
/// integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_h_extents(
    font: *const SofelFont,
    location: *const SofelLocation,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, extents) = unsafe { (font_ref(font), r(location), ints::<3>(extents)) };
    status((|| {
        let (font, extents) = (some(font)?, some(extents)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *extents = extents3(font.instance(location).h_extents());
        Ok(())
    })())
}

/// `sofel_font_v_extents`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `extents` is NULL or writable for 3
/// integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_v_extents(
    font: *const SofelFont,
    location: *const SofelLocation,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, extents) = unsafe { (font_ref(font), r(location), ints::<3>(extents)) };
    status((|| {
        let (font, extents) = (some(font)?, some(extents)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *extents = extents3(font.instance(location).v_extents());
        Ok(())
    })())
}

/// `sofel_font_v_advance`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_v_advance(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    advance: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, advance) = unsafe { (font_ref(font), r(location), w(advance)) };
    status((|| {
        let (font, advance) = (some(font)?, some(advance)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *advance = font.instance(location).v_advance(GlyphId(glyph));
        Ok(())
    })())
}

/// `sofel_font_v_origin`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `origin` is NULL or writable for 2
/// integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_v_origin(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    origin: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, origin) = unsafe { (sofel_ref(font), r(location), ints::<2>(origin)) };
    status(font.ok_or(ERROR_NULL).and_then(|sf| {
        get(Some(&sf.font), origin, |f| {
            let location_ref =
                location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
            let s = HbScale::unscaled(f);
            let (x, y) = f
                .instance(location_ref)
                .v_origin(GlyphId(glyph), glyph_box(sf, location, &s));
            [x, y]
        })
    }))
}

/// `sofel_font_glyph_extents`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `extents` is NULL or writable for 4
/// integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_glyph_extents(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, extents) = unsafe { (sofel_ref(font), r(location), ints::<4>(extents)) };
    status(font.ok_or(ERROR_NULL).and_then(|sf| {
        find(Some(&sf.font), extents, |_| {
            sofel_color::glyph_extents(
                &outlines_at(sf, location),
                glyph,
                &mut sofel_color::Scratch::default(),
            )
            .map(extents4)
        })
    }))
}

fn metrics_position(
    scale: Option<&SofelScale>,
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    tag: u32,
    fallback: bool,
    position: Option<&mut i32>,
) -> Status {
    let (sf, position) = (some(font)?, some(position)?);
    let font = &sf.font;
    let tag = MetricsTag::from_tag(Tag::from_u32(tag)).ok_or(ERROR_INVALID_ARGUMENT)?;
    let s = scale_of(scale, font);
    let location_ref = match location {
        Some(value) => &value.location,
        None => sofel_font::var::Location::default_ref(),
    };
    let instance = font.instance(location_ref);
    *position = if fallback {
        if scale.is_some() {
            instance.scaled_metrics_position_with_fallback(&s, tag, glyph_box(sf, location, &s))
        } else {
            instance.metrics_position_with_fallback(tag, glyph_box(sf, location, &s))
        }
    } else {
        if scale.is_some() {
            instance
                .scaled_metrics_position(&s, tag)
                .ok_or(ERROR_NOT_FOUND)?
        } else {
            instance.metrics_position(tag).ok_or(ERROR_NOT_FOUND)?
        }
    };
    Ok(())
}

/// `sofel_font_metrics_position`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `position` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_metrics_position(
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    position: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, position) = unsafe { (sofel_ref(font), r(location), w(position)) };
    status(metrics_position(None, font, location, tag, false, position))
}

/// `sofel_font_metrics_position_with_fallback`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `position` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_metrics_position_with_fallback(
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    position: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, position) = unsafe { (sofel_ref(font), r(location), w(position)) };
    status(metrics_position(None, font, location, tag, true, position))
}

/// `sofel_scale_create`.
///
/// # Safety
///
/// `font` is NULL or a live font; `error` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_create(
    font: *const SofelFont,
    x_scale: i32,
    y_scale: i32,
    error: *mut i32,
) -> *mut SofelScale {
    // SAFETY: the caller's conditions are those of the delegated constructor.
    unsafe { sofel_scale_create_with_ppem(font, x_scale, y_scale, 0, 0, error) }
}

/// `sofel_scale_create_with_ppem`.
///
/// # Safety
///
/// `font` is NULL or a live font; `error` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_create_with_ppem(
    font: *const SofelFont,
    x_scale: i32,
    y_scale: i32,
    x_ppem: u32,
    y_ppem: u32,
    error: *mut i32,
) -> *mut SofelScale {
    // SAFETY: the caller's conditions.
    let (font, error) = unsafe { (font_ref(font), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    match font {
        None => {
            *error = ERROR_NULL;
            core::ptr::null_mut()
        }
        Some(f) => {
            *error = OK;
            Box::into_raw(Box::new(SofelScale {
                scale: HbScale::for_font(f, x_scale, y_scale),
                x_ppem,
                y_ppem,
            }))
        }
    }
}

/// `sofel_scale_create_with_ptem`.
///
/// # Safety
///
/// `font` is NULL or a live font; `error` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_create_with_ptem(
    font: *const SofelFont,
    x_scale: i32,
    y_scale: i32,
    x_ppem: u32,
    y_ppem: u32,
    ptem: f32,
    error: *mut i32,
) -> *mut SofelScale {
    // SAFETY: the caller's conditions are those of the delegated constructor.
    let scale =
        unsafe { sofel_scale_create_with_ppem(font, x_scale, y_scale, x_ppem, y_ppem, error) };
    if !scale.is_null() {
        // SAFETY: the constructor returned a uniquely owned scale.
        unsafe { (*scale).scale = (*scale).scale.with_ptem(ptem) };
    }
    scale
}

/// `sofel_scale_destroy`.
///
/// # Safety
///
/// `scale` is NULL or from `sofel_scale_create`, not yet destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_destroy(scale: *mut SofelScale) {
    if !scale.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(scale) });
    }
}

/// `sofel_scale_em_scale_x` and `_y`.
fn em_scale(scale: Option<&SofelScale>, v: i32, out: Option<&mut i32>, y: bool) -> Status {
    let (scale, out) = (some(scale)?, some(out)?);
    *out = if y {
        scale.scale.em_scale_y(v)
    } else {
        scale.scale.em_scale_x(v)
    };
    Ok(())
}

/// `sofel_scale_em_scale_x`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_em_scale_x(
    scale: *const SofelScale,
    v: i32,
    out: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, out) = unsafe { (r(scale), w(out)) };
    status(em_scale(scale, v, out, false))
}

/// `sofel_scale_em_scale_y`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_em_scale_y(
    scale: *const SofelScale,
    v: i32,
    out: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, out) = unsafe { (r(scale), w(out)) };
    status(em_scale(scale, v, out, true))
}

/// `sofel_scale_glyph_extents`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `extents` is NULL or readable and
/// writable for 4 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_glyph_extents(
    scale: *const SofelScale,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, extents) = unsafe { (r(scale), ints::<4>(extents)) };
    status((|| {
        let (scale, e) = (some(scale)?, some(extents)?);
        let [x_bearing, y_bearing, width, height] = *e;
        *e = extents4(scale.scale.scale_glyph_extents(GlyphExtents {
            x_bearing,
            y_bearing,
            width,
            height,
        }));
        Ok(())
    })())
}

/// `sofel_scale_h_extents`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `extents` is NULL or writable for 3 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_h_extents(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let Some(scale) = (unsafe { r(scale) }) else {
        return ERROR_NULL;
    };
    // SAFETY: the caller's conditions.
    unsafe { h_or_v_extents(Some(scale), font, r(location), extents, false) }
}

/// `sofel_scale_v_extents`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `extents` is NULL or writable for 3 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_v_extents(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let Some(scale) = (unsafe { r(scale) }) else {
        return ERROR_NULL;
    };
    // SAFETY: the caller's conditions.
    unsafe { h_or_v_extents(Some(scale), font, r(location), extents, true) }
}

fn scaled<T>(
    scale: Option<&SofelScale>,
    font: Option<&SofelFont>,
    out: Option<&mut T>,
    f: impl FnOnce(&HbScale, &SofelFont) -> Option<T>,
) -> Status {
    let (scale, font, out) = (some(scale)?, some(font)?, some(out)?);
    *out = f(&scale.scale, font).ok_or(ERROR_NOT_FOUND)?;
    Ok(())
}

/// `sofel_scale_h_advance`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_h_advance(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    advance: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, advance) =
        unsafe { (r(scale), sofel_ref(font), r(location), w(advance)) };
    status(scaled(scale, font, advance, |s, f| {
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        #[allow(clippy::cast_possible_wrap)]
        // HarfBuzz casts the unsigned advance to hb_position_t.
        let signed = f.font.instance(location).advance(GlyphId(glyph)) as i32;
        Some(s.em_scale_x(signed))
    }))
}

/// `sofel_scale_v_advance`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_v_advance(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    advance: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, advance) =
        unsafe { (r(scale), sofel_ref(font), r(location), w(advance)) };
    status(scaled(scale, font, advance, |s, f| {
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        Some(
            f.font
                .instance(location)
                .scaled_v_advance(s, GlyphId(glyph)),
        )
    }))
}

/// `sofel_scale_v_origin`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `origin` is NULL or writable for 2 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_v_origin(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    origin: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, origin) =
        unsafe { (r(scale), sofel_ref(font), r(location), ints::<2>(origin)) };
    status(scaled(scale, font, origin, |s, f| {
        let location_ref =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        let (x, y) = f.font.instance(location_ref).scaled_v_origin(
            s,
            GlyphId(glyph),
            glyph_box(f, location, s),
        );
        Some([x, y])
    }))
}

/// `sofel_scale_glyph_extents_of`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `extents` is NULL or writable for 4 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_glyph_extents_of(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    extents: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, extents) =
        unsafe { (r(scale), sofel_ref(font), r(location), ints::<4>(extents)) };
    status(scaled(scale, font, extents, |s, f| {
        sofel_color::scaled_glyph_extents(
            &outlines_at(f, location),
            glyph,
            s,
            &mut sofel_color::Scratch::default(),
        )
        .map(extents4)
    }))
}

/// `sofel_scale_metrics_position`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `position` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_metrics_position(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    position: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, position) =
        unsafe { (r(scale), sofel_ref(font), r(location), w(position)) };
    status(
        some(scale).and_then(|s| metrics_position(Some(s), font, location, tag, false, position)),
    )
}

/// `sofel_scale_metrics_position_with_fallback`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `position` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_metrics_position_with_fallback(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    position: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, position) =
        unsafe { (r(scale), sofel_ref(font), r(location), w(position)) };
    status(some(scale).and_then(|s| metrics_position(Some(s), font, location, tag, true, position)))
}

// ----------------------------------------------------------- baselines

/// `SOFEL_DIRECTION_*` to [`Direction`].
fn direction(d: i32) -> Result<Direction, i32> {
    match d {
        0 => Ok(Direction::Horizontal),
        1 => Ok(Direction::Vertical),
        _ => Err(ERROR_INVALID_ARGUMENT),
    }
}

fn baseline(
    scale: Option<&SofelScale>,
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    tag: u32,
    dir: i32,
    script: u32,
    fallback: bool,
    coord: Option<&mut i32>,
) -> Status {
    let (sf, coord) = (some(font)?, some(coord)?);
    let font = &sf.font;
    let (tag, dir, script) = (Tag::from_u32(tag), direction(dir)?, Tag::from_u32(script));
    let s = scale_of(scale, font);
    let location_ref = match location {
        Some(v) => &v.location,
        None => sofel_font::var::Location::default_ref(),
    };
    let instance = font.instance(location_ref);
    let (x_ppem, y_ppem) = scale.map_or((0, 0), |scale| (scale.x_ppem, scale.y_ppem));
    let query = instance.layout_queries(s, x_ppem, y_ppem);
    *coord = if fallback {
        query.baseline_with_fallback(tag, dir, script, glyph_box(sf, location, &s))
    } else {
        query.baseline(tag, dir, script).ok_or(ERROR_NOT_FOUND)?
    };
    Ok(())
}

/// `sofel_font_baseline`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `coord` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_baseline(
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    direction: i32,
    script: u32,
    coord: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, coord) = unsafe { (sofel_ref(font), r(location), w(coord)) };
    status(baseline(
        None, font, location, tag, direction, script, false, coord,
    ))
}

/// `sofel_font_baseline_with_fallback`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `coord` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_baseline_with_fallback(
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    direction: i32,
    script: u32,
    coord: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, coord) = unsafe { (sofel_ref(font), r(location), w(coord)) };
    status(baseline(
        None, font, location, tag, direction, script, true, coord,
    ))
}

/// `sofel_scale_baseline`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `coord` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_baseline(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    direction: i32,
    script: u32,
    coord: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, coord) =
        unsafe { (r(scale), sofel_ref(font), r(location), w(coord)) };
    status(some(scale).and_then(|s| {
        baseline(
            Some(s),
            font,
            location,
            tag,
            direction,
            script,
            false,
            coord,
        )
    }))
}

/// `sofel_scale_baseline_with_fallback`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `coord` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_baseline_with_fallback(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    tag: u32,
    direction: i32,
    script: u32,
    coord: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, coord) =
        unsafe { (r(scale), sofel_ref(font), r(location), w(coord)) };
    status(
        some(scale)
            .and_then(|s| baseline(Some(s), font, location, tag, direction, script, true, coord)),
    )
}

fn layout_font_extents(
    scale: Option<&SofelScale>,
    font: Option<&Font<'static>>,
    location: Option<&SofelLocation>,
    dir: i32,
    script: u32,
    language: u32,
    extents: Option<&mut [i32; 3]>,
    found: Option<&mut i32>,
) -> Status {
    let (font, extents, found) = (some(font)?, some(extents)?, some(found)?);
    let dir = direction(dir)?;
    let location = match location {
        Some(v) => &v.location,
        None => sofel_font::var::Location::default_ref(),
    };
    let (x_ppem, y_ppem) = scale.map_or((0, 0), |scale| (scale.x_ppem, scale.y_ppem));
    let query = font
        .instance(location)
        .layout_queries(scale_of(scale, font), x_ppem, y_ppem);
    let (e, f) = query.layout_font_extents(dir, Tag::from_u32(script), Tag::from_u32(language));
    *extents = extents3(e);
    *found = i32::from(f);
    Ok(())
}

/// `sofel_font_layout_font_extents`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `extents` is NULL or writable for 3
/// integers; `found` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_layout_font_extents(
    font: *const SofelFont,
    location: *const SofelLocation,
    direction: i32,
    script: u32,
    language: u32,
    extents: *mut i32,
    found: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, extents, found) =
        unsafe { (font_ref(font), r(location), ints::<3>(extents), w(found)) };
    status(layout_font_extents(
        None, font, location, direction, script, language, extents, found,
    ))
}

/// `sofel_scale_layout_font_extents`.
///
/// # Safety
///
/// `scale` is NULL or a live scale; `font` is NULL or a live font;
/// `extents` is NULL or writable for 3 integers; `found` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_scale_layout_font_extents(
    scale: *const SofelScale,
    font: *const SofelFont,
    location: *const SofelLocation,
    direction: i32,
    script: u32,
    language: u32,
    extents: *mut i32,
    found: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (scale, font, location, extents, found) = unsafe {
        (
            r(scale),
            font_ref(font),
            r(location),
            ints::<3>(extents),
            w(found),
        )
    };
    status(some(scale).and_then(|s| {
        layout_font_extents(
            Some(s),
            font,
            location,
            direction,
            script,
            language,
            extents,
            found,
        )
    }))
}

// --------------------------------------------------------------- names

/// `sofel_glyph_names_t`: an index of the glyph names of a font.
pub struct SofelGlyphNames {
    font: Font<'static>,
    buf: Vec<u32>,
}

/// The language a C caller asks for: none for a length of 0.
///
/// # Safety
///
/// `lang` is NULL or valid for reads of `len` bytes.
unsafe fn language<'a>(lang: *const u8, len: usize) -> Result<Option<&'a [u8]>, i32> {
    if len == 0 {
        return Ok(None);
    }
    // SAFETY: the caller's conditions.
    unsafe { bytes(lang, len) }.map(Some).ok_or(ERROR_NULL)
}

/// Copies `src` to the caller's array, snprintf-style.
fn copy_out<T: Copy>(src: &[T], out: Option<&mut [T]>, len: Option<&mut usize>) -> Status {
    let (out, len) = (some(out)?, some(len)?);
    *len = src.len();
    let dst = out.get_mut(..src.len()).ok_or(ERROR_BUFFER_TOO_SMALL)?;
    dst.copy_from_slice(src);
    Ok(())
}

/// `sofel_font_name_count`.
///
/// # Safety
///
/// `font` is NULL or a live font; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_name_count(font: *const SofelFont, count: *mut usize) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, |f| f.names().count()))
}

/// `sofel_font_name_entry`.
///
/// # Safety
///
/// `font` is NULL or a live font; `name_id` and `len` are NULL or
/// writable; `lang` is NULL with `capacity` 0 or writable for `capacity`
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_name_entry(
    font: *const SofelFont,
    index: usize,
    name_id: *mut u16,
    lang: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, name_id, lang, len) = unsafe {
        (
            font_ref(font),
            w(name_id),
            out_array(lang, capacity),
            w(len),
        )
    };
    status((|| {
        let (font, name_id) = (some(font)?, some(name_id)?);
        let e = font.names().nth(index).ok_or(ERROR_NOT_FOUND)?;
        let mut tag = [0u8; 64];
        let n = e.language.write(&mut tag);
        copy_out(tag.get(..n).unwrap_or_default(), lang, len)?;
        *name_id = e.name_id;
        Ok(())
    })())
}

macro_rules! name_utf {
    ($name:ident, $method:ident, $t:ty) => {
        /// The C function of the same name.
        ///
        /// # Safety
        ///
        /// `font` is NULL or a live font; `lang` is NULL with `lang_len`
        /// 0 or readable for `lang_len` bytes; `text` is NULL with
        /// `capacity` 0 or writable for `capacity` units; `written` and
        /// `total` are NULL or writable.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            font: *const SofelFont,
            name_id: u16,
            lang: *const u8,
            lang_len: usize,
            text: *mut $t,
            capacity: usize,
            written: *mut usize,
            total: *mut usize,
        ) -> i32 {
            // SAFETY: the caller's conditions.
            let (font, lang, text, written, total) = unsafe {
                (
                    font_ref(font),
                    language(lang, lang_len),
                    out_array(text, capacity),
                    w(written),
                    w(total),
                )
            };
            status((|| {
                let (font, text, written, total) =
                    (some(font)?, some(text)?, some(written)?, some(total)?);
                let n = font.$method(name_id, lang?, text);
                (*written, *total) = (n.written, n.total);
                Ok(())
            })())
        }
    };
}

name_utf!(sofel_font_name_utf8, name_utf8, u8);
name_utf!(sofel_font_name_utf16, name_utf16, u16);
name_utf!(sofel_font_name_utf32, name_utf32, u32);

/// `sofel_font_sfnt_name_count`.
///
/// # Safety
///
/// `font` is NULL or a live font; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_sfnt_name_count(
    font: *const SofelFont,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, |f| f.sfnt_name_count()))
}

/// `sofel_font_sfnt_name`.
///
/// # Safety
///
/// `font` is NULL or a live font; `ids` is NULL or writable for 4
/// integers; `string` and `len` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_sfnt_name(
    font: *const SofelFont,
    index: usize,
    ids: *mut u16,
    string: *mut *const u8,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions; `[u16; 4]` has the alignment of `u16`.
    let (font, ids, string, len) = unsafe {
        (
            font_ref(font),
            ids.cast::<[u16; 4]>().as_mut(),
            w(string),
            w(len),
        )
    };
    status((|| {
        let (font, ids, string, len) = (some(font)?, some(ids)?, some(string)?, some(len)?);
        let r = font.sfnt_name(index).ok_or(ERROR_NOT_FOUND)?;
        *ids = [r.platform_id, r.encoding_id, r.language_id, r.name_id];
        (*string, *len) = (r.string.as_ptr(), r.string.len());
        Ok(())
    })())
}

/// `sofel_font_postscript_name`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or a live
/// location whose source bytes remain live; `name` is NULL with `capacity` 0 or
/// writable for `capacity` bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_postscript_name(
    font: *const SofelFont,
    location: *const SofelLocation,
    name: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, name, len) = unsafe {
        (
            font_ref(font),
            r(location),
            out_array(name, capacity),
            w(len),
        )
    };
    status((|| {
        let font = some(font)?;
        let mut buf = [0u8; 128];
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        let n = font
            .instance(location)
            .postscript_name(&mut buf)
            .ok_or(ERROR_NOT_FOUND)?;
        copy_out(buf.get(..n).unwrap_or(&buf), name, len)
    })())
}

fn glyph_name_out(
    name: Option<&[u8]>,
    out: Option<&mut *const u8>,
    len: Option<&mut usize>,
) -> Status {
    let (out, len) = (some(out)?, some(len)?);
    let n = name.ok_or(ERROR_NOT_FOUND)?;
    (*out, *len) = (n.as_ptr(), n.len());
    Ok(())
}

/// `sofel_font_glyph_name`.
///
/// # Safety
///
/// `font` is NULL or a live font; `name` and `len` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_glyph_name(
    font: *const SofelFont,
    glyph: u32,
    name: *mut *const u8,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, name, len) = unsafe { (sofel_ref(font), w(name), w(len)) };
    status(some(font).and_then(|f| {
        let n = f
            .font
            .glyph_name_with(GlyphId(glyph), f.outlines.hb_extents().cff1());
        glyph_name_out(n, name, len)
    }))
}

/// `sofel_font_ft_glyph_name`.
///
/// # Safety
///
/// `font` is NULL or a live font; `name` and `len` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_glyph_name(
    font: *const SofelFont,
    glyph: u32,
    name: *mut *const u8,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, name, len) = unsafe { (sofel_ref(font), w(name), w(len)) };
    status(some(font).and_then(|f| glyph_name_out(ft_glyph_name(f, GlyphId(glyph)), name, len)))
}

/// `Font::ft_glyph_name`, through the charset index for faces FreeType
/// opens as CFF (version 1), where it reads the charset (ADR 0025).
fn ft_glyph_name(f: &SofelFont, glyph: GlyphId) -> Option<&'static [u8]> {
    match (&f.cff, &f.charset) {
        (Some(c), Some(ix)) if c.version() == Version::Cff => c.ft_glyph_name_with(ix, glyph),
        _ => f.font.ft_glyph_name(glyph),
    }
}

/// `sofel_glyph_names_create`.
///
/// # Safety
///
/// `font` is NULL or a live font that outlives the index; `error` is
/// NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_glyph_names_create(
    font: *const SofelFont,
    error: *mut i32,
) -> *mut SofelGlyphNames {
    // SAFETY: the caller's conditions.
    let (font, error) = unsafe { (font_ref(font), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    let Some(font) = font else {
        *error = ERROR_NULL;
        return core::ptr::null_mut();
    };
    let mut buf = vec![0u32; font.glyph_names_len()];
    if font.glyph_names(&mut buf).is_none() {
        *error = ERROR_INVALID_ARGUMENT;
        return core::ptr::null_mut();
    }
    *error = OK;
    Box::into_raw(Box::new(SofelGlyphNames { font: *font, buf }))
}

/// `sofel_glyph_names_destroy`.
///
/// # Safety
///
/// `names` is NULL or from `sofel_glyph_names_create`, not yet destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_glyph_names_destroy(names: *mut SofelGlyphNames) {
    if !names.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(names) });
    }
}

fn from_name(
    names: Option<&SofelGlyphNames>,
    name: Option<&[u8]>,
    glyph: Option<&mut u32>,
    ft: bool,
) -> Status {
    let (names, name, glyph) = (some(names)?, some(name)?, some(glyph)?);
    let index = names
        .font
        .glyph_names_from(&names.buf)
        .ok_or(ERROR_INVALID_ARGUMENT)?;
    let found = if ft {
        index.ft_glyph_from_name(name)
    } else {
        index.glyph_from_name(name)
    };
    *glyph = found.ok_or(ERROR_NOT_FOUND)?.0;
    Ok(())
}

/// `sofel_glyph_names_glyph_from_name`.
///
/// # Safety
///
/// `names` is NULL or a live index; `name` is NULL or readable for `len`
/// bytes; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_glyph_names_glyph_from_name(
    names: *const SofelGlyphNames,
    name: *const u8,
    len: usize,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (names, name, glyph) = unsafe { (r(names), bytes(name, len), w(glyph)) };
    status(from_name(names, name, glyph, false))
}

/// `sofel_glyph_names_ft_glyph_from_name`.
///
/// # Safety
///
/// `names` is NULL or a live index; `name` is NULL or readable for `len`
/// bytes; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_glyph_names_ft_glyph_from_name(
    names: *const SofelGlyphNames,
    name: *const u8,
    len: usize,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (names, name, glyph) = unsafe { (r(names), bytes(name, len), w(glyph)) };
    status(from_name(names, name, glyph, true))
}

/// `sofel_font_ft_face_metrics`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `values` is NULL or writable for 11
/// integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_face_metrics(
    font: *const SofelFont,
    location: *const SofelLocation,
    values: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, values) = unsafe { (font_ref(font), r(location), ints::<11>(values)) };
    status(get(font, values, |f| {
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        let m = f.instance(location).ft_face_metrics();
        let [a, b, c, d] = m.bbox;
        [
            m.ascender,
            m.descender,
            m.height,
            m.max_advance_width,
            m.max_advance_height,
            m.underline_position,
            m.underline_thickness,
            a,
            b,
            c,
            d,
        ]
        .map(i32::from)
    }))
}

/// `sofel_font_ft_v_advance`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_v_advance(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    advance: *mut u16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, advance) = unsafe { (font_ref(font), r(location), w(advance)) };
    status((|| {
        let (font, advance) = (some(font)?, some(advance)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *advance = font
            .instance(location)
            .ft_v_advance(GlyphId(glyph))
            .ok_or(ERROR_INVALID_GLYPH)?;
        Ok(())
    })())
}

/// Unscaled TrueType control values in 26.6 font units.
///
/// # Safety
///
/// `font` and `location` must be live handles or NULL. `out` must be
/// writable for `capacity` elements or NULL with zero capacity. `len`
/// must be a writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_control_values(
    font: *const SofelFont,
    location: *const SofelLocation,
    out: *mut i32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, out, len) = unsafe {
        (
            font_ref(font),
            r(location),
            out_array(out, capacity),
            w(len),
        )
    };
    status((|| {
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        let values = some(font)?
            .instance(location)
            .ft_control_values()
            .ok_or(ERROR_NOT_FOUND)?;
        copy_out(&values, out, len)
    })())
}

/// `sofel_font_ft_h_advance`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `advance` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_h_advance(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    advance: *mut u16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, advance) = unsafe { (font_ref(font), r(location), w(advance)) };
    status((|| {
        let (font, advance) = (some(font)?, some(advance)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *advance = font
            .instance(location)
            .ft_h_advance(GlyphId(glyph))
            .ok_or(ERROR_INVALID_GLYPH)?;
        Ok(())
    })())
}

/// `sofel_font_ft_top_side_bearing`.
///
/// # Safety
///
/// `font` is NULL or a live font; `location` is NULL for the default or live;
/// `bearing` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_top_side_bearing(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    y_max: i16,
    bearing: *mut i16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, bearing) = unsafe { (font_ref(font), r(location), w(bearing)) };
    status((|| {
        let (font, bearing) = (some(font)?, some(bearing)?);
        let location =
            location.map_or_else(sofel_font::var::Location::default_ref, |v| &v.location);
        *bearing = font
            .instance(location)
            .ft_top_side_bearing(GlyphId(glyph), y_max)
            .ok_or(ERROR_INVALID_GLYPH)?;
        Ok(())
    })())
}

// ---------------------------------------------------------------- CFF

/// `sofel_font_is_cff`.
///
/// # Safety
///
/// `font` is NULL or a live font.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_is_cff(font: *const SofelFont) -> i32 {
    // SAFETY: the caller's conditions.
    unsafe { font_ref(font) }.map_or(0, |f| i32::from(f.is_cff()))
}

/// `sofel_font_is_cff2`.
///
/// # Safety
///
/// `font` is NULL or a live font.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_is_cff2(font: *const SofelFont) -> i32 {
    // SAFETY: the caller's conditions.
    unsafe { font_ref(font) }.map_or(0, |f| i32::from(f.is_cff2()))
}

fn cff_of(font: Option<&SofelFont>) -> Result<&Cff<'static>, i32> {
    some(font)?.cff.as_ref().ok_or(ERROR_INVALID_CFF)
}

fn cid_error(_: CidError) -> i32 {
    ERROR_INVALID_ARGUMENT
}

/// `sofel_font_is_cid_keyed`.
///
/// # Safety
///
/// `font` is NULL or a live font; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_is_cid_keyed(font: *const SofelFont, out: *mut i32) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, out) = unsafe { (sofel_ref(font), w(out)) };
    status((|| {
        let out = some(out)?;
        *out = i32::from(cff_of(font)?.is_cid_keyed());
        Ok(())
    })())
}

/// `sofel_font_cid`.
///
/// # Safety
///
/// `font` is NULL or a live font; `cid` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_cid(font: *const SofelFont, glyph: u32, cid: *mut u32) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, cid) = unsafe { (sofel_ref(font), w(cid)) };
    status((|| {
        let cid = some(cid)?;
        *cid = 0;
        let f = some(font)?;
        let c = cff_of(font)?;
        let g = GlyphId(glyph);
        *cid = match &f.charset {
            Some(ix) => c.cid_with(ix, g),
            None => c.cid(g),
        }
        .map_err(cid_error)?;
        Ok(())
    })())
}

/// Writes `s` cut at its first 0 byte, with a terminating 0, into `out`.
fn c_string(s: Option<&[u8]>, out: &mut [u8]) -> Status {
    let s = s.unwrap_or_default();
    let s = s.split(|&b| b == 0).next().unwrap_or_default();
    let dst = out.get_mut(..=s.len()).ok_or(ERROR_BUFFER_TOO_SMALL)?;
    let (body, end) = dst.split_at_mut(s.len());
    body.copy_from_slice(s);
    if let Some(e) = end.first_mut() {
        *e = 0;
    }
    Ok(())
}

/// `sofel_font_ros`.
///
/// # Safety
///
/// `font` is NULL or a live font; `registry` and `ordering` are NULL or
/// writable for their capacities; `supplement` and `present` are NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ros(
    font: *const SofelFont,
    registry: *mut u8,
    registry_capacity: usize,
    ordering: *mut u8,
    ordering_capacity: usize,
    supplement: *mut i32,
    present: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, reg, ord, sup, present) = unsafe {
        (
            sofel_ref(font),
            out_array(registry, registry_capacity),
            out_array(ordering, ordering_capacity),
            w(supplement),
            w(present),
        )
    };
    status((|| {
        let (reg, ord, sup, present) = (some(reg)?, some(ord)?, some(sup)?, some(present)?);
        let r = cff_of(font)?.ros().map_err(cid_error)?;
        c_string(r.registry, reg)?;
        c_string(r.ordering, ord)?;
        *sup = r.supplement;
        *present = u32::from(r.registry.is_some()) | (u32::from(r.ordering.is_some()) << 1);
        Ok(())
    })())
}

/// `sofel_font_ps_font_info`.
///
/// # Safety
///
/// `font` is NULL or a live font; `values` is NULL or writable for 4
/// integers, `offsets` for 5; `strings` is NULL or writable for
/// `capacity` bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ps_font_info(
    font: *const SofelFont,
    values: *mut i64,
    strings: *mut u8,
    capacity: usize,
    offsets: *mut usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, values, strings, offsets, len) = unsafe {
        (
            sofel_ref(font),
            values.cast::<[i64; 4]>().as_mut(),
            out_array(strings, capacity),
            offsets.cast::<[usize; 5]>().as_mut(),
            w(len),
        )
    };
    status((|| {
        let (values, strings, offsets, len) =
            (some(values)?, some(strings)?, some(offsets)?, some(len)?);
        let i = cff_of(font)?
            .ps_font_info()
            .map_err(|_| ERROR_INVALID_ARGUMENT)?;
        let parts = [i.version, i.notice, i.full_name, i.family_name, i.weight];
        let cut = |s: &'static [u8]| s.split(|&b| b == 0).next().unwrap_or_default();
        let needed = parts
            .iter()
            .flatten()
            .map(|s| cut(s).len().saturating_add(1))
            .fold(0usize, usize::saturating_add);
        *len = needed;
        if needed > strings.len() {
            return Err(ERROR_BUFFER_TOO_SMALL);
        }
        let mut at = 0usize;
        for (slot, p) in offsets.iter_mut().zip(parts) {
            match p {
                None => *slot = usize::MAX,
                Some(s) => {
                    *slot = at;
                    let rest = strings.get_mut(at..).ok_or(ERROR_BUFFER_TOO_SMALL)?;
                    c_string(Some(s), rest)?;
                    at = at.saturating_add(cut(s).len()).saturating_add(1);
                }
            }
        }
        *values = [
            i.italic_angle,
            i64::from(i.is_fixed_pitch),
            i64::from(i.underline_position),
            i64::from(i.underline_thickness),
        ];
        Ok(())
    })())
}

/// `sofel_font_ps_fs_type`.
///
/// # Safety
///
/// `font` is NULL or a live font; `fs_type` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ps_fs_type(font: *const SofelFont, fs_type: *mut u16) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, fs_type) = unsafe { (sofel_ref(font), w(fs_type)) };
    status((|| {
        let out = some(fs_type)?;
        *out = cff_of(font)?.ps_fs_type();
        Ok(())
    })())
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
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    glyph: u32,
    scale: Result<Scale, i32>,
    scratch: Option<&mut SofelOutlineScratch>,
    cbox: Option<&mut [i32; 4]>,
    overlap: Option<&mut u32>,
) -> Status {
    let (font, scratch, cbox, overlap) = (some(font)?, some(scratch)?, some(cbox)?, some(overlap)?);
    let info = outlines_at(font, location)
        .load(GlyphId(glyph), scale?, &mut scratch.scratch)
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
    location: *const SofelLocation,
    glyph: u32,
    mode: u32,
    x_scale: i32,
    y_scale: i32,
    scratch: *mut SofelOutlineScratch,
    cbox: *mut i32,
    overlap: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, scratch, cbox, overlap) = unsafe {
        (
            r(font),
            r(location),
            w(scratch),
            ints::<4>(cbox),
            w(overlap),
        )
    };
    status(outline_load(
        font,
        location,
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
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    glyph: u32,
    scale: Result<Scale, i32>,
    scratch: Option<&mut SofelOutlineScratch>,
    pen: Result<CPen, i32>,
    cbox: Option<&mut [i32; 4]>,
    overlap: Option<&mut u32>,
) -> Status {
    let (font, scratch, cbox, overlap) = (some(font)?, some(scratch)?, some(cbox)?, some(overlap)?);
    let (scale, mut pen) = (scale?, pen?);
    let info = outlines_at(font, location)
        .draw(GlyphId(glyph), scale, &mut scratch.scratch, &mut pen)
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
    location: *const SofelLocation,
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
    let (font, location, scratch, cbox, overlap) = unsafe {
        (
            r(font),
            r(location),
            w(scratch),
            ints::<4>(cbox),
            w(overlap),
        )
    };
    let pen = CPen::new(move_to, line_to, quad_to, cubic_to, close, user);
    status(outline_draw(
        font,
        location,
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

enum RasterTarget {
    Normal,
    Placed(PixelBox),
    Harmony(sofel_raster::LcdMode),
    Filtered(sofel_raster::LcdMode, [u8; 5]),
}

fn rasterizer_render(
    rasterizer: Option<&mut SofelRasterizer>,
    mode: RasterTarget,
    options: Result<RenderOptions, i32>,
    out: BitmapOut<'_>,
) -> Status {
    let r = some(rasterizer)?;
    out.check()?;
    let options = options?;
    let bitmap = match mode {
        RasterTarget::Normal => r.raster.render(options),
        RasterTarget::Placed(b) => r.raster.render_placed(b, options),
        RasterTarget::Harmony(lcd) => r.raster.render_harmony(lcd, options),
        RasterTarget::Filtered(lcd, weights) => r.raster.render_filtered_lcd(lcd, weights, options),
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
        RasterTarget::Normal,
        options(fill, overlap, x_ppem, y_ppem),
        out,
    ))
}

/// `sofel_rasterizer_render_harmony`.
///
/// # Safety
///
/// `rasterizer` is NULL or a live rasterizer no other thread uses;
/// `pixels` is NULL or writable for `capacity` bytes; `len` is NULL or
/// writable; `box_` is NULL or writable for 4 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_render_harmony(
    rasterizer: *mut SofelRasterizer,
    fill: u32,
    direction: u32,
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
    let lcd = match direction {
        0 => sofel_raster::LcdMode::Horizontal,
        1 => sofel_raster::LcdMode::Vertical,
        _ => return ERROR_INVALID_ARGUMENT,
    };
    let out = BitmapOut { pixels, len, b };
    status(rasterizer_render(
        rasterizer,
        RasterTarget::Harmony(lcd),
        options(fill, 0, x_ppem, y_ppem),
        out,
    ))
}

/// `sofel_rasterizer_render_filtered_lcd`.
///
/// # Safety
///
/// `rasterizer` is NULL or a live rasterizer no other thread uses;
/// `pixels` is NULL or writable for `capacity` bytes; `len` is NULL or
/// writable; `weights` is NULL or readable for five bytes; `box_` is NULL or writable for 4 integers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_rasterizer_render_filtered_lcd(
    rasterizer: *mut SofelRasterizer,
    fill: u32,
    direction: u32,
    weights: *const u8,
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
    // SAFETY: the caller provides five readable weights or NULL.
    let Some(weights) = (unsafe { weights.cast::<[u8; 5]>().as_ref() }) else {
        return ERROR_INVALID_ARGUMENT;
    };
    let lcd = match direction {
        0 => sofel_raster::LcdMode::Horizontal,
        1 => sofel_raster::LcdMode::Vertical,
        _ => return ERROR_INVALID_ARGUMENT,
    };
    let out = BitmapOut { pixels, len, b };
    status(rasterizer_render(
        rasterizer,
        RasterTarget::Filtered(lcd, *weights),
        options(fill, 0, x_ppem, y_ppem),
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
        RasterTarget::Placed(pb),
        options(fill, overlap, x_ppem, y_ppem),
        out,
    ))
}

fn render_glyph(
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    glyph: u32,
    ppem: u16,
    scratch: Option<&mut SofelOutlineScratch>,
    rasterizer: Option<&mut SofelRasterizer>,
    out: BitmapOut<'_>,
) -> Status {
    let (font, scratch, r) = (some(font)?, some(scratch)?, some(rasterizer)?);
    out.check()?;
    let bitmap = sofel_cli::glyph::render(
        &outlines_at(font, location),
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
    location: *const SofelLocation,
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
    let (font, location, scratch, rasterizer, pixels, len, b) = unsafe {
        (
            r(font),
            r(location),
            w(scratch),
            w(rasterizer),
            out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
        )
    };
    let out = BitmapOut { pixels, len, b };
    status(render_glyph(
        font, location, glyph, ppem, scratch, rasterizer, out,
    ))
}

fn image_out(image: &sofel_raster::BgraImage, out: BitmapOut<'_>) -> Status {
    let (pixels, len, b) = (some(out.pixels)?, some(out.len)?, some(out.b)?);
    *len = image.pixels.len();
    *b = [
        image.left,
        image.top,
        to_i32(image.width),
        to_i32(image.rows),
    ];
    pixels
        .get_mut(..image.pixels.len())
        .ok_or(ERROR_BUFFER_TOO_SMALL)?
        .copy_from_slice(&image.pixels);
    Ok(())
}

struct ColorRequest<'a> {
    palette: u16,
    custom: Option<&'a [color::CColor]>,
    foreground: color::CColor,
}

fn render_color_glyph(
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    glyph: u32,
    ppem: u16,
    request: ColorRequest<'_>,
    scratch: Option<&mut SofelOutlineScratch>,
    rasterizer: Option<&mut SofelRasterizer>,
    out: BitmapOut<'_>,
    is_color: Option<&mut u32>,
) -> Status {
    use sofel_cli::glyph::{ColorGlyph, render_color};
    let (font, scratch, r) = (some(font)?, some(scratch)?, some(rasterizer)?);
    let is_color = some(is_color)?;
    out.check()?;
    let custom: Option<Vec<sofel_geom::Color>> = request
        .custom
        .map(|c| c.iter().map(|&x| x.into()).collect());
    let options = sofel_color::PaintOptions {
        palette: custom.as_deref().map_or(
            sofel_color::Palette::Font(request.palette),
            sofel_color::Palette::Custom,
        ),
        foreground: request.foreground.into(),
        ppem: 0,
    };
    let mut image = sofel_raster::BgraImage::default();
    let drawn = render_color(
        &outlines_at(font, location),
        GlyphId(glyph),
        ppem,
        &options,
        &mut scratch.scratch,
        &mut r.raster,
        &mut image,
    )
    .map_err(|e| match e {
        RenderError::Outline(e) => outline_error(e),
        RenderError::Raster(e) => raster_error(e),
    })?;
    match drawn {
        ColorGlyph::Color => {
            *is_color = 1;
            image_out(&image, out)
        }
        ColorGlyph::Gray(b) => {
            *is_color = 0;
            out.put(Ok(b))
        }
    }
}

/// `sofel_render_color_glyph`.
///
/// # Safety
///
/// As [`sofel_render_glyph`]; `custom_colors` is NULL or valid for
/// `n_custom` colors; `is_color` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_render_color_glyph(
    font: *const SofelFont,
    location: *const SofelLocation,
    glyph: u32,
    ppem: u16,
    palette: u16,
    custom_colors: *const color::CColor,
    n_custom: usize,
    foreground: color::CColor,
    scratch: *mut SofelOutlineScratch,
    rasterizer: *mut SofelRasterizer,
    pixels: *mut u8,
    capacity: usize,
    len: *mut usize,
    box_: *mut i32,
    is_color: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, scratch, rasterizer, pixels, len, b, is_color) = unsafe {
        (
            r(font),
            r(location),
            w(scratch),
            w(rasterizer),
            out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
            w(is_color),
        )
    };
    let custom = if custom_colors.is_null() {
        None
    } else {
        // SAFETY: not NULL; the caller's conditions.
        Some(unsafe { core::slice::from_raw_parts(custom_colors, n_custom) })
    };
    let out = BitmapOut { pixels, len, b };
    let request = ColorRequest {
        palette,
        custom,
        foreground,
    };
    status(render_color_glyph(
        font, location, glyph, ppem, request, scratch, rasterizer, out, is_color,
    ))
}

fn blend(
    image: Option<&mut [u8]>,
    len: Option<&mut usize>,
    b: Option<&mut [i32; 4]>,
    src: Result<Bitmap<'_>, i32>,
    bgra: [u8; 4],
) -> Status {
    let (pixels, len, b) = (some(image)?, some(len)?, some(b)?);
    let src = src?;
    let [left, top, width, rows] = *b;
    let used = pixels.get(..*len).ok_or(ERROR_INVALID_ARGUMENT)?;
    let mut image = sofel_raster::BgraImage {
        left,
        top,
        width: u32::try_from(width).map_err(|_| ERROR_INVALID_ARGUMENT)?,
        rows: u32::try_from(rows).map_err(|_| ERROR_INVALID_ARGUMENT)?,
        pixels: used.to_vec(),
    };
    let expected = usize::try_from(image.width)
        .unwrap_or(usize::MAX)
        .saturating_mul(4)
        .saturating_mul(usize::try_from(image.rows).unwrap_or(usize::MAX));
    if !image.pixels.is_empty() && image.pixels.len() != expected {
        return Err(ERROR_INVALID_ARGUMENT);
    }
    sofel_raster::blend_layer(&mut image, &src, bgra);
    if image.pixels.len() > pixels.len() {
        *len = image.pixels.len();
        return Err(ERROR_BUFFER_TOO_SMALL);
    }
    image_out(
        &image,
        BitmapOut {
            pixels: Some(pixels),
            len: Some(len),
            b: Some(b),
        },
    )
}

/// `sofel_blend_layer`.
///
/// # Safety
///
/// `pixels` is NULL or valid for reads and writes of `capacity` bytes;
/// `len` is NULL or writable; `box_` is NULL or writable for 4 integers;
/// `src_pixels` is NULL or valid for reads of `src_width × src_rows`
/// bytes; none of them overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_blend_layer(
    pixels: *mut u8,
    capacity: usize,
    len: *mut usize,
    box_: *mut i32,
    src_left: i32,
    src_top: i32,
    src_width: u32,
    src_rows: u32,
    src_pixels: *const u8,
    color: color::CColor,
) -> i32 {
    let n = usize::try_from(src_width)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::try_from(src_rows).unwrap_or(usize::MAX));
    // SAFETY: the caller's conditions.
    let (image, len, b, src) = unsafe {
        (
            out_array(pixels, capacity),
            w(len),
            ints::<4>(box_),
            if n == 0 {
                Some(&[][..])
            } else {
                bytes(src_pixels, n)
            },
        )
    };
    let src = src.ok_or(ERROR_NULL).map(|p| Bitmap {
        left: src_left,
        top: src_top,
        width: src_width,
        rows: src_rows,
        pixels: p,
    });
    status(blend(image, len, b, src, {
        let c: sofel_geom::Color = color.into();
        [c.b, c.g, c.r, c.a]
    }))
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
        FontFile::Dfont(d) => {
            (0..d.num_fonts())
                .filter_map(|i| d.font(i).ok().map(|s| (i, s)))
                .for_each(|(i, s)| validate::sfnt(&s, &mut |problem| emit(Some(i), problem)));
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

// ------------------------------------------------------------- unicode

/// `sofel_unicode_version`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_version() -> u32 {
    let (major, minor, update) = sofel_unicode::UNICODE_VERSION;
    (u32::from(major) << 16) | (u32::from(minor) << 8) | u32::from(update)
}

/// `sofel_unicode_general_category`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_general_category(codepoint: u32) -> u8 {
    sofel_unicode::general_category(codepoint) as u8
}

/// `sofel_unicode_combining_class`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_combining_class(codepoint: u32) -> u8 {
    sofel_unicode::combining_class(codepoint)
}

/// `sofel_unicode_script`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_script(codepoint: u32) -> u32 {
    u32::from_be_bytes(sofel_unicode::script(codepoint).tag())
}

/// `sofel_script_from_iso15924`.
///
/// # Safety
///
/// `tag` is NULL or valid for reads of `len` bytes; `script` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_script_from_iso15924(
    tag: *const u8,
    len: usize,
    script: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (tag, script) = unsafe { (bytes(tag, len), w(script)) };
    status((|| {
        let (tag, script) = (some(tag)?, some(script)?);
        let s = sofel_unicode::Script::from_iso15924(tag).ok_or(ERROR_INVALID_ARGUMENT)?;
        *script = u32::from_be_bytes(s.tag());
        Ok(())
    })())
}

/// `sofel_unicode_mirroring`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_mirroring(codepoint: u32) -> u32 {
    sofel_unicode::mirroring(codepoint)
}

/// `sofel_unicode_decompose`.
///
/// # Safety
///
/// `a` and `b` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_unicode_decompose(codepoint: u32, a: *mut u32, b: *mut u32) -> i32 {
    // SAFETY: the caller's conditions.
    let (a, b) = unsafe { (w(a), w(b)) };
    status((|| {
        let (a, b) = (some(a)?, some(b)?);
        (*a, *b) = sofel_unicode::decompose(codepoint).ok_or(ERROR_NOT_FOUND)?;
        Ok(())
    })())
}

/// `sofel_unicode_compose`.
///
/// # Safety
///
/// `ab` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_unicode_compose(a: u32, b: u32, ab: *mut u32) -> i32 {
    // SAFETY: the caller's conditions.
    let ab = unsafe { w(ab) };
    status((|| {
        let ab = some(ab)?;
        *ab = sofel_unicode::compose(a, b).ok_or(ERROR_NOT_FOUND)?;
        Ok(())
    })())
}

/// `sofel_unicode_is_extended_pictographic`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_is_extended_pictographic(codepoint: u32) -> u8 {
    u8::from(sofel_unicode::is_extended_pictographic(codepoint))
}

/// `sofel_unicode_vertical_orientation`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_vertical_orientation(codepoint: u32) -> u8 {
    sofel_unicode::vertical_orientation(codepoint) as u8
}

#[cfg(test)]
#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(clippy::unwrap_used)]
mod test_build;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn inv2_axis_ppem_values() {
        let data = test_build::sample_font();
        let mut error = 0;
        // SAFETY: the font bytes remain live until the handle is destroyed.
        let font = unsafe { sofel_font_create(data.as_ptr(), data.len(), 0, &mut error) };
        assert_eq!(error, OK);
        assert!(!font.is_null());
        for (x_ppem, y_ppem) in [(0, 0), (1, 0), (0, 1), (12, 16), (u32::MAX, 1)] {
            // SAFETY: font is a live handle and error is writable.
            let scale = unsafe {
                sofel_scale_create_with_ppem(font, 1000, 2000, x_ppem, y_ppem, &mut error)
            };
            assert_eq!(error, OK);
            assert!(!scale.is_null());
            // SAFETY: a successful constructor returns a live scale.
            let scale_value = unsafe { &*scale };
            let rust =
                sofel_shape::Scale::new(scale_value.scale, scale_value.x_ppem, scale_value.y_ppem);
            assert_eq!((rust.x_ppem, rust.y_ppem), (x_ppem, y_ppem));
            // SAFETY: scale is a live handle and is destroyed once.
            unsafe { sofel_scale_destroy(scale) };
        }
        // SAFETY: font is a live handle and is destroyed once.
        unsafe { sofel_font_destroy(font) };
    }

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
