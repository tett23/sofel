//! C ABI of locations and variation strings (ADR 0027).

#![allow(clippy::unwrap_used, clippy::undocumented_unsafe_blocks)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{Buf, name, sample_tables_with_glyf, sfnt, utf16be};
use sofel_capi::*;

#[test]
fn adr0030_i06_cff2_blend_vector_c_abi_preserves_columns_and_short_buffer() {
    let store = build::item_variation_store(
        1,
        &[vec![(0, 0x4000, 0x4000)], vec![(0, 0, 0)]],
        &[vec![0, 1, 0]],
    );
    let table = build::cff2(&build::Cff2Spec {
        charstrings: vec![Vec::new()],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: Some(store),
        ..build::Cff2Spec::default()
    });
    let bytes = build::otto_font(1, vec![(b"CFF2", table), (b"fvar", fvar_one_axis())]);
    let mut error = -99;
    // SAFETY: font bytes remain live through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let mut len = 0;
    // SAFETY: font and len are live; NULL output with zero capacity is valid.
    assert_eq!(
        unsafe {
            sofel_cff2_blend_vector(
                font,
                core::ptr::null(),
                0,
                core::ptr::null_mut(),
                0,
                &mut len,
            )
        },
        ERROR_BUFFER_TOO_SMALL
    );
    assert_eq!(len, 4);
    let mut out = [7; 4];
    // SAFETY: font and output are live and writable.
    assert_eq!(
        unsafe {
            sofel_cff2_blend_vector(font, core::ptr::null(), 0, out.as_mut_ptr(), 3, &mut len)
        },
        ERROR_BUFFER_TOO_SMALL
    );
    assert_eq!(out, [7; 4]);
    // SAFETY: font and output are live and writable.
    assert_eq!(
        unsafe {
            sofel_cff2_blend_vector(font, core::ptr::null(), 0, out.as_mut_ptr(), 4, &mut len)
        },
        OK
    );
    assert_eq!((out, len), ([0x10000, 0, 0, 0], 4));
    let coords = [1.0f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font, coordinates and output are live.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    // SAFETY: font, location and output are live.
    assert_eq!(
        unsafe { sofel_cff2_blend_vector(font, location, 0, out.as_mut_ptr(), 4, &mut len) },
        OK
    );
    assert_eq!((out, len), ([0x10000; 4], 4));
    // SAFETY: font, location and output are live.
    assert_eq!(
        unsafe { sofel_cff2_blend_vector(font, location, 1, out.as_mut_ptr(), 4, &mut len) },
        ERROR_INVALID_ARGUMENT
    );
    // SAFETY: handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

fn fvar_one_axis() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    fvar
}

fn font_bytes() -> Vec<u8> {
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn base_variation_font_bytes() -> Vec<u8> {
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .done();
    let coord = || build::Coord::F3(101, Some(build::device(0, 0, 0x8000, &[])));
    let axis = build::AxisSpec {
        tags: Some(vec![*b"ideo", *b"romn"]),
        scripts: Some(vec![(
            *b"DFLT",
            Some(build::ScriptSpec {
                values: Some((0, vec![Some(coord()), Some(coord())])),
                default_min_max: Some(build::MinMaxSpec {
                    min: Some(build::Coord::F3(10, Some(build::device(0, 0, 0x8000, &[])))),
                    max: Some(coord()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )]),
    };
    let mut tables = sample_tables_with_glyf();
    tables.push((b"BASE", build::base11(Some(&axis), None, Some(&store))));
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn i03_base_variation_index_reaches_scaled_c_abi() {
    let bytes = base_variation_font_bytes();
    let mut error = -99;
    // SAFETY: font bytes remain alive through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [1.0f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let scale = unsafe { sofel_scale_create(font, 500, 500, &mut error) };
    assert_eq!(error, OK);
    let mut value = i32::MIN;
    let script = u32::from_be_bytes(*b"DFLT");
    // SAFETY: all handles are live and value is writable.
    assert_eq!(
        unsafe {
            sofel_scale_baseline(
                scale,
                font,
                location,
                u32::from_be_bytes(*b"romn"),
                0,
                script,
                &mut value,
            )
        },
        OK
    );
    assert_eq!(value, 62);
    assert_eq!(
        unsafe {
            sofel_scale_baseline_with_fallback(
                scale,
                font,
                location,
                u32::from_be_bytes(*b"idtp"),
                0,
                script,
                &mut value,
            )
        },
        OK
    );
    assert_eq!(value, 562);
    let mut extents = [i32::MIN; 3];
    let mut found = -1;
    // SAFETY: all handles are live and outputs are writable.
    assert_eq!(
        unsafe {
            sofel_scale_layout_font_extents(
                scale,
                font,
                location,
                0,
                script,
                u32::from_be_bytes(*b"dflt"),
                extents.as_mut_ptr(),
                &mut found,
            )
        },
        OK
    );
    assert_eq!(found, 1);
    assert_eq!(extents, [62, 16, 0]);
    // SAFETY: each handle is owned and no longer used.
    unsafe {
        sofel_scale_destroy(scale);
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

fn hvar_font_bytes() -> Vec<u8> {
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .done();
    let hvar = Buf::new()
        .u32(0x0001_0000)
        .u32(20)
        .u32(0)
        .u32(0)
        .u32(0)
        .bytes(&store)
        .done();
    let mut tables = sample_tables_with_glyf();
    tables
        .iter_mut()
        .find(|(tag, _)| **tag == *b"hmtx")
        .unwrap()
        .1
        .get_mut(..2)
        .unwrap()
        .copy_from_slice(&u16::MAX.to_be_bytes());
    tables.push((b"HVAR", hvar));
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn i08_item_delta_c_abi_matches_rust_and_checks_arguments() {
    let bytes = hvar_font_bytes();
    let mut error = -99;
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coord = [1.0f32];
    let mut location = core::ptr::null_mut();
    assert_eq!(
        unsafe { sofel_location_create_design(font, coord.as_ptr(), 1, &mut location) },
        OK
    );
    let mut hb = -1.0f32;
    let mut ft = -1i32;
    assert_eq!(
        unsafe {
            sofel_instance_item_delta_hb(font, location, SOFEL_VAR_STORE_HVAR, 0, 0, &mut hb)
        },
        OK
    );
    assert_eq!(hb, 21.0);
    assert_eq!(
        unsafe {
            sofel_instance_item_delta_ft(font, location, SOFEL_VAR_STORE_HVAR, 0, 0, &mut ft)
        },
        OK
    );
    assert_eq!(ft, 21);
    assert_eq!(
        unsafe {
            sofel_instance_item_delta_hb(font, location, SOFEL_VAR_STORE_HVAR, 1, 0, &mut hb)
        },
        OK
    );
    assert_eq!(hb, 0.0);
    assert_eq!(
        unsafe {
            sofel_instance_item_delta_hb(
                font,
                core::ptr::null(),
                SOFEL_VAR_STORE_HVAR,
                0,
                0,
                &mut hb,
            )
        },
        OK
    );
    assert_eq!(hb, 0.0);
    assert_eq!(
        unsafe { sofel_instance_item_delta_hb(font, location, -1, 0, 0, &mut hb) },
        ERROR_INVALID_ARGUMENT
    );
    assert_eq!(hb, 0.0);
    assert_eq!(
        unsafe {
            sofel_instance_item_delta_hb(
                core::ptr::null(),
                location,
                SOFEL_VAR_STORE_HVAR,
                0,
                0,
                &mut hb,
            )
        },
        ERROR_NULL
    );
    assert_eq!(
        unsafe {
            sofel_instance_item_delta_ft(
                font,
                location,
                SOFEL_VAR_STORE_HVAR,
                0,
                0,
                core::ptr::null_mut(),
            )
        },
        ERROR_NULL
    );
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

fn vvar_font_bytes() -> Vec<u8> {
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .done();
    let vvar = Buf::new()
        .u32(0x0001_0000)
        .u32(24)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(56)
        .bytes(&store)
        .u8(0)
        .u8(0)
        .u16(1)
        .u8(0)
        .done();
    let mut tables = sample_tables_with_glyf();
    tables.push((b"vhea", build::hhea(1)));
    tables.push((b"vmtx", build::hmtx(&[(500, 0)], &[])));
    tables.push((b"VORG", Buf::new().u16(1).u16(0).i16(700).u16(0).done()));
    tables.push((b"fvar", fvar_one_axis()));
    tables.push((b"VVAR", vvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn mvar_font_bytes() -> Vec<u8> {
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .done();
    let mvar = Buf::new()
        .u32(0x0001_0000)
        .u16(0)
        .u16(8)
        .u16(1)
        .u16(20)
        .bytes(b"hasc")
        .u16(0)
        .u16(0)
        .bytes(&store)
        .done();
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_axis()));
    tables.push((b"MVAR", mvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn named_font_bytes() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 1, 0, 10];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    fvar.extend_from_slice(&[1, 1, 0, 0]);
    fvar.extend_from_slice(&0xC000i32.to_be_bytes());
    fvar.extend_from_slice(&258u16.to_be_bytes());
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    let prefix = utf16be("Demo");
    tables.push((b"name", name(0, &[(3, 1, 0x409, 25, &prefix)], &[])));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn i02_axis_inquiries_match_rust_view() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive until the font is destroyed.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let mut count = usize::MAX;
    // SAFETY: font is live and count is writable.
    assert_eq!(unsafe { sofel_font_var_axis_count(font, &mut count) }, OK);
    assert_eq!(count, 1);
    let mut axis = SofelVarAxisInfo::default();
    // SAFETY: font is live and axis is writable.
    assert_eq!(unsafe { sofel_font_var_axis(font, 0, &mut axis) }, OK);
    assert_eq!(
        (
            axis.index,
            axis.tag,
            axis.name_id,
            axis.min,
            axis.default,
            axis.max
        ),
        (0, u32::from_be_bytes(*b"wght"), 256, 0.0, 0.0, 1.0)
    );
    // SAFETY: font is live and axis is writable.
    assert_eq!(
        unsafe { sofel_font_find_var_axis(font, u32::from_be_bytes(*b"wght"), &mut axis) },
        OK
    );
    // SAFETY: font is live and axis is writable.
    assert_eq!(
        unsafe { sofel_font_find_var_axis(font, u32::from_be_bytes(*b"wdth"), &mut axis) },
        ERROR_NOT_FOUND
    );
    // SAFETY: font is owned and no longer used.
    unsafe { sofel_font_destroy(font) };
}

#[test]
fn i02_named_instance_inquiries_copy_design_coordinates() {
    let bytes = named_font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive until the font is destroyed.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let mut count = usize::MAX;
    // SAFETY: font is live and count is writable.
    assert_eq!(
        unsafe { sofel_font_named_instance_count(font, &mut count) },
        OK
    );
    assert_eq!(count, 1);
    let mut info = SofelNamedInstanceInfo::default();
    // SAFETY: font is live and info is writable.
    assert_eq!(unsafe { sofel_font_named_instance(font, 0, &mut info) }, OK);
    assert_eq!(
        (info.subfamily_name_id, info.postscript_name_id),
        (257, 258)
    );
    let mut coords = [0.0f32; 1];
    let mut needed = 0usize;
    // SAFETY: font is live and outputs are writable.
    assert_eq!(
        unsafe { sofel_font_named_instance_coords(font, 0, coords.as_mut_ptr(), 1, &mut needed) },
        OK
    );
    assert_eq!((coords, needed), ([0.75], 1));
    // SAFETY: font is live and outputs are writable.
    assert_eq!(
        unsafe { sofel_font_named_instance_coords(font, 0, coords.as_mut_ptr(), 0, &mut needed) },
        ERROR_BUFFER_TOO_SMALL
    );
    assert_eq!(coords, [0.75]);
    // SAFETY: font and output are valid; the index is intentionally out of range.
    assert_eq!(
        unsafe { sofel_font_named_instance(font, 1, &mut info) },
        ERROR_NOT_FOUND
    );
    // SAFETY: font is owned and no longer used.
    unsafe { sofel_font_destroy(font) };
}

#[test]
fn adr0036_i01_i02_i03_freetype_mm_var_inquiries_include_zero_style() {
    let bytes = named_font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive until the font is destroyed.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let mut count = 0usize;
    // SAFETY: font is live and count is writable.
    assert_eq!(
        unsafe { sofel_font_ft_mm_var_axis_count(font, &mut count) },
        OK
    );
    assert_eq!(count, 1);
    let mut axis = SofelFtAxisInfo::default();
    // SAFETY: font is live and axis is writable.
    assert_eq!(unsafe { sofel_font_ft_mm_var_axis(font, 0, &mut axis) }, OK);
    assert_eq!(
        (axis.tag, axis.min, axis.default, axis.max),
        (u32::from_be_bytes(*b"wght"), 0, 0, 65536)
    );
    // SAFETY: font is live and count is writable.
    assert_eq!(
        unsafe { sofel_font_ft_mm_var_named_style_count(font, &mut count) },
        OK
    );
    assert_eq!(count, 2);
    let mut style = SofelFtNamedStyleInfo::default();
    // SAFETY: font is live and style is writable.
    assert_eq!(
        unsafe { sofel_font_ft_mm_var_named_style(font, 0, &mut style) },
        OK
    );
    assert_eq!((style.strid, style.psid), (257, 258));
    let mut coords = [0i32; 1];
    let mut needed = 0usize;
    // SAFETY: font is live and outputs are writable.
    assert_eq!(
        unsafe {
            sofel_font_ft_mm_var_named_style_coords(font, 0, coords.as_mut_ptr(), 1, &mut needed)
        },
        OK
    );
    assert_eq!((coords, needed), ([0xC000], 1));
    // SAFETY: font is live and style is writable.
    assert_eq!(
        unsafe { sofel_font_ft_mm_var_named_style(font, 1, &mut style) },
        OK
    );
    assert_eq!((style.strid, style.psid), (0, 0));
    // SAFETY: font is live and outputs are writable.
    assert_eq!(
        unsafe {
            sofel_font_ft_mm_var_named_style_coords(font, 1, coords.as_mut_ptr(), 1, &mut needed)
        },
        OK
    );
    assert_eq!((coords, needed), ([0], 1));
    let mut default_index = u32::MAX;
    // SAFETY: font is live and output is writable.
    assert_eq!(
        unsafe { sofel_font_ft_default_named_instance(font, &mut default_index) },
        OK
    );
    assert_eq!(default_index, 2);
    // SAFETY: font is owned and no longer used.
    unsafe { sofel_font_destroy(font) };
}

#[test]
fn i02_postscript_name_uses_location() {
    let bytes = named_font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for the font and location.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and input/output pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut out = [0u8; 128];
    let mut needed = 0usize;
    // SAFETY: font, location, and output pointers are valid.
    assert_eq!(
        unsafe {
            sofel_font_postscript_name(font, location, out.as_mut_ptr(), out.len(), &mut needed)
        },
        OK
    );
    assert_eq!(out.get(..needed), Some(b"Demo_.5wght".as_slice()));
    // SAFETY: the handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_h_advance_accepts_location() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for both handles.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: the font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut advance = 0u32;
    // SAFETY: font and location are live; advance is writable.
    assert_eq!(
        unsafe { sofel_font_h_advance(font, location, 0, &mut advance) },
        OK
    );
    assert_eq!(advance, 500);
    let mut ft_advance = u16::MAX;
    // SAFETY: font and location are live; advance is writable.
    assert_eq!(
        unsafe { sofel_font_ft_h_advance(font, location, 0, &mut ft_advance) },
        OK
    );
    assert_eq!(ft_advance, 500);
    let mut bearing = i16::MIN;
    // SAFETY: font and location are live; bearing is writable.
    assert_eq!(
        unsafe { sofel_font_ft_left_side_bearing(font, location, 0, &mut bearing) },
        OK
    );
    assert_ne!(bearing, i16::MIN);
    let mut vertical = i32::MIN;
    // SAFETY: font and location are live; vertical is writable.
    assert_eq!(
        unsafe { sofel_font_v_advance(font, location, 0, &mut vertical) },
        OK
    );
    assert_ne!(vertical, i32::MIN);
    // SAFETY: font is live; NULL means the default location.
    assert_eq!(
        unsafe { sofel_font_h_advance(font, core::ptr::null(), 0, &mut advance) },
        OK
    );
    assert_eq!(advance, 500);
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_hvar_advance_preserves_unsigned_range_in_c_abi() {
    let bytes = hvar_font_bytes();
    let mut error = -99;
    // SAFETY: bytes remain valid through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [1.0f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut advance = 0u32;
    // SAFETY: font and location are live; advance is writable.
    assert_eq!(
        unsafe { sofel_font_h_advance(font, location, 0, &mut advance) },
        OK
    );
    assert_eq!(advance, 65_556);
    let mut ft_advance = u16::MAX;
    // SAFETY: font and location are live; advance is writable.
    assert_eq!(
        unsafe { sofel_font_ft_h_advance(font, location, 0, &mut ft_advance) },
        OK
    );
    assert_eq!(ft_advance, 20);
    // SAFETY: font remains live until scale destruction.
    let scale = unsafe { sofel_scale_create(font, 1000, 1000, &mut error) };
    assert_eq!(error, OK);
    let mut scaled = i32::MIN;
    // SAFETY: all handles are live; scaled is writable.
    assert_eq!(
        unsafe { sofel_scale_h_advance(scale, font, location, 0, &mut scaled) },
        OK
    );
    assert_eq!(scaled, 65_556);
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_scale_destroy(scale);
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_vvar_advance_uses_location_in_c_abi() {
    let bytes = vvar_font_bytes();
    let mut error = -99;
    // SAFETY: bytes remain valid through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut advance = i32::MIN;
    // SAFETY: font and location are live; advance is writable.
    assert_eq!(
        unsafe { sofel_font_v_advance(font, location, 0, &mut advance) },
        OK
    );
    assert_eq!(advance, -511);
    let mut ft_advance = u16::MAX;
    // SAFETY: font and location are live; ft_advance is writable.
    assert_eq!(
        unsafe { sofel_font_ft_v_advance(font, location, 0, &mut ft_advance) },
        OK
    );
    assert_eq!(ft_advance, 511);
    let mut top = i16::MIN;
    // SAFETY: font and location are live; top is writable.
    assert_eq!(
        unsafe { sofel_font_ft_top_side_bearing(font, location, 0, 123, &mut top) },
        OK
    );
    assert_eq!(top, 0);
    let mut origin = [i32::MIN; 2];
    // SAFETY: font and location are live; origin is writable.
    assert_eq!(
        unsafe { sofel_font_v_origin(font, location, 0, origin.as_mut_ptr()) },
        OK
    );
    assert_eq!(origin, [250, 711]);
    // SAFETY: font remains live until scale destruction.
    let scale = unsafe { sofel_scale_create(font, 1000, 1000, &mut error) };
    assert_eq!(error, OK);
    // SAFETY: all handles are live; advance is writable.
    assert_eq!(
        unsafe { sofel_scale_v_advance(scale, font, location, 0, &mut advance) },
        OK
    );
    assert_eq!(advance, -511);
    origin = [i32::MIN; 2];
    // SAFETY: all handles are live; origin is writable.
    assert_eq!(
        unsafe { sofel_scale_v_origin(scale, font, location, 0, origin.as_mut_ptr()) },
        OK
    );
    assert_eq!(origin, [250, 711]);
    // SAFETY: all handles are owned and no longer used.
    unsafe {
        sofel_scale_destroy(scale);
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_mvar_metric_uses_location_at_scale_in_c_abi() {
    let bytes = mvar_font_bytes();
    let mut error = -99;
    // SAFETY: bytes remain valid through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [1.0f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let tag = u32::from_be_bytes(*b"hasc");
    let mut position = i32::MIN;
    // SAFETY: font and location are live; position is writable.
    assert_eq!(
        unsafe { sofel_font_metrics_position(font, location, tag, &mut position) },
        OK
    );
    assert_eq!(position, 821);
    let mut extents = [i32::MIN; 3];
    // SAFETY: font and location are live; extents is writable.
    assert_eq!(
        unsafe { sofel_font_h_extents(font, location, extents.as_mut_ptr()) },
        OK
    );
    assert_eq!(extents[0], 821);
    let mut ft_metrics = [i32::MIN; 11];
    // SAFETY: font and location are live; ft_metrics is writable.
    assert_eq!(
        unsafe { sofel_font_ft_face_metrics(font, location, ft_metrics.as_mut_ptr()) },
        OK
    );
    assert_eq!(ft_metrics[0], 821);
    // SAFETY: font remains live until scale destruction.
    let scale = unsafe { sofel_scale_create(font, 1000, 1000, &mut error) };
    assert_eq!(error, OK);
    position = i32::MIN;
    // SAFETY: all handles are live; position is writable.
    assert_eq!(
        unsafe { sofel_scale_metrics_position(scale, font, location, tag, &mut position) },
        OK
    );
    assert_eq!(position, 821);
    extents = [i32::MIN; 3];
    // SAFETY: all handles are live; extents is writable.
    assert_eq!(
        unsafe { sofel_scale_h_extents(scale, font, location, extents.as_mut_ptr()) },
        OK
    );
    assert_eq!(extents[0], 821);
    // SAFETY: all handles are owned and no longer used.
    unsafe {
        sofel_scale_destroy(scale);
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_font_extents_accept_location() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for both handles.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut h = [0i32; 3];
    let mut v = [0i32; 3];
    // SAFETY: the font and location are live, outputs are writable.
    assert_eq!(
        unsafe { sofel_font_h_extents(font, location, h.as_mut_ptr()) },
        OK
    );
    // SAFETY: the font and location are live, outputs are writable.
    assert_eq!(
        unsafe { sofel_font_v_extents(font, location, v.as_mut_ptr()) },
        OK
    );
    assert_ne!(h, [0; 3]);
    assert_ne!(v, [0; 3]);
    let mut origin = [i32::MIN; 2];
    // SAFETY: font and location are live, output is writable.
    assert_eq!(
        unsafe { sofel_font_v_origin(font, location, 0, origin.as_mut_ptr()) },
        OK
    );
    assert_ne!(origin, [i32::MIN; 2]);
    let mut glyph_extents = [i32::MIN; 4];
    // SAFETY: font and location are live, output is writable.
    assert_eq!(
        unsafe { sofel_font_glyph_extents(font, location, 0, glyph_extents.as_mut_ptr()) },
        OK
    );
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_metric_position_accepts_location() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for both handles.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut value = i32::MIN;
    let tag = u32::from_be_bytes(*b"hasc");
    // SAFETY: font and location are live; value is writable.
    let status = unsafe { sofel_font_metrics_position(font, location, tag, &mut value) };
    assert!(matches!(status, OK | ERROR_NOT_FOUND));
    // SAFETY: font and location are live; value is writable.
    assert_eq!(
        unsafe { sofel_font_metrics_position_with_fallback(font, location, tag, &mut value) },
        OK
    );
    assert_ne!(value, i32::MIN);
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_baseline_and_layout_extents_accept_location() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for both handles.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut baseline = i32::MIN;
    // SAFETY: font and location are live; baseline is writable.
    assert_eq!(
        unsafe {
            sofel_font_baseline_with_fallback(
                font,
                location,
                u32::from_be_bytes(*b"romn"),
                0,
                u32::from_be_bytes(*b"latn"),
                &mut baseline,
            )
        },
        OK
    );
    assert_ne!(baseline, i32::MIN);
    let mut extents = [i32::MIN; 3];
    let mut found = -1i32;
    // SAFETY: font and location are live; both outputs are writable.
    assert_eq!(
        unsafe {
            sofel_font_layout_font_extents(
                font,
                location,
                0,
                u32::from_be_bytes(*b"latn"),
                0,
                extents.as_mut_ptr(),
                &mut found,
            )
        },
        OK
    );
    assert_ne!(extents, [i32::MIN; 3]);
    // SAFETY: both handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_scaled_advance_accepts_location() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive for all handles.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    // SAFETY: font stays live until scale destruction.
    let scale = unsafe { sofel_scale_create(font, 1000, 1000, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut advance = i32::MIN;
    // SAFETY: all handles are live and output is writable.
    assert_eq!(
        unsafe { sofel_scale_h_advance(scale, font, location, 0, &mut advance) },
        OK
    );
    assert_ne!(advance, i32::MIN);
    // SAFETY: all handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_scale_destroy(scale);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_freetype_metrics_accept_location() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes stay alive through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [0.5f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: font and all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut values = [i32::MIN; 11];
    // SAFETY: handles are live and values are writable.
    assert_eq!(
        unsafe { sofel_font_ft_face_metrics(font, location, values.as_mut_ptr()) },
        OK
    );
    assert_ne!(values, [i32::MIN; 11]);
    let mut advance = u16::MAX;
    // SAFETY: handles are live and advance is writable.
    assert_eq!(
        unsafe { sofel_font_ft_v_advance(font, location, 0, &mut advance) },
        OK
    );
    assert_ne!(advance, u16::MAX);
    // SAFETY: handles are owned and no longer used.
    unsafe {
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn i03_location_keeps_default_outline_and_metrics_until_values_are_applied() {
    let bytes = font_bytes();
    let mut error = -99;
    // SAFETY: bytes remain valid through handle destruction.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let coords = [1.0f32];
    let mut location = core::ptr::null_mut();
    // SAFETY: all pointers are valid.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let mut default_advance = u32::MAX;
    let mut located_advance = u32::MAX;
    // SAFETY: font and location are live; outputs are writable.
    assert_eq!(
        unsafe { sofel_font_h_advance(font, core::ptr::null(), 1, &mut default_advance) },
        OK
    );
    // SAFETY: font and location are live; outputs are writable.
    assert_eq!(
        unsafe { sofel_font_h_advance(font, location, 1, &mut located_advance) },
        OK
    );
    assert_eq!(located_advance, default_advance);
    let scratch = sofel_outline_scratch_create();
    assert!(!scratch.is_null());
    let mut default_box = [i32::MIN; 4];
    let mut located_box = [i32::MIN; 4];
    let mut overlap = u32::MAX;
    // SAFETY: handles are live; box and overlap are writable.
    assert_eq!(
        unsafe {
            sofel_outline_load(
                font,
                core::ptr::null(),
                1,
                0,
                0,
                0,
                scratch,
                default_box.as_mut_ptr(),
                &mut overlap,
            )
        },
        OK
    );
    // SAFETY: handles are live; box and overlap are writable.
    assert_eq!(
        unsafe {
            sofel_outline_load(
                font,
                location,
                1,
                0,
                0,
                0,
                scratch,
                located_box.as_mut_ptr(),
                &mut overlap,
            )
        },
        OK
    );
    assert_eq!(located_box, default_box);
    // SAFETY: handles are owned and no longer used.
    unsafe {
        sofel_outline_scratch_destroy(scratch);
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
}

#[test]
fn adr0027_i03_location_changes_glyf_outline_and_box() {
    let root = std::env::var_os("SOFEL_TESTDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata")
        });
    let bytes = std::fs::read(root.join("extra-fonts/fonts/arabic.ttf")).unwrap();
    let mut error = -99;
    // SAFETY: bytes live through both handle destructions.
    let font = unsafe { sofel_font_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert_eq!(error, OK);
    let mut location = core::ptr::null_mut();
    let coords = [900.0f32];
    // SAFETY: font and output pointer are live; one coordinate is readable.
    assert_eq!(
        unsafe { sofel_location_create_design(font, coords.as_ptr(), 1, &mut location) },
        OK
    );
    let scratch = sofel_outline_scratch_create();
    let mut default_box = [0; 4];
    let mut located_box = [0; 4];
    let mut overlap = 0;
    // SAFETY: handles are live; outputs are writable.
    assert_eq!(
        unsafe {
            sofel_outline_load(
                font,
                core::ptr::null(),
                5,
                0,
                0,
                0,
                scratch,
                default_box.as_mut_ptr(),
                &mut overlap,
            )
        },
        OK
    );
    // SAFETY: handles are live; outputs are writable.
    assert_eq!(
        unsafe {
            sofel_outline_load(
                font,
                location,
                5,
                0,
                0,
                0,
                scratch,
                located_box.as_mut_ptr(),
                &mut overlap,
            )
        },
        OK
    );
    assert_ne!(located_box, default_box);
    let mut default_extents = [0; 4];
    let mut located_extents = [0; 4];
    // SAFETY: handles are live; output is writable.
    assert_eq!(
        unsafe {
            sofel_font_glyph_extents(font, core::ptr::null(), 5, default_extents.as_mut_ptr())
        },
        OK
    );
    // SAFETY: handles are live; output is writable.
    assert_eq!(
        unsafe { sofel_font_glyph_extents(font, location, 5, located_extents.as_mut_ptr()) },
        OK
    );
    assert_ne!(located_extents, default_extents);
    assert_eq!(located_extents, [10, 589, 470, -529]);
    let rasterizer = sofel_rasterizer_create();
    let mut default_pixels = vec![0u8; 65536];
    let mut located_pixels = vec![0u8; 65536];
    let mut default_len = 0;
    let mut located_len = 0;
    let mut default_bitmap_box = [0; 4];
    let mut located_bitmap_box = [0; 4];
    // SAFETY: all handles are live; buffers and output pointers are writable.
    assert_eq!(
        unsafe {
            sofel_render_glyph(
                font,
                core::ptr::null(),
                5,
                24,
                scratch,
                rasterizer,
                default_pixels.as_mut_ptr(),
                default_pixels.len(),
                &mut default_len,
                default_bitmap_box.as_mut_ptr(),
            )
        },
        OK
    );
    // SAFETY: all handles are live; buffers and output pointers are writable.
    assert_eq!(
        unsafe {
            sofel_render_glyph(
                font,
                location,
                5,
                24,
                scratch,
                rasterizer,
                located_pixels.as_mut_ptr(),
                located_pixels.len(),
                &mut located_len,
                located_bitmap_box.as_mut_ptr(),
            )
        },
        OK
    );
    assert_ne!(
        (
            default_len,
            default_bitmap_box,
            default_pixels.get(..default_len)
        ),
        (
            located_len,
            located_bitmap_box,
            located_pixels.get(..located_len)
        )
    );
    // SAFETY: handles are live and owned.
    unsafe {
        sofel_rasterizer_destroy(rasterizer);
        sofel_outline_scratch_destroy(scratch);
        sofel_location_destroy(location);
        sofel_font_destroy(font);
    }
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
