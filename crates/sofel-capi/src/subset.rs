//! The subsetting functions of the C ABI (ADR 0022): inputs, plans,
//! subsetting, bytes that sofel allocates, and the serialization of
//! caller-built objects.
//!
//! A `sofel_subset_input_t` holds an [`Input`], an immutable Rust value;
//! the functions that change it replace it with the changed value, as
//! HarfBuzz changes an `hb_subset_input_t` in place.

use sofel_font::read::Tag;
use sofel_subset::{AxisRange, Flags, INVALID, Input, Plan, RangeSet, SetKind, SubsetError};
use sofel_write::{Link, Object, Whence, WriteError};

use crate::{
    ERROR_BUFFER_TOO_SMALL, ERROR_INVALID_ARGUMENT, ERROR_NOT_FOUND, SofelFont, Status, copy_out,
    out_array, r, some, status, w,
};

/// `SOFEL_ERROR_NO_GLYPHS`.
pub const ERROR_NO_GLYPHS: i32 = -15;
/// `SOFEL_ERROR_INVALID_INPUT`.
pub const ERROR_INVALID_INPUT: i32 = -16;
/// `SOFEL_ERROR_MALFORMED_TABLE`.
pub const ERROR_MALFORMED_TABLE: i32 = -17;
/// `SOFEL_ERROR_UNSUPPORTED`.
pub const ERROR_UNSUPPORTED: i32 = -18;
/// `SOFEL_ERROR_OFFSET_OVERFLOW`.
pub const ERROR_OFFSET_OVERFLOW: i32 = -19;
/// `SOFEL_ERROR_LIMIT`.
pub const ERROR_LIMIT: i32 = -20;
/// `SOFEL_ERROR_SUBSET_FAILED`.
pub const ERROR_SUBSET_FAILED: i32 = -21;

/// `sofel_bytes_t`: bytes from `Box<[u8]>::into_raw`.
#[repr(C)]
pub struct SofelBytes {
    data: *const u8,
    len: usize,
}

/// `sofel_subset_input_t`.
pub struct SofelSubsetInput {
    input: Input,
}

/// `sofel_subset_plan_t`.
pub struct SofelSubsetPlan {
    plan: Plan,
}

/// `sofel_subset_prepared_t`: none can be made yet (ADR 0022 stage 11).
pub enum SofelSubsetPrepared {}

/// `sofel_serialize_link_t`.
#[repr(C)]
pub struct SofelSerializeLink {
    width: u32,
    position: u32,
    objidx: u32,
    is_signed: u32,
    whence: u32,
    bias: u32,
}

/// `sofel_serialize_object_t`.
#[repr(C)]
pub struct SofelSerializeObject {
    data: *const u8,
    len: usize,
    real_links: *const SofelSerializeLink,
    num_real_links: usize,
    virtual_links: *const SofelSerializeLink,
    num_virtual_links: usize,
}

/// The C error number of a subsetting error.
pub(crate) fn subset_error(e: SubsetError) -> i32 {
    match e {
        SubsetError::NoGlyphs => ERROR_NO_GLYPHS,
        SubsetError::InvalidInput => ERROR_INVALID_INPUT,
        SubsetError::Malformed { .. } => ERROR_MALFORMED_TABLE,
        SubsetError::Unsupported { .. } => ERROR_UNSUPPORTED,
        SubsetError::OffsetOverflow { .. } => ERROR_OFFSET_OVERFLOW,
        SubsetError::Limit { .. } => ERROR_LIMIT,
        SubsetError::Failed { .. } => ERROR_SUBSET_FAILED,
    }
}

/// `len` elements at `p`; NULL is an empty array when `len` is 0.
///
/// # Safety
///
/// `p` is NULL or valid for reads of `len` elements that nothing mutates
/// during `'a`.
unsafe fn in_array<'a, T>(p: *const T, len: usize) -> Option<&'a [T]> {
    if p.is_null() {
        return (len == 0).then_some(&[]);
    }
    // SAFETY: `p` is not NULL, and the caller guarantees the rest.
    Some(unsafe { core::slice::from_raw_parts(p, len) })
}

// --------------------------------------------------------------- bytes

/// Hands `v` to C as a `sofel_bytes_t`.
fn give(v: Vec<u8>, out: &mut SofelBytes) {
    let len = v.len();
    let data = Box::into_raw(v.into_boxed_slice()).cast::<u8>();
    *out = SofelBytes { data, len };
}

/// Writes `result` to `out` (bytes or an error).
fn give_result(result: Result<Vec<u8>, i32>, out: Option<&mut SofelBytes>) -> Status {
    let out = some(out)?;
    give(result?, out);
    Ok(())
}

/// `sofel_bytes_destroy`.
///
/// # Safety
///
/// `bytes` is NULL or points to bytes from sofel not yet destroyed, or to
/// `{NULL, 0}`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_bytes_destroy(bytes: *mut SofelBytes) {
    // SAFETY: the caller's conditions.
    let Some(b) = (unsafe { w(bytes) }) else {
        return;
    };
    if !b.data.is_null() {
        let slice = core::ptr::slice_from_raw_parts_mut(b.data.cast_mut(), b.len);
        // SAFETY: data and len came from Box::into_raw of a boxed slice in
        // `give` and are dropped once: they are cleared below.
        drop(unsafe { Box::from_raw(slice) });
    }
    *b = SofelBytes {
        data: core::ptr::null(),
        len: 0,
    };
}

// --------------------------------------------------------------- input

/// `sofel_subset_input_create`.
///
/// # Safety
///
/// `input` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_create(input: *mut *mut SofelSubsetInput) -> i32 {
    // SAFETY: the caller's conditions.
    let out = unsafe { w(input) };
    status(some(out).map(|o| {
        *o = Box::into_raw(Box::new(SofelSubsetInput {
            input: Input::new(),
        }));
    }))
}

/// `sofel_subset_input_destroy`.
///
/// # Safety
///
/// `input` is NULL or from `sofel_subset_input_create`, not yet
/// destroyed, that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_destroy(input: *mut SofelSubsetInput) {
    if !input.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(input) });
    }
}

/// Replaces the input with `f` of it.
fn change(input: Option<&mut SofelSubsetInput>, f: impl FnOnce(Input) -> Input) -> Status {
    let i = some(input)?;
    i.input = f(core::mem::take(&mut i.input));
    Ok(())
}

/// `sofel_subset_input_keep_everything`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_keep_everything(input: *mut SofelSubsetInput) -> i32 {
    // SAFETY: the caller's conditions.
    status(change(unsafe { w(input) }, Input::keep_everything))
}

fn set_kind(set: u32) -> Result<SetKind, i32> {
    SetKind::from_u32(set).ok_or(ERROR_INVALID_ARGUMENT)
}

/// Changes the set `set` of the input with `f`.
fn change_set(
    input: Option<&mut SofelSubsetInput>,
    set: u32,
    f: impl FnOnce(&mut RangeSet),
) -> Status {
    let kind = set_kind(set)?;
    change(input, |i| i.map_set(kind, f))
}

/// Reads the set `set` of the input.
fn read_set<T>(
    input: Option<&SofelSubsetInput>,
    set: u32,
    f: impl FnOnce(&RangeSet) -> T,
) -> Result<T, i32> {
    let kind = set_kind(set)?;
    Ok(f(some(input)?.input.set(kind)))
}

/// `sofel_subset_input_set_add`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_add(
    input: *mut SofelSubsetInput,
    set: u32,
    value: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change_set(unsafe { w(input) }, set, |s| s.add(value)))
}

/// `sofel_subset_input_set_add_range`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_add_range(
    input: *mut SofelSubsetInput,
    set: u32,
    first: u32,
    last: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change_set(unsafe { w(input) }, set, |s| {
        s.add_range(first, last);
    }))
}

/// `sofel_subset_input_set_remove`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_remove(
    input: *mut SofelSubsetInput,
    set: u32,
    value: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change_set(unsafe { w(input) }, set, |s| s.remove(value)))
}

/// `sofel_subset_input_set_clear`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_clear(
    input: *mut SofelSubsetInput,
    set: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change_set(unsafe { w(input) }, set, RangeSet::clear))
}

/// `sofel_subset_input_set_invert`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_invert(
    input: *mut SofelSubsetInput,
    set: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change_set(unsafe { w(input) }, set, RangeSet::invert))
}

/// `sofel_subset_input_set_contains`.
///
/// # Safety
///
/// `input` is NULL or a live input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_contains(
    input: *const SofelSubsetInput,
    set: u32,
    value: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let input = unsafe { r(input) };
    read_set(input, set, |s| i32::from(s.contains(value))).unwrap_or_else(|e| e)
}

/// `sofel_subset_input_set_count`.
///
/// # Safety
///
/// `input` is NULL or a live input; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_count(
    input: *const SofelSubsetInput,
    set: u32,
    count: *mut u64,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, count) = unsafe { (r(input), w(count)) };
    status((|| {
        let count = some(count)?;
        *count = read_set(input, set, RangeSet::len)?;
        Ok(())
    })())
}

/// `sofel_subset_input_set_next`.
///
/// # Safety
///
/// `input` is NULL or a live input; `value` is NULL or readable and
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_next(
    input: *const SofelSubsetInput,
    set: u32,
    value: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, value) = unsafe { (r(input), w(value)) };
    (|| {
        let value = some(value)?;
        let next = read_set(input, set, |s| s.next(*value))?;
        *value = next;
        Ok(i32::from(next != INVALID))
    })()
    .unwrap_or_else(|e| e)
}

/// `sofel_subset_input_get_flags`.
///
/// # Safety
///
/// `input` is NULL or a live input; `flags` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_get_flags(
    input: *const SofelSubsetInput,
    flags: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, flags) = unsafe { (r(input), w(flags)) };
    status((|| {
        *some(flags)? = some(input)?.input.flags().0;
        Ok(())
    })())
}

/// `sofel_subset_input_set_flags`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_flags(
    input: *mut SofelSubsetInput,
    flags: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let input = unsafe { w(input) };
    if flags & !Flags::ALL.0 != 0 {
        return if input.is_none() {
            crate::ERROR_NULL
        } else {
            ERROR_INVALID_ARGUMENT
        };
    }
    status(change(input, |i| i.with_flags(Flags(flags))))
}

/// `sofel_subset_input_set_glyph_mapping`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_set_glyph_mapping(
    input: *mut SofelSubsetInput,
    old: u32,
    new_glyph: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change(unsafe { w(input) }, |i| {
        let mut map = i.glyph_mapping().clone();
        map.insert(old, new_glyph);
        i.with_glyph_mapping(map)
    }))
}

/// `sofel_subset_input_clear_glyph_mapping`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_clear_glyph_mapping(
    input: *mut SofelSubsetInput,
) -> i32 {
    // SAFETY: the caller's conditions.
    status(change(unsafe { w(input) }, |i| {
        i.with_glyph_mapping(Default::default())
    }))
}

/// `sofel_subset_input_get_axis_range`.
///
/// # Safety
///
/// `input` is NULL or a live input; `min`, `def` and `max` are NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_get_axis_range(
    input: *const SofelSubsetInput,
    tag: u32,
    min: *mut f32,
    def: *mut f32,
    max: *mut f32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, min, def, max) = unsafe { (r(input), w(min), w(def), w(max)) };
    status((|| {
        let (input, min, def, max) = (some(input)?, some(min)?, some(def)?, some(max)?);
        let AxisRange {
            min: a,
            default: d,
            max: b,
        } = input
            .input
            .axis_range(Tag::from_u32(tag))
            .ok_or(ERROR_NOT_FOUND)?;
        (*min, *def, *max) = (a, d, b);
        Ok(())
    })())
}

/// `sofel_subset_input_override_name_table`.
///
/// # Safety
///
/// `input` is NULL or a live input that no other thread uses; `text` is
/// NULL or readable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_override_name_table(
    input: *mut SofelSubsetInput,
    name_id: u16,
    platform_id: u16,
    encoding_id: u16,
    language_id: u16,
    text: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, text) = unsafe { (w(input), in_array(text, len)) };
    status((|| {
        let input = some(input)?;
        let text = some(text)?;
        // HarfBuzz refuses a Macintosh name that is not ASCII.
        if platform_id == 1 && !text.is_ascii() {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        let ids = (platform_id, encoding_id, language_id, name_id);
        change(Some(input), |i| i.with_name_override(ids, text.to_vec()))
    })())
}

/// `sofel_subset_input_to_args`.
///
/// # Safety
///
/// `input` is NULL or a live input; `args` is NULL with `capacity` 0 or
/// writable for `capacity` bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_input_to_args(
    input: *const SofelSubsetInput,
    args: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, args, len) = unsafe { (r(input), out_array(args, capacity), w(len)) };
    status((|| {
        let s = some(input)?.input.to_args().join(" ");
        copy_out(s.as_bytes(), args, len)
    })())
}

/// `sofel_subset_axis_range_from_string`.
///
/// # Safety
///
/// `s` is NULL or readable for `len` bytes; `min`, `def` and `max` are
/// NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_axis_range_from_string(
    s: *const u8,
    len: usize,
    min: *mut f32,
    def: *mut f32,
    max: *mut f32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (s, min, def, max) = unsafe { (in_array(s, len), w(min), w(def), w(max)) };
    status((|| {
        let (s, min, def, max) = (some(s)?, some(min)?, some(def)?, some(max)?);
        let s = core::str::from_utf8(s).map_err(|_| ERROR_INVALID_ARGUMENT)?;
        let (a, d, b) = sofel_subset::axis_range_from_string(s).ok_or(ERROR_INVALID_ARGUMENT)?;
        let v = |x: Option<f32>| x.unwrap_or(f32::NAN);
        (*min, *def, *max) = (v(a), v(d), v(b));
        Ok(())
    })())
}

/// `sofel_subset_axis_range_to_string`.
///
/// # Safety
///
/// `input` is NULL or a live input; `s` is NULL with `capacity` 0 or
/// writable for `capacity` bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_axis_range_to_string(
    input: *const SofelSubsetInput,
    tag: u32,
    s: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (input, s, len) = unsafe { (r(input), out_array(s, capacity), w(len)) };
    status((|| {
        let text = sofel_subset::axis_range_to_string(&some(input)?.input, Tag::from_u32(tag))
            .ok_or(ERROR_NOT_FOUND)?;
        copy_out(text.as_bytes(), s, len)
    })())
}

// ---------------------------------------------------------------- plans

/// `sofel_subset_plan_create`.
///
/// # Safety
///
/// `font` is NULL or a live font; `prepared` is NULL; `input` is NULL or
/// a live input; `plan` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_create(
    font: *const SofelFont,
    prepared: *const SofelSubsetPrepared,
    input: *const SofelSubsetInput,
    plan: *mut *mut SofelSubsetPlan,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, input, plan) = unsafe { (r(font), r(input), w(plan)) };
    status((|| {
        let (font, input, plan) = (some(font)?, some(input)?, some(plan)?);
        if !prepared.is_null() {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        let p = Plan::new(&font.font, &input.input).map_err(subset_error)?;
        *plan = Box::into_raw(Box::new(SofelSubsetPlan { plan: p }));
        Ok(())
    })())
}

/// `sofel_subset_plan_destroy`.
///
/// # Safety
///
/// `plan` is NULL or from `sofel_subset_plan_create`, not yet destroyed,
/// that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_destroy(plan: *mut SofelSubsetPlan) {
    if !plan.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(plan) });
    }
}

/// `sofel_subset_plan_execute`.
///
/// # Safety
///
/// `plan` and `font` are NULL or live; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_execute(
    plan: *const SofelSubsetPlan,
    font: *const SofelFont,
    out: *mut SofelBytes,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, font, out) = unsafe { (r(plan), r(font), w(out)) };
    status((|| {
        let (plan, font) = (some(plan)?, some(font)?);
        give_result(plan.plan.execute(&font.font).map_err(subset_error), out)
    })())
}

/// `sofel_subset_plan_num_output_glyphs`.
///
/// # Safety
///
/// `plan` is NULL or live; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_num_output_glyphs(
    plan: *const SofelSubsetPlan,
    count: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, count) = unsafe { (r(plan), w(count)) };
    status((|| {
        *some(count)? = some(plan)?.plan.num_output_glyphs();
        Ok(())
    })())
}

fn lookup(
    plan: Option<&SofelSubsetPlan>,
    out: Option<&mut u32>,
    f: impl FnOnce(&Plan) -> Option<u32>,
) -> Status {
    let (plan, out) = (some(plan)?, some(out)?);
    *out = f(&plan.plan).ok_or(ERROR_NOT_FOUND)?;
    Ok(())
}

/// `sofel_subset_plan_old_to_new_glyph`.
///
/// # Safety
///
/// `plan` is NULL or live; `new_glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_old_to_new_glyph(
    plan: *const SofelSubsetPlan,
    old: u32,
    new_glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, out) = unsafe { (r(plan), w(new_glyph)) };
    status(lookup(plan, out, |p| p.new_glyph(old)))
}

/// `sofel_subset_plan_new_to_old_glyph`.
///
/// # Safety
///
/// `plan` is NULL or live; `old` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_new_to_old_glyph(
    plan: *const SofelSubsetPlan,
    new_glyph: u32,
    old: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, out) = unsafe { (r(plan), w(old)) };
    status(lookup(plan, out, |p| p.old_glyph(new_glyph)))
}

/// `sofel_subset_plan_unicode_to_old_glyph`.
///
/// # Safety
///
/// `plan` is NULL or live; `old` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_unicode_to_old_glyph(
    plan: *const SofelSubsetPlan,
    unicode: u32,
    old: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, out) = unsafe { (r(plan), w(old)) };
    status(lookup(plan, out, |p| {
        p.unicode_to_old_glyphs()
            .find(|&(c, _)| c == unicode)
            .map(|(_, g)| g)
    }))
}

/// Writes pairs to the caller's two arrays, snprintf-style.
fn pairs_out(
    pairs: &[(u32, u32)],
    first: Option<&mut [u32]>,
    second: Option<&mut [u32]>,
    len: Option<&mut usize>,
) -> Status {
    let (first, second, len) = (some(first)?, some(second)?, some(len)?);
    *len = pairs.len();
    if first.len() < pairs.len() || second.len() < pairs.len() {
        return Err(ERROR_BUFFER_TOO_SMALL);
    }
    first
        .iter_mut()
        .zip(second.iter_mut())
        .zip(pairs)
        .for_each(|((a, b), &(x, y))| (*a, *b) = (x, y));
    Ok(())
}

/// The body of the three pair functions.
///
/// # Safety
///
/// As the pair functions.
unsafe fn plan_pairs(
    plan: *const SofelSubsetPlan,
    first: *mut u32,
    second: *mut u32,
    capacity: usize,
    len: *mut usize,
    f: impl FnOnce(&Plan) -> Vec<(u32, u32)>,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, first, second, len) = unsafe {
        (
            r(plan),
            out_array(first, capacity),
            out_array(second, capacity),
            w(len),
        )
    };
    status(some(plan).and_then(|p| pairs_out(&f(&p.plan), first, second, len)))
}

/// `sofel_subset_plan_old_to_new_glyphs`.
///
/// # Safety
///
/// `plan` is NULL or live; `old` and `new_glyph` are NULL with `capacity`
/// 0 or writable for `capacity` values; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_old_to_new_glyphs(
    plan: *const SofelSubsetPlan,
    old: *mut u32,
    new_glyph: *mut u32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    unsafe { plan_pairs(plan, old, new_glyph, capacity, len, Plan::old_to_new_glyphs) }
}

/// `sofel_subset_plan_new_to_old_glyphs`.
///
/// # Safety
///
/// As `sofel_subset_plan_old_to_new_glyphs`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_new_to_old_glyphs(
    plan: *const SofelSubsetPlan,
    new_glyph: *mut u32,
    old: *mut u32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    unsafe {
        plan_pairs(plan, new_glyph, old, capacity, len, |p| {
            p.new_to_old_glyphs().to_vec()
        })
    }
}

/// `sofel_subset_plan_unicode_to_old_glyphs`.
///
/// # Safety
///
/// As `sofel_subset_plan_old_to_new_glyphs`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset_plan_unicode_to_old_glyphs(
    plan: *const SofelSubsetPlan,
    unicode: *mut u32,
    old: *mut u32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    unsafe {
        plan_pairs(plan, unicode, old, capacity, len, |p| {
            p.unicode_to_old_glyphs().collect()
        })
    }
}

/// `sofel_subset`.
///
/// # Safety
///
/// `font` and `input` are NULL or live; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_subset(
    font: *const SofelFont,
    input: *const SofelSubsetInput,
    out: *mut SofelBytes,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, input, out) = unsafe { (r(font), r(input), w(out)) };
    status((|| {
        let (font, input) = (some(font)?, some(input)?);
        give_result(
            sofel_subset::subset(&font.font, &input.input).map_err(subset_error),
            out,
        )
    })())
}

// -------------------------------------------------------- serialization

/// A link of the caller, checked against an object of `len` bytes among
/// `count` objects; `None` when it is not valid.
fn link(l: &SofelSerializeLink, len: usize, count: usize, real: bool) -> Option<Link> {
    let target_ok = l.objidx != 0 && usize::try_from(l.objidx).is_ok_and(|i| i <= count);
    if !target_ok {
        return None;
    }
    if !real {
        return Some(Link {
            objidx: l.objidx,
            ..Link::default()
        });
    }
    let width = u8::try_from(l.width).ok().filter(|w| (2..=4).contains(w))?;
    let signed = match l.is_signed {
        0 => false,
        1 if width != 3 => true,
        _ => return None,
    };
    let whence = match l.whence {
        0 => Whence::Head,
        1 => Whence::Tail,
        2 => Whence::Absolute,
        _ => return None,
    };
    let end = usize::try_from(l.position)
        .ok()?
        .checked_add(usize::from(width))?;
    (end <= len).then_some(Link {
        width,
        signed,
        whence,
        bias: l.bias,
        position: l.position,
        objidx: l.objidx,
    })
}

/// The caller's object as a [`Object`].
///
/// # Safety
///
/// The object's pointers are NULL or valid for their lengths.
unsafe fn object(o: &SofelSerializeObject, count: usize) -> Result<Object, i32> {
    // SAFETY: the caller's conditions.
    let (data, real, virt) = unsafe {
        (
            in_array(o.data, o.len),
            in_array(o.real_links, o.num_real_links),
            in_array(o.virtual_links, o.num_virtual_links),
        )
    };
    let (data, real, virt) = (some(data)?, some(real)?, some(virt)?);
    let links = |ls: &[SofelSerializeLink], is_real: bool| {
        ls.iter()
            .map(|l| link(l, data.len(), count, is_real).ok_or(ERROR_INVALID_ARGUMENT))
            .collect::<Result<Vec<Link>, i32>>()
    };
    Ok(Object {
        bytes: data.to_vec(),
        real_links: links(real, true)?,
        virtual_links: links(virt, false)?,
    })
}

/// `sofel_serialize_objects`.
///
/// # Safety
///
/// `objects` is NULL with `count` 0 or readable for `count` objects,
/// each with pointers NULL or valid for their lengths; `out` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_serialize_objects(
    tag: u32,
    objects: *const SofelSerializeObject,
    count: usize,
    out: *mut SofelBytes,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (objects, out) = unsafe { (in_array(objects, count), w(out)) };
    status((|| {
        let objects = some(objects)?;
        let out = some(out)?;
        if objects.is_empty() {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        let objs = objects
            .iter()
            // SAFETY: the caller's conditions on each object.
            .map(|o| unsafe { object(o, count) })
            .collect::<Result<Vec<Object>, i32>>()?;
        let bytes =
            sofel_write::serialize_objects(Tag::from_u32(tag), &objs).map_err(|e| match e {
                WriteError::OffsetOverflow => ERROR_OFFSET_OVERFLOW,
                _ => ERROR_INVALID_ARGUMENT,
            })?;
        give(bytes, out);
        Ok(())
    })())
}
