//! C ABI for ADR 0062 layout queries.

use sofel_font::layout_query::{LayoutQueries, MathGlyphPart, MathGlyphVariant, MathKernEntry};
use sofel_font::read::{GlyphId, Tag, math::Corner};

use crate::variation::SofelLocation;
use crate::{
    ERROR_INVALID_ARGUMENT, SofelFont, SofelScale, Status, direction, out_array, r, scale_of, some,
    status, w,
};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
/// One MATH kern entry in the C ABI.
pub struct SofelMathKernEntry {
    max_correction_height: i32,
    kern_value: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
/// One MATH glyph variant in the C ABI.
pub struct SofelMathGlyphVariant {
    glyph: u32,
    advance: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
/// One MATH glyph assembly part in the C ABI.
pub struct SofelMathGlyphPart {
    glyph: u32,
    start_connector_length: i32,
    end_connector_length: i32,
    full_advance: i32,
    flags: u16,
}

fn query<'f, 'l>(
    font: &'f SofelFont,
    location: Option<&'l SofelLocation>,
    scale: Option<&SofelScale>,
) -> LayoutQueries<'f, 'static, 'l, 'static> {
    let location: &'l sofel_font::var::Location<'static> = match location {
        Some(location) => &location.location,
        None => sofel_font::var::Location::default_ref(),
    };
    let (x_ppem, y_ppem) = scale.map_or((0, 0), |scale| (scale.x_ppem, scale.y_ppem));
    font.font
        .instance(location)
        .layout_queries(scale_of(scale, &font.font), x_ppem, y_ppem)
}

fn corner(value: i32) -> Result<Corner, i32> {
    match value {
        0 => Ok(Corner::TopRight),
        1 => Ok(Corner::TopLeft),
        2 => Ok(Corner::BottomRight),
        3 => Ok(Corner::BottomLeft),
        _ => Err(ERROR_INVALID_ARGUMENT),
    }
}

fn page<T, U>(
    values: &[U],
    start: usize,
    out: &mut [T],
    stored: &mut usize,
    total: &mut usize,
    map: impl Fn(&U) -> T,
) {
    *total = values.len();
    let src = values.get(start..).unwrap_or(&[]);
    let count = src.len().min(out.len());
    for (dest, source) in out.iter_mut().zip(src.iter()).take(count) {
        *dest = map(source);
    }
    *stored = count;
}

macro_rules! scalar {
    ($name:ident ($font:ident, $location:ident, $scale:ident, $($arg:ident : $ty:ty),*) => $body:expr) => {
        /// A scalar MATH layout query.
        ///
        /// # Safety
        ///
        /// Handles are NULL or live; a non-NULL output points to writable storage.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(font: *const SofelFont, location: *const SofelLocation, scale: *const SofelScale, $($arg: $ty,)* value: *mut i32) -> i32 {
            // SAFETY: the caller supplies live handles and a writable output.
            let (font, location, scale, value) = unsafe { (r(font), r(location), r(scale), w(value)) };
            status((|| -> Status {
                let (font, value) = (some(font)?, some(value)?);
                let $font = font;
                let $location = location;
                let $scale = scale;
                *value = $body;
                Ok(())
            })())
        }
    };
}

scalar!(sofel_math_constant(font, location, scale, constant: u32) => query(font, location, scale).math_constant(constant));
scalar!(sofel_math_italics_correction(font, location, scale, glyph: u32) => query(font, location, scale).math_italics_correction(GlyphId(glyph)));
scalar!(sofel_math_top_accent_attachment(font, location, scale, glyph: u32) => query(font, location, scale).math_top_accent_attachment(GlyphId(glyph)));

/// Whether the font has a readable MATH table.
///
/// # Safety
///
/// `font` is NULL or live; `has_data` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_has_data(font: *const SofelFont, has_data: *mut i32) -> i32 {
    // SAFETY: the caller supplies a live font and a writable output.
    let (font, has_data) = unsafe { (r(font), w(has_data)) };
    status((|| -> Status {
        *some(has_data)? = i32::from(query(some(font)?, None, None).has_math());
        Ok(())
    })())
}

/// Query a style value at the specified location and point size.
///
/// # Safety
///
/// Handles are NULL or live; `value` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_style_value(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    tag: u32,
    value: *mut f32,
) -> i32 {
    // SAFETY: the caller supplies live handles and a writable output.
    let (font, location, scale, value) = unsafe { (r(font), r(location), r(scale), w(value)) };
    status((|| -> Status {
        *some(value)? = query(some(font)?, location, scale).style_value(Tag::from_u32(tag));
        Ok(())
    })())
}

/// Whether a glyph is marked as an extended MATH shape.
///
/// # Safety
///
/// `font` is NULL or live; `is_extended` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_is_extended_shape(
    font: *const SofelFont,
    glyph: u32,
    is_extended: *mut i32,
) -> i32 {
    // SAFETY: the caller supplies a live font and a writable output.
    let (font, is_extended) = unsafe { (r(font), w(is_extended)) };
    status((|| -> Status {
        *some(is_extended)? =
            i32::from(query(some(font)?, None, None).math_is_extended_shape(GlyphId(glyph)));
        Ok(())
    })())
}

/// Query the MATH kern at a correction height.
///
/// # Safety
///
/// Handles are NULL or live; `value` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_kerning(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    glyph: u32,
    corner_value: i32,
    correction_height: i32,
    value: *mut i32,
) -> i32 {
    // SAFETY: the caller supplies live handles and a writable output.
    let (font, location, scale, value) = unsafe { (r(font), r(location), r(scale), w(value)) };
    status((|| -> Status {
        *some(value)? = query(some(font)?, location, scale).math_kerning(
            GlyphId(glyph),
            corner(corner_value)?,
            correction_height,
        );
        Ok(())
    })())
}

/// Query the minimum overlap for MATH assembly parts.
///
/// # Safety
///
/// Handles are NULL or live; `value` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_min_connector_overlap(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    direction_value: i32,
    value: *mut i32,
) -> i32 {
    // SAFETY: the caller supplies live handles and a writable output.
    let (font, location, scale, value) = unsafe { (r(font), r(location), r(scale), w(value)) };
    status((|| -> Status {
        *some(value)? = query(some(font)?, location, scale)
            .math_min_connector_overlap(direction(direction_value)?);
        Ok(())
    })())
}

/// Copy a page of GDEF ligature caret positions.
///
/// # Safety
///
/// Handles are NULL or live; outputs point to writable storage of their given capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_layout_ligature_carets(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    glyph: u32,
    direction_value: i32,
    start: usize,
    carets: *mut i32,
    capacity: usize,
    stored: *mut usize,
    total: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies live handles, writable counts and `capacity` output entries.
    let (font, location, scale, carets, stored, total) = unsafe {
        (
            r(font),
            r(location),
            r(scale),
            out_array(carets, capacity),
            w(stored),
            w(total),
        )
    };
    status((|| -> Status {
        let values = query(some(font)?, location, scale)
            .ligature_carets(GlyphId(glyph), direction(direction_value)?);
        page(
            &values,
            start,
            some(carets)?,
            some(stored)?,
            some(total)?,
            |v| *v,
        );
        Ok(())
    })())
}

/// Copy a page of MATH kern entries.
///
/// # Safety
///
/// Handles are NULL or live; outputs point to writable storage of their given capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_kernings(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    glyph: u32,
    corner_value: i32,
    start: usize,
    entries: *mut SofelMathKernEntry,
    capacity: usize,
    stored: *mut usize,
    total: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies live handles, writable counts and `capacity` output entries.
    let (font, location, scale, entries, stored, total) = unsafe {
        (
            r(font),
            r(location),
            r(scale),
            out_array(entries, capacity),
            w(stored),
            w(total),
        )
    };
    status((|| -> Status {
        let values = query(some(font)?, location, scale)
            .math_kernings(GlyphId(glyph), corner(corner_value)?);
        page(
            &values,
            start,
            some(entries)?,
            some(stored)?,
            some(total)?,
            |&MathKernEntry {
                 max_correction_height,
                 kern_value,
             }| SofelMathKernEntry {
                max_correction_height,
                kern_value,
            },
        );
        Ok(())
    })())
}

/// Copy a page of MATH glyph variants.
///
/// # Safety
///
/// Handles are NULL or live; outputs point to writable storage of their given capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_variants(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    glyph: u32,
    direction_value: i32,
    start: usize,
    variants: *mut SofelMathGlyphVariant,
    capacity: usize,
    stored: *mut usize,
    total: *mut usize,
) -> i32 {
    // SAFETY: the caller supplies live handles, writable counts and `capacity` output entries.
    let (font, location, scale, variants, stored, total) = unsafe {
        (
            r(font),
            r(location),
            r(scale),
            out_array(variants, capacity),
            w(stored),
            w(total),
        )
    };
    status((|| -> Status {
        let values = query(some(font)?, location, scale)
            .math_variants(GlyphId(glyph), direction(direction_value)?);
        page(
            &values,
            start,
            some(variants)?,
            some(stored)?,
            some(total)?,
            |&MathGlyphVariant { glyph, advance }| SofelMathGlyphVariant {
                glyph: glyph.0,
                advance,
            },
        );
        Ok(())
    })())
}

/// Copy a page of MATH glyph assembly parts and its italic correction.
///
/// # Safety
///
/// Handles are NULL or live; outputs point to writable storage of their given capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_math_assembly(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    glyph: u32,
    direction_value: i32,
    start: usize,
    parts: *mut SofelMathGlyphPart,
    capacity: usize,
    stored: *mut usize,
    total: *mut usize,
    italics_correction: *mut i32,
) -> i32 {
    // SAFETY: the caller supplies live handles, writable counts and `capacity` output entries.
    let (font, location, scale, parts, stored, total, italics_correction) = unsafe {
        (
            r(font),
            r(location),
            r(scale),
            out_array(parts, capacity),
            w(stored),
            w(total),
            w(italics_correction),
        )
    };
    status((|| -> Status {
        let assembly = query(some(font)?, location, scale)
            .math_assembly(GlyphId(glyph), direction(direction_value)?);
        *some(italics_correction)? = assembly.italics_correction;
        page(
            &assembly.parts,
            start,
            some(parts)?,
            some(stored)?,
            some(total)?,
            |&MathGlyphPart {
                 glyph,
                 start_connector_length,
                 end_connector_length,
                 full_advance,
                 flags,
             }| SofelMathGlyphPart {
                glyph: glyph.0,
                start_connector_length,
                end_connector_length,
                full_advance,
                flags,
            },
        );
        Ok(())
    })())
}
