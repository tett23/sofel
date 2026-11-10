//! Location and variation string C ABI (ADR 0027).

use super::*;
use sofel_font::read::Fixed as ReadFixed;
use sofel_font::var::{FtLocationError, Location, Variation};

/// `sofel_location_t`: immutable coordinates and cached variation scalars.
pub struct SofelLocation {
    location: Location<'static>,
}

/// `sofel_variation_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SofelVariation {
    /// Four-byte axis tag.
    pub tag: u32,
    /// Design-space axis value.
    pub value: f32,
}

/// A caller's input array; NULL is allowed only for zero elements.
///
/// # Safety
///
/// Non-NULL `ptr` is valid for reads of `len` elements.
unsafe fn input_array<'a, T>(ptr: *const T, len: usize) -> Option<&'a [T]> {
    if ptr.is_null() {
        return (len == 0).then_some(&[]);
    }
    // SAFETY: the caller guarantees that the non-NULL array is readable.
    Some(unsafe { core::slice::from_raw_parts(ptr, len) })
}

fn set_location(out: Option<&mut *mut SofelLocation>, location: Location<'static>) -> Status {
    let out = some(out)?;
    *out = Box::into_raw(Box::new(SofelLocation { location }));
    Ok(())
}

/// `sofel_location_create`.
///
/// # Safety
///
/// `font` is NULL or live; `variations` is NULL only if `count` is zero;
/// `out` is NULL or writable. The font's source bytes must stay alive and
/// unchanged until the returned location is destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_create(
    font: *const SofelFont,
    named_instance: u32,
    variations: *const SofelVariation,
    count: usize,
    out: *mut *mut SofelLocation,
) -> i32 {
    // SAFETY: the caller guarantees the font, input array and output.
    let (font, variations, out) =
        unsafe { (font_ref(font), input_array(variations, count), w(out)) };
    status((|| {
        let (font, variations) = (some(font)?, some(variations)?);
        let named = if named_instance == u32::MAX {
            None
        } else {
            Some(u16::try_from(named_instance).map_err(|_| ERROR_INVALID_ARGUMENT)?)
        };
        let values: Vec<Variation> = variations
            .iter()
            .map(|v| Variation {
                tag: Tag::from_u32(v.tag),
                value: v.value,
            })
            .collect();
        set_location(out, Location::new(font, named, &values))
    })())
}

/// `sofel_location_create_design`.
///
/// # Safety
///
/// `font` is NULL or live; `coords` is NULL only if `count` is zero;
/// `out` is NULL or writable. Keep the source bytes alive and unchanged
/// until the returned location is destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_create_design(
    font: *const SofelFont,
    coords: *const f32,
    count: usize,
    out: *mut *mut SofelLocation,
) -> i32 {
    // SAFETY: the caller guarantees the font, input array and output.
    let (font, coords, out) = unsafe { (font_ref(font), input_array(coords, count), w(out)) };
    status((|| {
        set_location(out, Location::from_design(some(font)?, some(coords)?))
    })())
}

/// `sofel_location_create_normalized_2_14`.
///
/// # Safety
///
/// `font` is NULL or live; `coords` is NULL only if `count` is zero;
/// `out` is NULL or writable. Keep source bytes alive until destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_create_normalized_2_14(
    font: *const SofelFont,
    coords: *const i32,
    count: usize,
    out: *mut *mut SofelLocation,
) -> i32 {
    // SAFETY: the caller guarantees the font, input array and output.
    let (font, coords, out) = unsafe { (font_ref(font), input_array(coords, count), w(out)) };
    status((|| {
        set_location(
            out,
            Location::from_normalized_2_14(some(font)?, some(coords)?),
        )
    })())
}

/// `sofel_location_create_normalized_16_16`.
///
/// # Safety
///
/// `font` is NULL or live; `coords` is NULL only if `count` is zero;
/// `out` is NULL or writable. Keep source bytes alive until destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_create_normalized_16_16(
    font: *const SofelFont,
    coords: *const i32,
    count: usize,
    out: *mut *mut SofelLocation,
) -> i32 {
    // SAFETY: the caller guarantees the font, input array and output.
    let (font, coords, out) = unsafe { (font_ref(font), input_array(coords, count), w(out)) };
    status((|| {
        let coords: Vec<ReadFixed> = some(coords)?.iter().copied().map(ReadFixed).collect();
        set_location(out, Location::from_normalized_16_16(some(font)?, &coords))
    })())
}

/// `sofel_location_create_named_instance`.
///
/// # Safety
///
/// `font` is NULL or live and `out` is NULL or writable. Keep source bytes
/// alive until the returned location is destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_create_named_instance(
    font: *const SofelFont,
    index: u32,
    out: *mut *mut SofelLocation,
) -> i32 {
    // SAFETY: the caller guarantees the font and output pointers.
    let (font, out) = unsafe { (font_ref(font), w(out)) };
    status((|| {
        let index = u16::try_from(index).map_err(|_| ERROR_INVALID_ARGUMENT)?;
        set_location(out, Location::named_instance(some(font)?, index))
    })())
}

/// `sofel_location_destroy`.
///
/// # Safety
///
/// `location` is NULL or an owned live location, not used by another thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_destroy(location: *mut SofelLocation) {
    if !location.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is freed once.
        drop(unsafe { Box::from_raw(location) });
    }
}

fn location_values<T: Copy>(
    location: Option<&SofelLocation>,
    out: Option<&mut [T]>,
    len: Option<&mut usize>,
    values: impl for<'a> FnOnce(&'a Location<'static>) -> &'a [T],
) -> Status {
    copy_out(values(&some(location)?.location), out, len)
}

/// `sofel_location_coords_normalized`.
///
/// # Safety
///
/// `location` is NULL or live; `out` has `capacity` writable elements or
/// is NULL with zero capacity; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_coords_normalized(
    location: *const SofelLocation,
    out: *mut i32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees each pointer.
    let (location, out, len) = unsafe { (r(location), out_array(out, capacity), w(len)) };
    status(location_values(
        location,
        out,
        len,
        Location::coords_normalized,
    ))
}

/// `sofel_location_coords_design`.
///
/// # Safety
///
/// Pointer conditions match `sofel_location_coords_normalized` with f32 output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_coords_design(
    location: *const SofelLocation,
    out: *mut f32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees each pointer.
    let (location, out, len) = unsafe { (r(location), out_array(out, capacity), w(len)) };
    status(location_values(location, out, len, Location::coords_design))
}

fn ft_values(
    location: Option<&SofelLocation>,
    out: Option<&mut [i32]>,
    len: Option<&mut usize>,
    values: impl for<'a> FnOnce(&'a Location<'static>) -> &'a [ReadFixed],
) -> Status {
    let (location, out, len) = (some(location)?, some(out)?, some(len)?);
    let values = values(&location.location);
    *len = values.len();
    if out.len() < values.len() {
        return Err(ERROR_BUFFER_TOO_SMALL);
    }
    for (dest, src) in out.iter_mut().zip(values) {
        *dest = src.0;
    }
    Ok(())
}

/// `sofel_location_ft_coords`.
///
/// # Safety
///
/// Pointer conditions match `sofel_location_coords_normalized`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_ft_coords(
    location: *const SofelLocation,
    out: *mut i32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees each pointer.
    let (location, out, len) = unsafe { (r(location), out_array(out, capacity), w(len)) };
    status(ft_values(location, out, len, Location::ft_coords))
}

/// `sofel_location_ft_design_coords`.
///
/// # Safety
///
/// Pointer conditions match `sofel_location_coords_normalized`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_ft_design_coords(
    location: *const SofelLocation,
    out: *mut i32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees each pointer.
    let (location, out, len) = unsafe { (r(location), out_array(out, capacity), w(len)) };
    status(ft_values(location, out, len, Location::ft_design_coords))
}

/// `sofel_location_ft_error`.
///
/// # Safety
///
/// `location` is NULL or live, and `error` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_ft_error(
    location: *const SofelLocation,
    error: *mut i32,
) -> i32 {
    // SAFETY: the caller guarantees both pointers.
    let (location, error) = unsafe { (r(location), w(error)) };
    status((|| {
        *some(error)? = match some(location)?.location.ft_error() {
            None => OK,
            Some(FtLocationError::InvalidArgument) => ERROR_INVALID_ARGUMENT,
        };
        Ok(())
    })())
}

/// `sofel_location_named_instance_index`.
///
/// # Safety
///
/// `location` is NULL or live, and `index` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_named_instance_index(
    location: *const SofelLocation,
    index: *mut u32,
) -> i32 {
    // SAFETY: the caller guarantees both pointers.
    let (location, index) = unsafe { (r(location), w(index)) };
    status((|| {
        *some(index)? = some(location)?
            .location
            .named_instance_index()
            .map_or(u32::MAX, u32::from);
        Ok(())
    })())
}

/// `sofel_location_hb_nonzero`.
///
/// # Safety
///
/// `location` is NULL or live, and `nonzero` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_hb_nonzero(
    location: *const SofelLocation,
    nonzero: *mut u8,
) -> i32 {
    // SAFETY: the caller guarantees both pointers.
    let (location, nonzero) = unsafe { (r(location), w(nonzero)) };
    status((|| {
        *some(nonzero)? = u8::from(some(location)?.location.hb_nonzero());
        Ok(())
    })())
}

/// `sofel_location_ft_default`.
///
/// # Safety
///
/// `location` is NULL or live, and `is_default` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_location_ft_default(
    location: *const SofelLocation,
    is_default: *mut u8,
) -> i32 {
    // SAFETY: the caller guarantees both pointers.
    let (location, is_default) = unsafe { (r(location), w(is_default)) };
    status((|| {
        *some(is_default)? = u8::from(some(location)?.location.ft_default());
        Ok(())
    })())
}

/// `sofel_variation_parse`.
///
/// # Safety
///
/// `input` is NULL or readable for `len` bytes, and `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_variation_parse(
    input: *const c_char,
    len: usize,
    out: *mut SofelVariation,
) -> i32 {
    // SAFETY: the caller guarantees input and output pointers.
    let (input, out) = unsafe { (bytes(input.cast::<u8>(), len), w(out)) };
    status((|| {
        let text = core::str::from_utf8(some(input)?).map_err(|_| ERROR_INVALID_ARGUMENT)?;
        let parsed = Variation::parse(text).ok_or(ERROR_INVALID_ARGUMENT)?;
        *some(out)? = SofelVariation {
            tag: parsed.tag.to_u32(),
            value: parsed.value,
        };
        Ok(())
    })())
}

/// `sofel_variation_format`.
///
/// # Safety
///
/// `variation` is NULL or readable; `out` is NULL only if `capacity` is
/// zero; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_variation_format(
    variation: *const SofelVariation,
    out: *mut c_char,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees output pointers.
    let (variation, out, len) =
        unsafe { (r(variation), out_array(out.cast::<u8>(), capacity), w(len)) };
    status((|| {
        let (variation, out, len) = (some(variation)?, some(out)?, some(len)?);
        let value = Variation {
            tag: Tag::from_u32(variation.tag),
            value: variation.value,
        };
        let formatted = value.format();
        *len = formatted.len();
        let (nul, text) = out
            .get_mut(..=formatted.len())
            .and_then(|bytes| bytes.split_last_mut())
            .ok_or(ERROR_BUFFER_TOO_SMALL)?;
        text.copy_from_slice(formatted.as_bytes());
        *nul = 0;
        Ok(())
    })())
}
