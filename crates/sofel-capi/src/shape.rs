//! The shaping functions of the C ABI (ADR 0020): directions, scripts,
//! languages and their OpenType tags, features, segment properties,
//! plans, buffers, shaping, and the serialization of buffers.
//!
//! A `sofel_buffer_t` holds a [`UnicodeBuffer`] or, after shaping, a
//! [`GlyphBuffer`]; `sofel_shape` turns the one into the other inside
//! the same value, and `sofel_buffer_clear` turns it back with
//! [`GlyphBuffer::into_unicode`], so that the memory is reused as
//! HarfBuzz reuses an `hb_buffer_t`.

use core::ffi::c_char;

use sofel_font::read::{GlyphId, Tag};
use sofel_font::{GlyphExtents, GlyphNames, HbScale};
use sofel_shape::{
    BufferFlags, ClusterLevel, DiffFlags, Direction, Feature, GlyphBuffer, GlyphInfo,
    GlyphPosition, Language, ResolvedProperties, Scale, Script, SegmentProperties, SerializeFlags,
    SerializeFont, SerializeFormat, ShapePlan, Shaper, UnicodeBuffer,
};

use crate::{
    ERROR_BUFFER_CONTENT, ERROR_BUFFER_TOO_SMALL, ERROR_INVALID_ARGUMENT, ERROR_NOT_FOUND,
    ERROR_NULL, OK, SofelFont, SofelGlyphNames, SofelLocation, SofelScale, Status, out_array, r,
    scale_of, some, status, w,
};

/// `sofel_language_t`.
pub struct SofelLanguage {
    language: Language,
}

/// `sofel_shape_plan_t`: a plan over the data of a `sofel_font_t`,
/// which the caller keeps alive while the plan lives.
pub struct SofelShapePlan {
    plan: ShapePlan<'static>,
}

/// `sofel_buffer_t`.
pub struct SofelBuffer {
    content: Content,
}

/// What a buffer holds.
enum Content {
    Unicode(UnicodeBuffer),
    Glyphs(GlyphBuffer),
}

/// `sofel_feature_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CFeature {
    tag: u32,
    value: u32,
    start: u32,
    end: u32,
}

/// `sofel_segment_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CSegment {
    direction: i32,
    script: u32,
    language: *const SofelLanguage,
}

// ------------------------------------------------------------ helpers

/// `len` elements at `p`: empty for NULL with a length of 0, `None` for
/// NULL otherwise.
///
/// # Safety
///
/// `p` is NULL or valid for reads of `len` elements that nothing
/// mutates during `'a`.
unsafe fn slice<'a, T>(p: *const T, len: usize) -> Option<&'a [T]> {
    if p.is_null() {
        return (len == 0).then_some(&[]);
    }
    // SAFETY: `p` is not NULL, and the caller guarantees the rest.
    Some(unsafe { core::slice::from_raw_parts(p, len) })
}

/// The caller's optional output array of `capacity` elements: `None`
/// for NULL, which the caller does not want written.
///
/// # Safety
///
/// `p` is NULL or valid for writes of `capacity` elements that nothing
/// else accesses during `'a`.
unsafe fn optional_out<'a, T>(p: *mut T, capacity: usize) -> Option<&'a mut [T]> {
    if p.is_null() {
        return None;
    }
    // SAFETY: `p` is not NULL, and the caller guarantees the rest.
    Some(unsafe { core::slice::from_raw_parts_mut(p, capacity) })
}

/// The direction of `SOFEL_DIRECTION_*`.
fn direction_from_c(d: i32) -> Option<Direction> {
    match d {
        4 => Some(Direction::Ltr),
        5 => Some(Direction::Rtl),
        6 => Some(Direction::Ttb),
        7 => Some(Direction::Btt),
        _ => None,
    }
}

/// `SOFEL_DIRECTION_*` of a direction.
fn direction_to_c(d: Direction) -> i32 {
    match d {
        Direction::Ltr => 4,
        Direction::Rtl => 5,
        Direction::Ttb => 6,
        Direction::Btt => 7,
    }
}

/// The script of a C value: `None` for 0, an error for a tag that is
/// not four letters.
fn script_from_c(s: u32) -> Result<Option<Script>, i32> {
    if s == 0 {
        return Ok(None);
    }
    Script::from_iso15924(&s.to_be_bytes())
        .map(Some)
        .ok_or(ERROR_INVALID_ARGUMENT)
}

/// The C value of a script: 0 for none.
fn script_to_c(s: Option<Script>) -> u32 {
    s.map_or(0, |s| u32::from_be_bytes(s.tag()))
}

/// Writes `s` NUL-terminated into `buffer`, snprintf-style.
fn put_bytes(s: &[u8], buffer: Option<&mut [u8]>, len: Option<&mut usize>) -> Status {
    let (buffer, len) = (some(buffer)?, some(len)?);
    *len = s.len();
    let (nul, text) = buffer
        .get_mut(..=s.len())
        .and_then(|b| b.split_last_mut())
        .ok_or(ERROR_BUFFER_TOO_SMALL)?;
    text.copy_from_slice(s);
    *nul = 0;
    Ok(())
}

/// A new language for C, or NULL for none.
fn language_out(l: Option<Language>) -> *mut SofelLanguage {
    l.map_or(core::ptr::null_mut(), |language| {
        Box::into_raw(Box::new(SofelLanguage { language }))
    })
}

// --------------------------------------------- directions and scripts

/// `sofel_direction_from_string`.
///
/// # Safety
///
/// `str` is NULL or valid for reads of `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_direction_from_string(str: *const c_char, len: usize) -> i32 {
    // SAFETY: the caller's conditions.
    let s = unsafe { slice(str.cast::<u8>(), len) };
    s.and_then(Direction::from_bytes).map_or(0, direction_to_c)
}

/// `sofel_direction_to_string`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_direction_to_string(direction: i32) -> *const c_char {
    match direction_from_c(direction) {
        Some(Direction::Ltr) => c"ltr".as_ptr(),
        Some(Direction::Rtl) => c"rtl".as_ptr(),
        Some(Direction::Ttb) => c"ttb".as_ptr(),
        Some(Direction::Btt) => c"btt".as_ptr(),
        None => core::ptr::null(),
    }
}

/// The first four bytes of `s` up to a NUL byte, padded with spaces
/// (`hb_tag_from_string`).
fn tag_bytes(s: &[u8]) -> [u8; 4] {
    let mut tag = [b' '; 4];
    for (t, &c) in tag.iter_mut().zip(s.iter().take_while(|&&c| c != 0)) {
        *t = c;
    }
    tag
}

/// `sofel_script_from_string`.
///
/// # Safety
///
/// `str` is NULL or valid for reads of `len` bytes; `script` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_script_from_string(
    str: *const c_char,
    len: usize,
    script: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (s, script) = unsafe { (slice(str.cast::<u8>(), len), w(script)) };
    status((|| {
        let (s, script) = (some(s)?, some(script)?);
        let found = Script::from_iso15924(&tag_bytes(s)).ok_or(ERROR_INVALID_ARGUMENT)?;
        *script = u32::from_be_bytes(found.tag());
        Ok(())
    })())
}

/// `sofel_script_horizontal_direction`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_script_horizontal_direction(script: u32) -> i32 {
    let s = script_from_c(script).unwrap_or(Some(Script::UNKNOWN));
    sofel_shape::script_horizontal_direction(s).map_or(0, direction_to_c)
}

// ---------------------------------------------------------- languages

/// `sofel_language_create`.
///
/// # Safety
///
/// `str` is NULL or valid for reads of `len` bytes; `error` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_language_create(
    str: *const c_char,
    len: usize,
    error: *mut i32,
) -> *mut SofelLanguage {
    // SAFETY: the caller's conditions.
    let (s, error) = unsafe { (slice(str.cast::<u8>(), len), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    match some(s).and_then(|s| Language::new(s).ok_or(ERROR_INVALID_ARGUMENT)) {
        Ok(l) => {
            *error = OK;
            language_out(Some(l))
        }
        Err(e) => {
            *error = e;
            core::ptr::null_mut()
        }
    }
}

/// `sofel_language_destroy`.
///
/// # Safety
///
/// `language` is NULL or a language from sofel, not yet destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_language_destroy(language: *mut SofelLanguage) {
    if !language.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(language) });
    }
}

/// `sofel_language_string`.
///
/// # Safety
///
/// `language` is NULL or a live language; `str` and `len` are NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_language_string(
    language: *const SofelLanguage,
    str: *mut *const c_char,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (l, str, len) = unsafe { (r(language), w(str), w(len)) };
    status((|| {
        let (l, str, len) = (some(l)?, some(str)?, some(len)?);
        let s = l.language.as_str();
        (*str, *len) = (s.as_ptr().cast::<c_char>(), s.len());
        Ok(())
    })())
}

// ------------------------------------------------------- OpenType tags

/// Writes `tags` to the caller's array and its count.
fn tags_out(tags: &[Tag], out: Option<&mut [u32]>, count: &mut usize) {
    let out = out.unwrap_or_default();
    let n = tags.len().min(out.len());
    for (o, t) in out.iter_mut().zip(tags) {
        *o = t.to_u32();
    }
    *count = n;
}

/// `sofel_ot_tags_from_script_and_language`.
///
/// # Safety
///
/// `language` is NULL or a live language; `script_count` and
/// `language_count` are NULL or writable; `script_tags` and
/// `language_tags` are NULL or writable for as many elements as the
/// counts give on entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_ot_tags_from_script_and_language(
    script: u32,
    language: *const SofelLanguage,
    script_tags: *mut u32,
    script_count: *mut usize,
    language_tags: *mut u32,
    language_count: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions; the counts are read before the
    // arrays they size.
    let (l, script_count, language_count) =
        unsafe { (r(language), w(script_count), w(language_count)) };
    status((|| {
        let (script_count, language_count) = (some(script_count)?, some(language_count)?);
        // SAFETY: the caller's conditions, with the capacities just read.
        let (st, lt) = unsafe {
            (
                out_array(script_tags, *script_count),
                out_array(language_tags, *language_count),
            )
        };
        let (st, lt) = (some(st)?, some(lt)?);
        let script = script_from_c(script)?;
        let (s, t) = sofel_shape::ot_tags_from_script_and_language_max(
            script,
            l.map(|l| &l.language),
            st.len(),
            lt.len(),
        );
        tags_out(&s, Some(st), script_count);
        tags_out(&t, Some(lt), language_count);
        Ok(())
    })())
}

/// `sofel_ot_tags_to_script_and_language`.
///
/// # Safety
///
/// `script` and `language` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_ot_tags_to_script_and_language(
    script_tag: u32,
    language_tag: u32,
    script: *mut u32,
    language: *mut *mut SofelLanguage,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (script, language) = unsafe { (w(script), w(language)) };
    status((|| {
        let (script, language) = (some(script)?, some(language)?);
        let lt = (language_tag != 0).then(|| Tag::from_u32(language_tag));
        let (s, l) = sofel_shape::ot_tags_to_script_and_language(Tag::from_u32(script_tag), lt);
        *script = script_to_c(s);
        *language = language_out(l);
        Ok(())
    })())
}

/// `sofel_ot_tag_to_script`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_ot_tag_to_script(tag: u32) -> u32 {
    script_to_c(sofel_shape::ot_tag_to_script(Tag::from_u32(tag)))
}

/// `sofel_ot_tag_to_language`.
///
/// # Safety
///
/// `language` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_ot_tag_to_language(
    tag: u32,
    language: *mut *mut SofelLanguage,
) -> i32 {
    // SAFETY: the caller's conditions.
    let language = unsafe { w(language) };
    status(some(language).map(|out| {
        *out = language_out(sofel_shape::ot_tag_to_language(Tag::from_u32(tag)));
    }))
}

// ----------------------------------------------------------- features

fn feature_from_c(f: &CFeature) -> Feature {
    Feature {
        tag: Tag::from_u32(f.tag),
        value: f.value,
        start: f.start,
        end: f.end,
    }
}

/// `sofel_feature_parse`.
///
/// # Safety
///
/// `str` is NULL or valid for reads of `len` bytes; `feature` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_feature_parse(
    str: *const c_char,
    len: usize,
    feature: *mut CFeature,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (s, feature) = unsafe { (slice(str.cast::<u8>(), len), w(feature)) };
    status((|| {
        let (s, feature) = (some(s)?, some(feature)?);
        let f = Feature::parse(s).ok_or(ERROR_INVALID_ARGUMENT)?;
        *feature = CFeature {
            tag: f.tag.to_u32(),
            value: f.value,
            start: f.start,
            end: f.end,
        };
        Ok(())
    })())
}

/// `sofel_feature_format`.
///
/// # Safety
///
/// `feature` is NULL or readable; `buffer` is NULL or writable for
/// `size` bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_feature_format(
    feature: *const CFeature,
    buffer: *mut c_char,
    size: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (f, buffer, len) = unsafe { (r(feature), out_array(buffer.cast::<u8>(), size), w(len)) };
    status(some(f).and_then(|f| put_bytes(&feature_from_c(f).format(), buffer, len)))
}

// ----------------------------------------------------------- segments

/// The properties a C segment gives, without its language.
fn segment_from_c(s: &CSegment) -> Result<SegmentProperties, i32> {
    Ok(SegmentProperties {
        direction: direction_from_c(s.direction),
        script: script_from_c(s.script)?,
        language: None,
    })
}

/// The resolved properties of a C segment, whose direction must be
/// known.
fn resolved_from_c(
    s: &CSegment,
    language: Option<&SofelLanguage>,
) -> Result<ResolvedProperties, i32> {
    Ok(ResolvedProperties {
        direction: direction_from_c(s.direction).ok_or(ERROR_INVALID_ARGUMENT)?,
        script: script_from_c(s.script)?,
        language: language.map(|l| l.language.clone()),
    })
}

/// `sofel_segment_guess`.
///
/// # Safety
///
/// `utf8` is NULL or valid for reads of `len` bytes; `in` is NULL or a
/// readable segment whose language is NULL or live; `default_language`
/// is NULL or live; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_segment_guess(
    utf8: *const c_char,
    len: usize,
    r#in: *const CSegment,
    default_language: *const SofelLanguage,
    out: *mut CSegment,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (text, seg, out) = unsafe { (slice(utf8.cast::<u8>(), len), r(r#in), w(out)) };
    status((|| {
        let (text, seg, out) = (some(text)?, some(seg)?, some(out)?);
        let mut buffer = UnicodeBuffer::new();
        buffer.push_utf8(text, 0, None);
        let guessed = segment_from_c(seg)?.guess(&buffer, None);
        *out = CSegment {
            direction: direction_to_c(guessed.direction),
            script: script_to_c(guessed.script),
            language: if seg.language.is_null() {
                default_language
            } else {
                seg.language
            },
        };
        Ok(())
    })())
}

// -------------------------------------------------------------- plans

/// `sofel_shape_plan_create`.
///
/// # Safety
///
/// `font` is NULL or a live font that outlives the plan; `segment` is
/// NULL or a readable segment whose language is NULL or live;
/// `features` is NULL or readable for `n` features; `error` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_shape_plan_create(
    font: *const SofelFont,
    location: *const SofelLocation,
    segment: *const CSegment,
    features: *const CFeature,
    n: usize,
    error: *mut i32,
) -> *mut SofelShapePlan {
    // SAFETY: the caller's conditions; the plan borrows the font's data,
    // which the caller keeps alive while the plan lives.
    let (font, location, seg, features, error) = unsafe {
        (
            r(font),
            r(location),
            r(segment),
            slice(features, n),
            w(error),
        )
    };
    // SAFETY: the segment's language is NULL or live.
    let language = seg.and_then(|s| unsafe { r(s.language) });
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    let plan = (|| {
        let (font, seg, features) = (some(font)?, some(seg)?, some(features)?);
        let props = resolved_from_c(seg, language)?;
        let features: Vec<Feature> = features.iter().map(feature_from_c).collect();
        let location = location
            .map(|v| &v.location)
            .unwrap_or(sofel_font::var::Location::default_ref());
        Ok(ShapePlan::new(
            font.font.instance(location),
            props,
            &features,
        ))
    })();
    match plan {
        Ok(plan) => {
            *error = OK;
            Box::into_raw(Box::new(SofelShapePlan { plan }))
        }
        Err(e) => {
            *error = e;
            core::ptr::null_mut()
        }
    }
}

/// `sofel_shape_plan_destroy`.
///
/// # Safety
///
/// `plan` is NULL or a plan from `sofel_shape_plan_create`, not yet
/// destroyed, that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_shape_plan_destroy(plan: *mut SofelShapePlan) {
    if !plan.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(plan) });
    }
}

/// `sofel_shape_plan_shaper`.
///
/// # Safety
///
/// `plan` is NULL or a live plan; `shaper` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_shape_plan_shaper(
    plan: *const SofelShapePlan,
    shaper: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (plan, shaper) = unsafe { (r(plan), w(shaper)) };
    status((|| {
        let (plan, shaper) = (some(plan)?, some(shaper)?);
        *shaper = match plan.plan.shaper() {
            Shaper::Default => 0,
            Shaper::Arabic => 1,
            Shaper::Hangul => 2,
            Shaper::Hebrew => 3,
            Shaper::Indic => 4,
            Shaper::Khmer => 5,
            Shaper::Myanmar => 6,
            Shaper::MyanmarZawgyi => 7,
            Shaper::Thai => 8,
            Shaper::Use => 9,
        };
        Ok(())
    })())
}

// ------------------------------------------------------------ buffers

fn unicode(b: Option<&SofelBuffer>) -> Result<&UnicodeBuffer, i32> {
    match &some(b)?.content {
        Content::Unicode(u) => Ok(u),
        Content::Glyphs(_) => Err(ERROR_BUFFER_CONTENT),
    }
}

fn unicode_mut(b: Option<&mut SofelBuffer>) -> Result<&mut UnicodeBuffer, i32> {
    match &mut some(b)?.content {
        Content::Unicode(u) => Ok(u),
        Content::Glyphs(_) => Err(ERROR_BUFFER_CONTENT),
    }
}

fn glyphs(b: Option<&SofelBuffer>) -> Result<&GlyphBuffer, i32> {
    match &some(b)?.content {
        Content::Glyphs(g) => Ok(g),
        Content::Unicode(_) => Err(ERROR_BUFFER_CONTENT),
    }
}

/// Takes the characters out of a buffer to shape them.
fn take_unicode(b: &mut SofelBuffer) -> Result<UnicodeBuffer, i32> {
    match core::mem::replace(&mut b.content, Content::Unicode(UnicodeBuffer::new())) {
        Content::Unicode(u) => Ok(u),
        other => {
            b.content = other;
            Err(ERROR_BUFFER_CONTENT)
        }
    }
}

fn new_buffer(content: Content) -> *mut SofelBuffer {
    Box::into_raw(Box::new(SofelBuffer { content }))
}

/// `sofel_buffer_create`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_buffer_create() -> *mut SofelBuffer {
    new_buffer(Content::Unicode(UnicodeBuffer::new()))
}

/// `sofel_buffer_create_similar`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer; `error` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_create_similar(
    buffer: *const SofelBuffer,
    error: *mut i32,
) -> *mut SofelBuffer {
    // SAFETY: the caller's conditions.
    let (b, error) = unsafe { (r(buffer), w(error)) };
    let Some(error) = error else {
        return core::ptr::null_mut();
    };
    match unicode(b) {
        Ok(u) => {
            *error = OK;
            new_buffer(Content::Unicode(u.similar()))
        }
        Err(e) => {
            *error = e;
            core::ptr::null_mut()
        }
    }
}

/// `sofel_buffer_destroy`.
///
/// # Safety
///
/// `buffer` is NULL or a buffer from sofel, not yet destroyed, that no
/// other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_destroy(buffer: *mut SofelBuffer) {
    if !buffer.is_null() {
        // SAFETY: the pointer came from Box::into_raw and is dropped once.
        drop(unsafe { Box::from_raw(buffer) });
    }
}

/// `sofel_buffer_clear`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_clear(buffer: *mut SofelBuffer) -> i32 {
    // SAFETY: the caller's conditions.
    let b = unsafe { w(buffer) };
    status(some(b).map(|b| {
        let content = core::mem::replace(&mut b.content, Content::Unicode(UnicodeBuffer::new()));
        b.content = Content::Unicode(match content {
            Content::Unicode(mut u) => {
                u.clear();
                u
            }
            Content::Glyphs(g) => g.into_unicode(),
        });
    }))
}

/// `sofel_buffer_length`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_length(buffer: *const SofelBuffer, len: *mut usize) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, len) = unsafe { (r(buffer), w(len)) };
    status((|| {
        let (b, len) = (some(b)?, some(len)?);
        *len = match &b.content {
            Content::Unicode(u) => u.len(),
            Content::Glyphs(g) => g.len(),
        };
        Ok(())
    })())
}

/// `sofel_buffer_add`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_add(
    buffer: *mut SofelBuffer,
    codepoint: u32,
    cluster: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let b = unsafe { w(buffer) };
    status(unicode_mut(b).map(|u| u.push(codepoint, cluster)))
}

/// A `push_*` method of [`UnicodeBuffer`].
type Push<T> = fn(&mut UnicodeBuffer, &[T], usize, Option<usize>);

fn add<T>(
    b: Option<&mut SofelBuffer>,
    text: Option<&[T]>,
    item_offset: usize,
    item_len: usize,
    push: Push<T>,
) -> Status {
    let text = some(text)?;
    push(unicode_mut(b)?, text, item_offset, Some(item_len));
    Ok(())
}

/// `sofel_buffer_add_utf8`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_add_utf8(
    buffer: *mut SofelBuffer,
    text: *const c_char,
    len: usize,
    item_offset: usize,
    item_len: usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text) = unsafe { (w(buffer), slice(text.cast::<u8>(), len)) };
    status(add(
        b,
        text,
        item_offset,
        item_len,
        UnicodeBuffer::push_utf8,
    ))
}

/// `sofel_buffer_add_utf16`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_add_utf16(
    buffer: *mut SofelBuffer,
    text: *const u16,
    len: usize,
    item_offset: usize,
    item_len: usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text) = unsafe { (w(buffer), slice(text, len)) };
    status(add(
        b,
        text,
        item_offset,
        item_len,
        UnicodeBuffer::push_utf16,
    ))
}

/// `sofel_buffer_add_utf32`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_add_utf32(
    buffer: *mut SofelBuffer,
    text: *const u32,
    len: usize,
    item_offset: usize,
    item_len: usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text) = unsafe { (w(buffer), slice(text, len)) };
    status(add(
        b,
        text,
        item_offset,
        item_len,
        UnicodeBuffer::push_utf32,
    ))
}

/// `sofel_buffer_add_codepoints`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` code points.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_add_codepoints(
    buffer: *mut SofelBuffer,
    text: *const u32,
    len: usize,
    item_offset: usize,
    item_len: usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text) = unsafe { (w(buffer), slice(text, len)) };
    status(add(
        b,
        text,
        item_offset,
        item_len,
        UnicodeBuffer::push_codepoints,
    ))
}

/// `sofel_buffer_add_latin1`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_add_latin1(
    buffer: *mut SofelBuffer,
    text: *const u8,
    len: usize,
    item_offset: usize,
    item_len: usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text) = unsafe { (w(buffer), slice(text, len)) };
    status(add(
        b,
        text,
        item_offset,
        item_len,
        UnicodeBuffer::push_latin1,
    ))
}

/// `sofel_buffer_append`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses;
/// `source` is NULL or a live buffer that nothing mutates during the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_append(
    buffer: *mut SofelBuffer,
    source: *const SofelBuffer,
    start: usize,
    end: usize,
) -> i32 {
    if core::ptr::eq(buffer, source) && !buffer.is_null() {
        return ERROR_INVALID_ARGUMENT;
    }
    // SAFETY: the caller's conditions; the two buffers are distinct.
    let (b, source) = unsafe { (w(buffer), r(source)) };
    status((|| {
        let source = unicode(source)?;
        unicode_mut(b)?.append(source, start, end);
        Ok(())
    })())
}

/// `sofel_buffer_number_clusters`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_number_clusters(buffer: *mut SofelBuffer) -> i32 {
    // SAFETY: the caller's conditions.
    let b = unsafe { w(buffer) };
    status(unicode_mut(b).map(UnicodeBuffer::number_clusters))
}

/// `sofel_buffer_codepoints`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer; `codepoints` and `clusters` are
/// NULL or writable for `capacity` elements; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_codepoints(
    buffer: *const SofelBuffer,
    codepoints: *mut u32,
    clusters: *mut u32,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, codepoints, clusters, len) = unsafe {
        (
            r(buffer),
            optional_out(codepoints, capacity),
            optional_out(clusters, capacity),
            w(len),
        )
    };
    status((|| {
        let (u, len) = (unicode(b)?, some(len)?);
        *len = u.len();
        if capacity < u.len() {
            return Err(ERROR_BUFFER_TOO_SMALL);
        }
        for (o, c) in codepoints
            .unwrap_or_default()
            .iter_mut()
            .zip(u.codepoints())
        {
            *o = c;
        }
        for (o, c) in clusters.unwrap_or_default().iter_mut().zip(u.clusters()) {
            *o = c;
        }
        Ok(())
    })())
}

/// `sofel_buffer_context`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer; `context` is NULL or writable for
/// 5 elements; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_context(
    buffer: *const SofelBuffer,
    side: i32,
    context: *mut u32,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions; `[u32; 5]` has the alignment of `u32`.
    let (b, context, len) = unsafe { (r(buffer), context.cast::<[u32; 5]>().as_mut(), w(len)) };
    status((|| {
        let (u, context, len) = (unicode(b)?, some(context)?, some(len)?);
        let c = match side {
            0 => u.pre_context(),
            1 => u.post_context(),
            _ => return Err(ERROR_INVALID_ARGUMENT),
        };
        for (o, &v) in context.iter_mut().zip(c) {
            *o = v;
        }
        *len = c.len();
        Ok(())
    })())
}

fn get_setting<T>(
    b: Option<&SofelBuffer>,
    out: Option<&mut T>,
    f: fn(&UnicodeBuffer) -> T,
) -> Status {
    let (u, out) = (unicode(b)?, some(out)?);
    *out = f(u);
    Ok(())
}

fn set_setting<T>(
    b: Option<&mut SofelBuffer>,
    v: T,
    f: fn(&mut UnicodeBuffer, T) -> Status,
) -> Status {
    f(unicode_mut(b)?, v)
}

/// A getter and a setter of a buffer setting.
macro_rules! setting {
    ($get:ident, $set:ident, $t:ty, $read:expr, $write:expr) => {
        #[doc = concat!("`", stringify!($get), "`.")]
        ///
        /// # Safety
        ///
        /// `buffer` is NULL or a live buffer; `out` is NULL or writable.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(buffer: *const SofelBuffer, out: *mut $t) -> i32 {
            // SAFETY: the caller's conditions.
            let (b, out) = unsafe { (r(buffer), w(out)) };
            status(get_setting(b, out, $read))
        }

        #[doc = concat!("`", stringify!($set), "`.")]
        ///
        /// # Safety
        ///
        /// `buffer` is NULL or a live buffer that no other thread uses.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(buffer: *mut SofelBuffer, value: $t) -> i32 {
            // SAFETY: the caller's conditions.
            let b = unsafe { w(buffer) };
            status(set_setting(b, value, $write))
        }
    };
}

fn level_from_c(level: i32) -> Option<ClusterLevel> {
    match level {
        0 => Some(ClusterLevel::MonotoneGraphemes),
        1 => Some(ClusterLevel::MonotoneCharacters),
        2 => Some(ClusterLevel::Characters),
        3 => Some(ClusterLevel::Graphemes),
        _ => None,
    }
}

fn level_to_c(level: ClusterLevel) -> i32 {
    match level {
        ClusterLevel::MonotoneGraphemes => 0,
        ClusterLevel::MonotoneCharacters => 1,
        ClusterLevel::Characters => 2,
        ClusterLevel::Graphemes => 3,
    }
}

setting!(
    sofel_buffer_get_flags,
    sofel_buffer_set_flags,
    u32,
    |u| u.flags().0,
    |u, v| {
        u.set_flags(BufferFlags(v));
        Ok(())
    }
);
setting!(
    sofel_buffer_get_cluster_level,
    sofel_buffer_set_cluster_level,
    i32,
    |u| level_to_c(u.cluster_level()),
    |u, v| {
        u.set_cluster_level(level_from_c(v).ok_or(ERROR_INVALID_ARGUMENT)?);
        Ok(())
    }
);
setting!(
    sofel_buffer_get_replacement_codepoint,
    sofel_buffer_set_replacement_codepoint,
    u32,
    UnicodeBuffer::replacement_codepoint,
    |u, v| {
        u.set_replacement_codepoint(v);
        Ok(())
    }
);
setting!(
    sofel_buffer_get_invisible_glyph,
    sofel_buffer_set_invisible_glyph,
    u32,
    UnicodeBuffer::invisible_glyph,
    |u, v| {
        u.set_invisible_glyph(v);
        Ok(())
    }
);
setting!(
    sofel_buffer_get_not_found_glyph,
    sofel_buffer_set_not_found_glyph,
    u32,
    UnicodeBuffer::not_found_glyph,
    |u, v| {
        u.set_not_found_glyph(v);
        Ok(())
    }
);
setting!(
    sofel_buffer_get_not_found_variation_selector_glyph,
    sofel_buffer_set_not_found_variation_selector_glyph,
    u32,
    |u| u.not_found_variation_selector_glyph().unwrap_or(u32::MAX),
    |u, v| {
        u.set_not_found_variation_selector_glyph((v != u32::MAX).then_some(v));
        Ok(())
    }
);
setting!(
    sofel_buffer_get_random_state,
    sofel_buffer_set_random_state,
    u32,
    UnicodeBuffer::random_state,
    |u, v| {
        u.set_random_state(v);
        Ok(())
    }
);

// ------------------------------------------------------------ shaping

/// HarfBuzz's glyph box of a glyph in font units, from the best source
/// the font has (`hb_ot_get_glyph_extents`).
fn font_box(font: &SofelFont, glyph: GlyphId) -> Option<GlyphExtents> {
    sofel_color::glyph_extents(
        &font.outlines,
        glyph.0,
        &mut sofel_color::Scratch::default(),
    )
}

fn shape(
    font: Option<&SofelFont>,
    location: Option<&SofelLocation>,
    scale: Option<&SofelScale>,
    plan: Option<&SofelShapePlan>,
    b: Option<&mut SofelBuffer>,
) -> Status {
    let (font, plan, b) = (some(font)?, some(plan)?, some(b)?);
    let u = take_unicode(b)?;
    let s = Scale::new(
        scale_of(scale, &font.font),
        scale.map_or(0, |s| s.x_ppem),
        scale.map_or(0, |s| s.y_ppem),
    );
    let location = location
        .map(|v| &v.location)
        .unwrap_or(sofel_font::var::Location::default_ref());
    let instance = font.font.instance(location);
    let boxes = |g: GlyphId| instance.glyf_extents(g).or_else(|| font_box(font, g));
    b.content = Content::Glyphs(sofel_shape::shape_with_extents(
        instance, &s, &plan.plan, u, &boxes,
    ));
    Ok(())
}

/// `sofel_shape`.
///
/// # Safety
///
/// `font`, `scale` and `plan` are NULL or live; `buffer` is NULL or a
/// live buffer that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_shape(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    plan: *const SofelShapePlan,
    buffer: *mut SofelBuffer,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, scale, plan, b) =
        unsafe { (r(font), r(location), r(scale), r(plan), w(buffer)) };
    status(shape(font, location, scale, plan, b))
}

/// `sofel_shape_fallback`.
///
/// # Safety
///
/// `font` and `scale` are NULL or live; `segment` is NULL or a readable
/// segment whose language is NULL or live; `buffer` is NULL or a live
/// buffer that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_shape_fallback(
    font: *const SofelFont,
    location: *const SofelLocation,
    scale: *const SofelScale,
    segment: *const CSegment,
    buffer: *mut SofelBuffer,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, location, scale, seg, b) =
        unsafe { (r(font), r(location), r(scale), r(segment), w(buffer)) };
    // SAFETY: the segment's language is NULL or live.
    let language = seg.and_then(|s| unsafe { r(s.language) });
    status((|| {
        let (font, seg, b) = (some(font)?, some(seg)?, some(b)?);
        let props = resolved_from_c(seg, language)?;
        let u = take_unicode(b)?;
        let s = Scale::new(
            scale_of(scale, &font.font),
            scale.map_or(0, |s| s.x_ppem),
            scale.map_or(0, |s| s.y_ppem),
        );
        let location = location
            .map(|v| &v.location)
            .unwrap_or(sofel_font::var::Location::default_ref());
        b.content = Content::Glyphs(sofel_shape::shape_fallback(
            font.font.instance(location),
            &s,
            &props,
            u,
        ));
        Ok(())
    })())
}

/// Writes the glyphs to the arrays the caller wants.
fn glyphs_out(
    g: &GlyphBuffer,
    ids: Option<&mut [u32]>,
    clusters: Option<&mut [u32]>,
    flags: Option<&mut [u32]>,
    positions: Option<&mut [i32]>,
) {
    let infos = g.glyph_infos();
    for (o, i) in ids.unwrap_or_default().iter_mut().zip(infos) {
        *o = i.glyph;
    }
    for (o, i) in clusters.unwrap_or_default().iter_mut().zip(infos) {
        *o = i.cluster;
    }
    for (o, i) in flags.unwrap_or_default().iter_mut().zip(infos) {
        *o = i.flags.0;
    }
    let positions = positions.unwrap_or_default().as_chunks_mut::<4>().0;
    for (o, p) in positions.iter_mut().zip(g.glyph_positions()) {
        *o = [p.x_advance, p.y_advance, p.x_offset, p.y_offset];
    }
}

/// `sofel_buffer_glyphs`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer; `glyphs`, `clusters` and `flags`
/// are NULL or writable for `capacity` elements, `positions` NULL or
/// writable for 4 × `capacity`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_glyphs(
    buffer: *const SofelBuffer,
    glyphs_out_: *mut u32,
    clusters: *mut u32,
    flags: *mut u32,
    positions: *mut i32,
    capacity: usize,
) -> i64 {
    // SAFETY: the caller's conditions.
    let b = unsafe { r(buffer) };
    let g = match glyphs(b) {
        Ok(g) => g,
        Err(e) => return i64::from(e),
    };
    let n = g.len();
    if n <= capacity {
        // SAFETY: the caller's conditions; nothing else is borrowed.
        let arrays = unsafe {
            (
                optional_out(glyphs_out_, capacity),
                optional_out(clusters, capacity),
                optional_out(flags, capacity),
                optional_out(positions, capacity.saturating_mul(4)),
            )
        };
        glyphs_out(g, arrays.0, arrays.1, arrays.2, arrays.3);
    }
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// `sofel_buffer_direction`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer; `direction` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_direction(
    buffer: *const SofelBuffer,
    direction: *mut i32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, direction) = unsafe { (r(buffer), w(direction)) };
    status((|| {
        let (g, direction) = (glyphs(b)?, some(direction)?);
        *direction = direction_to_c(g.direction());
        Ok(())
    })())
}

/// `sofel_buffer_normalize_glyphs`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_normalize_glyphs(buffer: *mut SofelBuffer) -> i32 {
    // SAFETY: the caller's conditions.
    let b = unsafe { w(buffer) };
    status((|| match &mut some(b)?.content {
        Content::Glyphs(g) => {
            g.normalize_glyphs();
            Ok(())
        }
        Content::Unicode(_) => Err(ERROR_BUFFER_CONTENT),
    })())
}

fn diff(a: &SofelBuffer, b: &SofelBuffer, dottedcircle: u32, fuzz: u32) -> DiffFlags {
    let empty_glyphs = || GlyphBuffer::from_parts(Direction::Ltr, Vec::new(), Vec::new());
    match (&a.content, &b.content) {
        (Content::Glyphs(x), Content::Glyphs(y)) => sofel_shape::diff(x, y, dottedcircle, fuzz),
        (Content::Unicode(x), Content::Unicode(y)) => sofel_shape::diff_unicode(x, y, dottedcircle),
        // hb_buffer_diff: different kinds of content compare as such only
        // when neither buffer is empty; an empty one is of either kind.
        (Content::Unicode(x), Content::Glyphs(y)) if x.is_empty() => {
            sofel_shape::diff(&empty_glyphs(), y, dottedcircle, fuzz)
        }
        (Content::Glyphs(x), Content::Unicode(y)) if y.is_empty() => {
            sofel_shape::diff(x, &empty_glyphs(), dottedcircle, fuzz)
        }
        (Content::Glyphs(x), Content::Unicode(y)) if x.is_empty() => {
            sofel_shape::diff_unicode(&UnicodeBuffer::new(), y, dottedcircle)
        }
        (Content::Unicode(x), Content::Glyphs(y)) if y.is_empty() => {
            sofel_shape::diff_unicode(x, &UnicodeBuffer::new(), dottedcircle)
        }
        _ => DiffFlags::CONTENT_TYPE_MISMATCH,
    }
}

/// `sofel_buffer_diff`.
///
/// # Safety
///
/// `buffer` and `reference` are NULL or live buffers; `flags` is NULL
/// or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_diff(
    buffer: *const SofelBuffer,
    reference: *const SofelBuffer,
    dottedcircle_glyph: u32,
    position_fuzz: u32,
    flags: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (a, b, flags) = unsafe { (r(buffer), r(reference), w(flags)) };
    status((|| {
        let (a, b, flags) = (some(a)?, some(b)?, some(flags)?);
        *flags = diff(a, b, dottedcircle_glyph, position_fuzz).0;
        Ok(())
    })())
}

// ------------------------------------------------------ serialization

/// The format of `SOFEL_SERIALIZE_FORMAT_*`.
fn format_from_c(f: u32) -> Option<SerializeFormat> {
    match &f.to_be_bytes() {
        b"TEXT" => Some(SerializeFormat::Text),
        b"JSON" => Some(SerializeFormat::Json),
        _ => None,
    }
}

/// `sofel_serialize_format_from_string`.
///
/// # Safety
///
/// `str` is NULL or valid for reads of `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_serialize_format_from_string(str: *const c_char, len: usize) -> u32 {
    // SAFETY: the caller's conditions.
    let s = unsafe { slice(str.cast::<u8>(), len) };
    match s.and_then(SerializeFormat::from_name) {
        Some(SerializeFormat::Text) => u32::from_be_bytes(*b"TEXT"),
        Some(SerializeFormat::Json) => u32::from_be_bytes(*b"JSON"),
        None => 0,
    }
}

/// `sofel_serialize_format_to_string`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_serialize_format_to_string(format: u32) -> *const c_char {
    match format_from_c(format) {
        Some(SerializeFormat::Text) => c"text".as_ptr(),
        Some(SerializeFormat::Json) => c"json".as_ptr(),
        None => core::ptr::null(),
    }
}

/// What serialization asks of a `sofel_font_t`: HarfBuzz's names (post,
/// then CFF), boxes at the scale from the best source, names to glyphs
/// through a `sofel_glyph_names_t`, and the cmap.
struct CSerializeFont<'a> {
    font: Option<&'a SofelFont>,
    names: Option<GlyphNames<'static, 'a>>,
    scale: Option<HbScale>,
}

impl<'a> CSerializeFont<'a> {
    fn new(
        font: Option<&'a SofelFont>,
        names: Option<&'a SofelGlyphNames>,
        scale: Option<&SofelScale>,
    ) -> CSerializeFont<'a> {
        CSerializeFont {
            font,
            names: names.and_then(|n| n.font.glyph_names_from(&n.buf)),
            scale: font.map(|f| scale_of(scale, &f.font)),
        }
    }
}

impl SerializeFont for CSerializeFont<'_> {
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        let f = self.font?;
        f.font
            .glyph_name_with(GlyphId(glyph), f.outlines.hb_extents().cff1())
    }
    fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        sofel_color::scaled_glyph_extents(
            &self.font?.outlines,
            glyph,
            self.scale.as_ref()?,
            &mut sofel_color::Scratch::default(),
        )
    }
    fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        self.names.as_ref()?.glyph_from_name(name).map(|g| g.0)
    }
    fn nominal_glyph(&self, c: u32) -> Option<u32> {
        self.font?.font.nominal_glyph_raw(c).map(|g| g.0)
    }
}

/// `sofel_buffer_serialize` without the pointers.
fn serialize(
    b: &SofelBuffer,
    (start, end): (usize, usize),
    out: &mut [u8],
    font: &CSerializeFont<'_>,
    format: SerializeFormat,
    flags: SerializeFlags,
) -> (usize, usize) {
    let size = out.len();
    let mut dest = Vec::new();
    let count = match &b.content {
        Content::Glyphs(g) => sofel_shape::serialize_glyphs_range(
            g.glyph_infos(),
            Some(g.glyph_positions()),
            start,
            end,
            size,
            font,
            format,
            flags,
            &mut dest,
        ),
        // A cleared HarfBuzz buffer has no content type.
        Content::Unicode(u) if u.is_empty() => {
            if size >= 3 {
                sofel_shape::serialize_invalid(format, &mut dest);
            }
            0
        }
        Content::Unicode(u) => {
            let codepoints: Vec<u32> = u.codepoints().collect();
            let clusters: Vec<u32> = u.clusters().collect();
            sofel_shape::serialize_unicode_range(
                &codepoints,
                &clusters,
                start,
                end,
                size,
                format,
                flags,
                &mut dest,
            )
        }
    };
    if let Some((nul, text)) = out.get_mut(..=dest.len()).and_then(|o| o.split_last_mut()) {
        text.copy_from_slice(&dest);
        *nul = 0;
    }
    (dest.len(), count)
}

/// `sofel_buffer_serialize`.
///
/// # Safety
///
/// `buffer`, `font` and `scale` are NULL or live; `out` is NULL or
/// writable for `size` bytes; `consumed` and `count` are NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_serialize(
    buffer: *const SofelBuffer,
    start: usize,
    end: usize,
    out: *mut c_char,
    size: usize,
    consumed: *mut usize,
    count: *mut usize,
    font: *const SofelFont,
    scale: *const SofelScale,
    format: u32,
    flags: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, out, consumed, count, font, scale) = unsafe {
        (
            r(buffer),
            out_array(out.cast::<u8>(), size),
            w(consumed),
            w(count),
            r(font),
            r(scale),
        )
    };
    status((|| {
        let (b, out, consumed, count) = (some(b)?, some(out)?, some(consumed)?, some(count)?);
        let format = format_from_c(format).ok_or(ERROR_INVALID_ARGUMENT)?;
        let sf = CSerializeFont::new(font, None, scale);
        (*consumed, *count) = serialize(b, (start, end), out, &sf, format, SerializeFlags(flags));
        Ok(())
    })())
}

/// `sofel_buffer_deserialize_glyphs` without the pointers.
fn deserialize_glyphs(
    b: &mut SofelBuffer,
    text: &[u8],
    font: &CSerializeFont<'_>,
    direction: i32,
    format: u32,
) -> Result<usize, (i32, usize)> {
    let invalid = |e| (e, 0);
    let format = format_from_c(format).ok_or(invalid(ERROR_INVALID_ARGUMENT))?;
    let direction = direction_from_c(direction).ok_or(invalid(ERROR_INVALID_ARGUMENT))?;
    let (mut infos, mut positions): (Vec<GlyphInfo>, Vec<GlyphPosition>) = match &b.content {
        Content::Glyphs(g) => (g.glyph_infos().to_vec(), g.glyph_positions().to_vec()),
        Content::Unicode(u) if u.is_empty() => (Vec::new(), Vec::new()),
        Content::Unicode(_) => return Err(invalid(ERROR_BUFFER_CONTENT)),
    };
    let read = sofel_shape::deserialize_glyphs(text, format, font, &mut infos, &mut positions);
    b.content = Content::Glyphs(GlyphBuffer::from_parts(direction, infos, positions));
    read.map(|()| text.len())
        .map_err(|e| (ERROR_INVALID_ARGUMENT, e.consumed))
}

/// `sofel_buffer_deserialize_glyphs`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` bytes; `consumed` is NULL or
/// writable; `font` and `names` are NULL or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_deserialize_glyphs(
    buffer: *mut SofelBuffer,
    text: *const c_char,
    len: usize,
    consumed: *mut usize,
    font: *const SofelFont,
    names: *const SofelGlyphNames,
    direction: i32,
    format: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text, consumed, font, names) = unsafe {
        (
            w(buffer),
            slice(text.cast::<u8>(), len),
            w(consumed),
            r(font),
            r(names),
        )
    };
    let (Some(b), Some(text), Some(consumed)) = (b, text, consumed) else {
        return ERROR_NULL;
    };
    let sf = CSerializeFont::new(font, names, None);
    let (result, n) = match deserialize_glyphs(b, text, &sf, direction, format) {
        Ok(n) => (OK, n),
        Err((e, n)) => (e, n),
    };
    *consumed = n;
    result
}

/// `sofel_buffer_deserialize_unicode` without the pointers.
fn deserialize_unicode(
    b: &mut SofelBuffer,
    text: &[u8],
    format: u32,
) -> Result<usize, (i32, usize)> {
    let format = format_from_c(format).ok_or((ERROR_INVALID_ARGUMENT, 0))?;
    let u = unicode_mut(Some(b)).map_err(|e| (e, 0))?;
    let (mut codepoints, mut clusters) = (Vec::new(), Vec::new());
    let read = sofel_shape::deserialize_unicode(text, format, &mut codepoints, &mut clusters);
    for (&c, &cl) in codepoints.iter().zip(&clusters) {
        u.push(c, cl);
    }
    read.map(|()| text.len())
        .map_err(|e| (ERROR_INVALID_ARGUMENT, e.consumed))
}

/// `sofel_buffer_deserialize_unicode`.
///
/// # Safety
///
/// `buffer` is NULL or a live buffer that no other thread uses; `text`
/// is NULL or valid for reads of `len` bytes; `consumed` is NULL or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_buffer_deserialize_unicode(
    buffer: *mut SofelBuffer,
    text: *const c_char,
    len: usize,
    consumed: *mut usize,
    format: u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (b, text, consumed) = unsafe { (w(buffer), slice(text.cast::<u8>(), len), w(consumed)) };
    let (Some(b), Some(text), Some(consumed)) = (b, text, consumed) else {
        return ERROR_NULL;
    };
    let (result, n) = match deserialize_unicode(b, text, format) {
        Ok(n) => (OK, n),
        Err((e, n)) => (e, n),
    };
    *consumed = n;
    result
}

/// `sofel_font_glyph_to_string`.
///
/// # Safety
///
/// `font` is NULL or live; `buffer` is NULL or writable for `size`
/// bytes; `len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_glyph_to_string(
    font: *const SofelFont,
    glyph: u32,
    buffer: *mut c_char,
    size: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, buffer, len) = unsafe { (r(font), out_array(buffer.cast::<u8>(), size), w(len)) };
    let mut s = Vec::new();
    sofel_shape::glyph_to_string(&CSerializeFont::new(font, None, None), glyph, &mut s);
    status(put_bytes(&s, buffer, len))
}

/// `sofel_font_glyph_from_string`.
///
/// # Safety
///
/// `font` and `names` are NULL or live; `str` is NULL or valid for reads
/// of `len` bytes; `glyph` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_glyph_from_string(
    font: *const SofelFont,
    names: *const SofelGlyphNames,
    str: *const c_char,
    len: usize,
    glyph: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, names, s, glyph) =
        unsafe { (r(font), r(names), slice(str.cast::<u8>(), len), w(glyph)) };
    status((|| {
        let (s, glyph) = (some(s)?, some(glyph)?);
        let sf = CSerializeFont::new(font, names, None);
        *glyph = sofel_shape::glyph_from_string(&sf, s).ok_or(ERROR_NOT_FOUND)?;
        Ok(())
    })())
}

// ---------------------------------------- HarfBuzz's Unicode functions

/// `sofel_unicode_modified_combining_class`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_modified_combining_class(codepoint: u32) -> u8 {
    sofel_shape::unicode::modified_combining_class(codepoint)
}

/// `sofel_unicode_is_default_ignorable`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_is_default_ignorable(codepoint: u32) -> u8 {
    u8::from(sofel_shape::unicode::is_default_ignorable(codepoint))
}

/// `sofel_unicode_is_variation_selector`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_is_variation_selector(codepoint: u32) -> u8 {
    u8::from(sofel_shape::unicode::is_variation_selector(codepoint))
}

/// `sofel_unicode_is_regional_indicator`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_is_regional_indicator(codepoint: u32) -> u8 {
    u8::from(sofel_shape::unicode::is_regional_indicator(codepoint))
}

/// `sofel_unicode_space_fallback_type`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_space_fallback_type(codepoint: u32) -> u8 {
    sofel_shape::unicode::space_fallback_type(codepoint) as u8
}

/// `sofel_unicode_vertical_char_for`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_unicode_vertical_char_for(codepoint: u32) -> u32 {
    sofel_shape::unicode::vertical_char_for(codepoint)
}
