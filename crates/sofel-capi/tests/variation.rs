//! C ABI of locations and variation strings (ADR 0027).

#![allow(clippy::unwrap_used, clippy::undocumented_unsafe_blocks)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{sample_tables_with_glyf, sfnt};
use sofel_capi::*;

fn font_bytes() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn i01_location_owns_coordinates_after_font_handle_is_destroyed() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: the bytes remain alive until after the location is destroyed.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    assert!(!font.is_null());
    let mut location = core::ptr::null_mut();
    let coords = [0.5f32];
    // SAFETY: the font is live, coords is readable, and location is writable.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    assert!(!location.is_null());
    // SAFETY: this font handle is no longer needed; bytes are still alive.
    unsafe { sofel_font_destroy(font) };
    let mut out = [0f32; 1];
    let mut needed = 0usize;
    // SAFETY: the location is live, out and needed are writable.
    assert_eq!(
        unsafe { sofel_location_coords_design(location, out.as_mut_ptr(), 1, &mut needed) },
        OK
    );
    assert_eq!((out, needed), ([0.5], 1));
    // SAFETY: the location is owned and no longer used.
    unsafe { sofel_location_destroy(location) };
}

#[test]
fn i02_location_constructors_and_coordinate_views() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for the font and all locations.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let variation = SofelVariation {
        tag: u32::from_be_bytes(*b"wght"),
        value: 0.5,
    };
    let mut location = core::ptr::null_mut();
    // SAFETY: font is live and input/output pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create(font, u32::MAX, &variation, 1, &mut location) },
        OK
    );
    let mut normalized = [0i32; 1];
    let mut needed = 0usize;
    // SAFETY: location and output pointers are valid.
    assert_eq!(
        unsafe {
            sofel_location_coords_normalized(location, normalized.as_mut_ptr(), 1, &mut needed)
        },
        OK
    );
    assert_eq!((normalized, needed), ([8192], 1));
    let mut short = [17i32; 1];
    // SAFETY: location is live; a zero-capacity output is permitted.
    assert_eq!(
        unsafe { sofel_location_coords_normalized(location, short.as_mut_ptr(), 0, &mut needed) },
        ERROR_BUFFER_TOO_SMALL
    );
    assert_eq!((short, needed), ([17], 1));
    let mut ft = [0i32; 1];
    // SAFETY: location and output pointers are valid.
    assert_eq!(
        unsafe { sofel_location_ft_coords(location, ft.as_mut_ptr(), 1, &mut needed) },
        OK
    );
    assert_eq!((ft, needed), ([32768], 1));
    // SAFETY: location is owned and no longer used.
    unsafe { sofel_location_destroy(location) };

    let invalid = [0x4001i32];
    // SAFETY: font and pointer are valid.
    assert_eq!(
        unsafe { sofel_location_create_normalized_2_14(font, invalid.as_ptr(), 1, &mut location) },
        OK
    );
    let mut ft_error = 0i32;
    // SAFETY: location and output pointer are valid.
    assert_eq!(
        unsafe { sofel_location_ft_error(location, &mut ft_error) },
        OK
    );
    assert_eq!(ft_error, ERROR_INVALID_ARGUMENT);
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
    }
    let blend = [32768i32];
    // SAFETY: font and input/output pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_normalized_16_16(font, blend.as_ptr(), 1, &mut location) },
        OK
    );
    // SAFETY: location and output pointers are valid.
    assert_eq!(
        unsafe { sofel_location_ft_design_coords(location, ft.as_mut_ptr(), 1, &mut needed) },
        OK
    );
    assert_eq!((ft, needed), ([32768], 1));
    // SAFETY: location is owned and no longer used.
    unsafe { sofel_location_destroy(location) };
    // SAFETY: font and output are valid, and the out-of-range index is a location error.
    assert_eq!(
        unsafe { sofel_location_create_named_instance(font, 1, &mut location) },
        OK
    );
    // SAFETY: location and output pointer are valid.
    assert_eq!(
        unsafe { sofel_location_ft_error(location, &mut ft_error) },
        OK
    );
    assert_eq!(ft_error, ERROR_INVALID_ARGUMENT);
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_variation_parse_and_format_roundtrip() {
    let mut variation = SofelVariation::default();
    // SAFETY: input bytes are readable and output is writable.
    assert_eq!(
        unsafe { sofel_variation_parse(c"wght=500".as_ptr(), 8, &mut variation) },
        OK
    );
    assert_eq!(
        (variation.tag, variation.value),
        (u32::from_be_bytes(*b"wght"), 500.0)
    );
    let mut out = [0u8; 16];
    let mut needed = 0usize;
    // SAFETY: output buffer and needed are writable.
    assert_eq!(
        unsafe {
            sofel_variation_format(&variation, out.as_mut_ptr().cast(), out.len(), &mut needed)
        },
        OK
    );
    assert_eq!(out.get(..needed), Some(b"wght=500".as_slice()));
    assert_eq!(out.get(needed), Some(&0));
    let mut short = [11u8; 1];
    // SAFETY: output and length are writable.
    assert_eq!(
        unsafe { sofel_variation_format(&variation, short.as_mut_ptr().cast(), 1, &mut needed) },
        ERROR_BUFFER_TOO_SMALL
    );
    assert_eq!((short, needed), ([11], 8));
    // SAFETY: input and output pointers are valid.
    assert_eq!(
        unsafe { sofel_variation_parse(c"bad".as_ptr(), 3, &mut variation) },
        ERROR_INVALID_ARGUMENT
    );
    // SAFETY: a NULL input is intentionally tested and output is writable.
    assert_eq!(
        unsafe { sofel_variation_parse(core::ptr::null(), 0, &mut variation) },
        ERROR_NULL
    );
}
