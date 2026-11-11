//! Location and variation string C ABI (ADR 0027).

use super::*;
use sofel_font::read::Fixed as ReadFixed;
use sofel_font::var::{
    BlendError, Ft, FtAxisInfo, FtLocationError, HbF32, Location, StoreKind, VarAxisInfo, Variation,
};

/// `SOFEL_VAR_STORE_HVAR`.
pub const SOFEL_VAR_STORE_HVAR: i32 = 0;
/// `SOFEL_VAR_STORE_VVAR`.
pub const SOFEL_VAR_STORE_VVAR: i32 = 1;
/// `SOFEL_VAR_STORE_MVAR`.
pub const SOFEL_VAR_STORE_MVAR: i32 = 2;
/// `SOFEL_VAR_STORE_GDEF`.
pub const SOFEL_VAR_STORE_GDEF: i32 = 3;
/// `SOFEL_VAR_STORE_BASE`.
pub const SOFEL_VAR_STORE_BASE: i32 = 4;
/// `SOFEL_VAR_STORE_COLR`.
pub const SOFEL_VAR_STORE_COLR: i32 = 5;
/// `SOFEL_VAR_STORE_CFF2`.
pub const SOFEL_VAR_STORE_CFF2: i32 = 6;

fn store_kind(value: i32) -> Result<StoreKind, i32> {
    match value {
        SOFEL_VAR_STORE_HVAR => Ok(StoreKind::Hvar),
        SOFEL_VAR_STORE_VVAR => Ok(StoreKind::Vvar),
        SOFEL_VAR_STORE_MVAR => Ok(StoreKind::Mvar),
        SOFEL_VAR_STORE_GDEF => Ok(StoreKind::Gdef),
        SOFEL_VAR_STORE_BASE => Ok(StoreKind::Base),
        SOFEL_VAR_STORE_COLR => Ok(StoreKind::Colr),
        SOFEL_VAR_STORE_CFF2 => Ok(StoreKind::Cff2),
        _ => Err(ERROR_INVALID_ARGUMENT),
    }
}

/// `sofel_instance_item_delta_hb`.
///
/// # Safety
///
/// `font` is NULL or live; `location` is NULL for the default or live;
/// `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_instance_item_delta_hb(
    font: *const SofelFont,
    location: *const SofelLocation,
    store: i32,
    outer: u16,
    inner: u16,
    out: *mut f32,
) -> i32 {
    // SAFETY: the caller guarantees the input and output pointers.
    let (font, location, out) = unsafe { (font_ref(font), r(location), w(out)) };
    status((|| {
        let (font, out) = (some(font)?, some(out)?);
        let kind = store_kind(store)?;
        let location = location.map_or_else(Location::default_ref, |value| &value.location);
        *out = font
            .instance(location)
            .item_delta::<HbF32>(kind, outer, inner)
            .map_err(|_| ERROR_INVALID_ARGUMENT)?;
        Ok(())
    })())
}

/// `sofel_instance_item_delta_ft`.
///
/// # Safety
///
/// `font` is NULL or live; `location` is NULL for the default or live;
/// `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_instance_item_delta_ft(
    font: *const SofelFont,
    location: *const SofelLocation,
    store: i32,
    outer: u16,
    inner: u16,
    out: *mut i32,
) -> i32 {
    // SAFETY: the caller guarantees the input and output pointers.
    let (font, location, out) = unsafe { (font_ref(font), r(location), w(out)) };
    status((|| {
        let (font, out) = (some(font)?, some(out)?);
        let kind = store_kind(store)?;
        let location = location.map_or_else(Location::default_ref, |value| &value.location);
        *out = font
            .instance(location)
            .item_delta::<Ft>(kind, outer, inner)
            .map_err(|_| ERROR_INVALID_ARGUMENT)?;
        Ok(())
    })())
}

/// `sofel_location_t`: immutable coordinates and cached variation scalars.
pub struct SofelLocation {
    pub(crate) location: Location<'static>,
}

/// `sofel_cff2_blend_vector`: FreeType's BV, including the leading 1.0.
///
/// # Safety
///
/// `font` and `location` are NULL or live handles. `out` is NULL with
/// capacity zero or has `capacity` writable elements; `len` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_cff2_blend_vector(
    font: *const SofelFont,
    location: *const SofelLocation,
    vsindex: u16,
    out: *mut i32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's pointer conditions match these helpers.
    let (font, location, out, len) = unsafe {
        (
            font_ref(font),
            r(location),
            out_array(out, capacity),
            w(len),
        )
    };
    status((|| {
        let (font, out, len) = (some(font)?, some(out)?, some(len)?);
        let default = Location::default_ref();
        let location = location.map_or(default, |value| &value.location);
        let columns = font
            .instance(location)
            .blend_scalars::<Ft>(vsindex)
            .map_err(|error| match error {
                BlendError::NoStore | BlendError::VsindexOutOfRange => ERROR_INVALID_ARGUMENT,
                BlendError::InvalidStore
                | BlendError::AxisCountMismatch
                | BlendError::RegionOutOfRange => ERROR_INVALID_FONT,
                BlendError::FtLocation(_) => ERROR_INVALID_ARGUMENT,
            })?;
        *len = columns.len().saturating_add(1);
        let dst = out.get_mut(..*len).ok_or(ERROR_BUFFER_TOO_SMALL)?;
        let (first, columns_out) = dst.split_first_mut().ok_or(ERROR_BUFFER_TOO_SMALL)?;
        *first = 0x10000;
        for (slot, scalar) in columns_out.iter_mut().zip(columns.iter()) {
            *slot = scalar;
        }
        Ok(())
    })())
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

/// `sofel_var_axis_info_t`: HarfBuzz-compatible design axis information.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SofelVarAxisInfo {
    /// Zero-based axis index.
    pub index: u32,
    /// Four-byte axis tag.
    pub tag: u32,
    /// Axis name ID.
    pub name_id: u16,
    /// Bit 0 marks a hidden axis.
    pub flags: u16,
    /// Ordered minimum design value.
    pub min: f32,
    /// Default design value.
    pub default: f32,
    /// Ordered maximum design value.
    pub max: f32,
}

/// `sofel_named_instance_info_t`: one fvar record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SofelNamedInstanceInfo {
    /// Subfamily name ID.
    pub subfamily_name_id: u16,
    /// PostScript name ID, or 0xFFFF when absent.
    pub postscript_name_id: u16,
}

/// `sofel_ft_axis_info_t`: one FreeType MM axis.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SofelFtAxisInfo {
    /// Four-byte axis tag.
    pub tag: u32,
    /// Raw axis name ID.
    pub name_id: u16,
    /// `FT_Get_Var_Axis_Flags` value.
    pub flags: u16,
    /// Minimum design value in 16.16.
    pub min: i32,
    /// Default design value in 16.16.
    pub default: i32,
    /// Maximum design value in 16.16.
    pub max: i32,
}

impl From<FtAxisInfo> for SofelFtAxisInfo {
    fn from(value: FtAxisInfo) -> Self {
        Self {
            tag: value.tag.to_u32(),
            name_id: value.name_id,
            flags: value.flags,
            min: value.min.0,
            default: value.default.0,
            max: value.max.0,
        }
    }
}

/// `sofel_ft_named_style_info_t`: one FreeType MM named style.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SofelFtNamedStyleInfo {
    /// Subfamily string ID.
    pub strid: u16,
    /// PostScript string ID, or 0xFFFF when absent.
    pub psid: u16,
}

impl From<VarAxisInfo> for SofelVarAxisInfo {
    fn from(value: VarAxisInfo) -> Self {
        Self {
            index: value.index,
            tag: value.tag.to_u32(),
            name_id: value.name_id,
            flags: u16::from(value.hidden),
            min: value.min,
            default: value.default,
            max: value.max,
        }
    }
}

/// `sofel_font_var_axis_count`.
///
/// # Safety
///
/// `font` is NULL or live, and `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_var_axis_count(
    font: *const SofelFont,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, |f| f.var_axes().count()))
}

/// `sofel_font_var_axis`.
///
/// # Safety
///
/// `font` is NULL or live, and `axis` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_var_axis(
    font: *const SofelFont,
    index: usize,
    axis: *mut SofelVarAxisInfo,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, axis) = unsafe { (font_ref(font), w(axis)) };
    status(find(font, axis, |f| {
        f.var_axes().nth(index).map(Into::into)
    }))
}

/// `sofel_font_find_var_axis`.
///
/// # Safety
///
/// `font` is NULL or live, and `axis` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_find_var_axis(
    font: *const SofelFont,
    tag: u32,
    axis: *mut SofelVarAxisInfo,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, axis) = unsafe { (font_ref(font), w(axis)) };
    status(find(font, axis, |f| {
        f.find_var_axis(Tag::from_u32(tag)).map(Into::into)
    }))
}

/// `sofel_font_named_instance_count`.
///
/// # Safety
///
/// `font` is NULL or live, and `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_named_instance_count(
    font: *const SofelFont,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, |f| f.named_instances().count()))
}

/// `sofel_font_named_instance`.
///
/// # Safety
///
/// `font` is NULL or live, and `info` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_named_instance(
    font: *const SofelFont,
    index: usize,
    info: *mut SofelNamedInstanceInfo,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, info) = unsafe { (font_ref(font), w(info)) };
    status(find(font, info, |f| {
        f.named_instances()
            .nth(index)
            .map(|record| SofelNamedInstanceInfo {
                subfamily_name_id: record.subfamily_name_id,
                postscript_name_id: record.postscript_name_id.unwrap_or(0xFFFF),
            })
    }))
}

/// `sofel_font_named_instance_coords`.
///
/// # Safety
///
/// `font` is NULL or live; `out` has `capacity` writable floats or is NULL
/// with zero capacity; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_named_instance_coords(
    font: *const SofelFont,
    index: usize,
    out: *mut f32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees all input and output pointers.
    let (font, out, len) = unsafe { (font_ref(font), out_array(out, capacity), w(len)) };
    status((|| {
        let (font, out, len) = (some(font)?, some(out)?, some(len)?);
        let record = font.named_instances().nth(index).ok_or(ERROR_NOT_FOUND)?;
        let count = record.design_coords().count();
        *len = count;
        if out.len() < count {
            return Err(ERROR_BUFFER_TOO_SMALL);
        }
        for (slot, coord) in out.iter_mut().zip(record.design_coords()) {
            *slot = coord;
        }
        Ok(())
    })())
}

/// `sofel_font_ft_mm_var_axis_count`.
///
/// # Safety
///
/// `font` is NULL or live, and `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_mm_var_axis_count(
    font: *const SofelFont,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status((|| {
        let (font, count) = (some(font)?, some(count)?);
        *count = font.ft_mm_var().ok_or(ERROR_NOT_FOUND)?.axes().count();
        Ok(())
    })())
}

/// `sofel_font_ft_mm_var_axis`.
///
/// # Safety
///
/// `font` is NULL or live, and `axis` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_mm_var_axis(
    font: *const SofelFont,
    index: usize,
    axis: *mut SofelFtAxisInfo,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, axis) = unsafe { (font_ref(font), w(axis)) };
    status((|| {
        let (font, axis) = (some(font)?, some(axis)?);
        *axis = font
            .ft_mm_var()
            .ok_or(ERROR_NOT_FOUND)?
            .axes()
            .nth(index)
            .ok_or(ERROR_NOT_FOUND)?
            .into();
        Ok(())
    })())
}

/// `sofel_font_ft_mm_var_named_style_count`.
///
/// # Safety
///
/// `font` is NULL or live, and `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_mm_var_named_style_count(
    font: *const SofelFont,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status((|| {
        let (font, count) = (some(font)?, some(count)?);
        *count = font
            .ft_mm_var()
            .ok_or(ERROR_NOT_FOUND)?
            .named_styles()
            .count();
        Ok(())
    })())
}

/// `sofel_font_ft_mm_var_named_style`.
///
/// # Safety
///
/// `font` is NULL or live, and `style` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_mm_var_named_style(
    font: *const SofelFont,
    index: usize,
    style: *mut SofelFtNamedStyleInfo,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, style) = unsafe { (font_ref(font), w(style)) };
    status((|| {
        let (font, style) = (some(font)?, some(style)?);
        let record = font
            .ft_mm_var()
            .ok_or(ERROR_NOT_FOUND)?
            .named_styles()
            .nth(index)
            .ok_or(ERROR_NOT_FOUND)?;
        *style = SofelFtNamedStyleInfo {
            strid: record.strid,
            psid: record.psid,
        };
        Ok(())
    })())
}

/// `sofel_font_ft_mm_var_named_style_coords`.
///
/// # Safety
///
/// `font` is NULL or live; `out` has `capacity` writable integers or is NULL
/// with zero capacity; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_mm_var_named_style_coords(
    font: *const SofelFont,
    index: usize,
    out: *mut i32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller guarantees all input and output pointers.
    let (font, out, len) = unsafe { (font_ref(font), out_array(out, capacity), w(len)) };
    status((|| {
        let (font, out, len) = (some(font)?, some(out)?, some(len)?);
        let record = font
            .ft_mm_var()
            .ok_or(ERROR_NOT_FOUND)?
            .named_styles()
            .nth(index)
            .ok_or(ERROR_NOT_FOUND)?;
        let count = record.design_coords().count();
        *len = count;
        if out.len() < count {
            return Err(ERROR_BUFFER_TOO_SMALL);
        }
        for (slot, coord) in out.iter_mut().zip(record.design_coords()) {
            *slot = coord.0;
        }
        Ok(())
    })())
}

/// `sofel_font_ft_default_named_instance`.
///
/// # Safety
///
/// `font` is NULL or live, and `index` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_ft_default_named_instance(
    font: *const SofelFont,
    index: *mut u32,
) -> i32 {
    // SAFETY: the caller guarantees the pointers.
    let (font, index) = unsafe { (font_ref(font), w(index)) };
    status((|| {
        let (font, index) = (some(font)?, some(index)?);
        *index = u32::from(
            font.ft_mm_var()
                .ok_or(ERROR_NOT_FOUND)?
                .default_named_instance()
                .unwrap_or(0),
        );
        Ok(())
    })())
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
