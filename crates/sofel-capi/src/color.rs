//! The color glyph functions of the C ABI (ADR 0023): palettes, the
//! paint commands as an array, the color formats, SVG documents, bitmap
//! strikes and glyph bitmaps, and the helper functions of `sofel-geom`.

use core::ffi::c_void;

use sofel_bitmap::{BitmapFormat, Strike, StrikeTable};
use sofel_color::{
    ColorFormat, PaintCommand, PaintOptions, Painted, Palette, RecordedBrush, Recording, Scratch,
};
use sofel_geom::{Color, ColorRef, ColorStop, Extend, ImageFormat, PointF, Solid};

use crate::{
    ERROR_BUFFER_TOO_SMALL, ERROR_INVALID_ARGUMENT, ERROR_NOT_FOUND, ERROR_NULL, SofelFont,
    font_ref, get, out_array, r, some, status, w,
};

/// `sofel_color_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl From<Color> for CColor {
    fn from(c: Color) -> CColor {
        CColor {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

impl From<CColor> for Color {
    fn from(c: CColor) -> Color {
        Color::new(c.r, c.g, c.b, c.a)
    }
}

/// `sofel_color_stop_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CColorStop {
    offset: f32,
    color: CColor,
    palette_index: i32,
    alpha: f32,
}

fn stop_c(s: &ColorStop) -> CColorStop {
    CColorStop {
        offset: s.offset,
        color: s.color.color.into(),
        palette_index: s.color.source.index.map_or(-1, i32::from),
        alpha: s.color.source.alpha,
    }
}

fn stop_rust(s: &CColorStop) -> ColorStop {
    ColorStop {
        offset: s.offset,
        color: Solid {
            color: s.color.into(),
            source: ColorRef {
                index: u16::try_from(s.palette_index).ok(),
                alpha: s.alpha,
            },
        },
    }
}

/// `sofel_paint_command_t`.
#[repr(C)]
pub struct CPaintCommand {
    kind: u32,
    glyph: u32,
    mode: u32,
    palette_index: i32,
    color: CColor,
    alpha: f32,
    values: [f32; 9],
    has_extents: i32,
    extents: [i32; 4],
    width: u32,
    height: u32,
    stops: *const CColorStop,
    n_stops: usize,
    data: *const u8,
    len: usize,
}

impl Default for CPaintCommand {
    fn default() -> CPaintCommand {
        CPaintCommand {
            kind: 0,
            glyph: 0,
            mode: 0,
            palette_index: -1,
            color: CColor::default(),
            alpha: 0.0,
            values: [0.0; 9],
            has_extents: 0,
            extents: [0; 4],
            width: 0,
            height: 0,
            stops: core::ptr::null(),
            n_stops: 0,
            data: core::ptr::null(),
            len: 0,
        }
    }
}

/// `sofel_paint_commands_t`.
#[repr(C)]
pub struct CPaintCommands {
    commands: *const CPaintCommand,
    count: usize,
    internal: *mut c_void,
}

/// The memory behind a `sofel_paint_commands_t`.
struct Owned {
    commands: Vec<CPaintCommand>,
    /// The stops the gradient commands point into, kept alive.
    _stops: Vec<CColorStop>,
}

const KIND_PUSH_TRANSFORM: u32 = 0;
const KIND_POP_TRANSFORM: u32 = 1;
const KIND_PUSH_CLIP_GLYPH: u32 = 2;
const KIND_PUSH_CLIP_RECTANGLE: u32 = 3;
const KIND_POP_CLIP: u32 = 4;
const KIND_FILL_SOLID: u32 = 5;
const KIND_FILL_LINEAR: u32 = 6;
const KIND_FILL_RADIAL: u32 = 7;
const KIND_FILL_SWEEP: u32 = 8;
const KIND_FILL_GLYPH: u32 = 9;
const KIND_IMAGE: u32 = 10;
const KIND_PUSH_GROUP: u32 = 11;
const KIND_POP_GROUP: u32 = 12;
const KIND_COLOR_GLYPH: u32 = 13;

fn extend_c(e: Extend) -> u32 {
    match e {
        Extend::Pad => 0,
        Extend::Repeat => 1,
        Extend::Reflect => 2,
    }
}

fn extend_rust(e: u32) -> Result<Extend, i32> {
    match e {
        0 => Ok(Extend::Pad),
        1 => Ok(Extend::Repeat),
        2 => Ok(Extend::Reflect),
        _ => Err(ERROR_INVALID_ARGUMENT),
    }
}

fn painted_c(p: Painted) -> i32 {
    match p {
        Painted::None => 0,
        Painted::ColrV1 => 1,
        Painted::ColrV0 => 2,
        Painted::Svg => 3,
        Painted::Cbdt => 4,
        Painted::Sbix => 5,
    }
}

fn solid_into(c: &mut CPaintCommand, s: &Solid) {
    c.color = s.color.into();
    c.palette_index = s.source.index.map_or(-1, i32::from);
    c.alpha = s.source.alpha;
}

/// The commands of a recording as C structs, and the stops they point
/// into (pointers set once the stops no longer move).
fn convert(rec: &Recording<'_>) -> Owned {
    let mut stops: Vec<CColorStop> = rec.stops().iter().map(stop_c).collect();
    stops.shrink_to_fit();
    let base = stops.as_ptr();
    let at = |r: &core::ops::Range<usize>| (base.wrapping_add(r.start), r.len());
    let p = |p: &PointF| [p.x, p.y];
    let commands = rec
        .commands()
        .iter()
        .map(|cmd| {
            let mut c = CPaintCommand::default();
            match cmd {
                PaintCommand::PushTransform(t) => {
                    c.kind = KIND_PUSH_TRANSFORM;
                    c.values[..6].copy_from_slice(&[t.xx, t.yx, t.xy, t.yy, t.dx, t.dy]);
                }
                PaintCommand::PopTransform => c.kind = KIND_POP_TRANSFORM,
                PaintCommand::PushClipGlyph(g) => {
                    c.kind = KIND_PUSH_CLIP_GLYPH;
                    c.glyph = *g;
                }
                PaintCommand::PushClipRectangle(rect) => {
                    c.kind = KIND_PUSH_CLIP_RECTANGLE;
                    c.values[..4]
                        .copy_from_slice(&[rect.x_min, rect.y_min, rect.x_max, rect.y_max]);
                }
                PaintCommand::PopClip => c.kind = KIND_POP_CLIP,
                PaintCommand::Fill(b) => match b {
                    RecordedBrush::Solid(s) => {
                        c.kind = KIND_FILL_SOLID;
                        solid_into(&mut c, s);
                    }
                    RecordedBrush::Linear {
                        stops: range,
                        extend,
                        p0,
                        p1,
                        p2,
                    } => {
                        c.kind = KIND_FILL_LINEAR;
                        c.mode = extend_c(*extend);
                        let [a, b] = p(p0);
                        let [cc, d] = p(p1);
                        let [e, f] = p(p2);
                        c.values[..6].copy_from_slice(&[a, b, cc, d, e, f]);
                        (c.stops, c.n_stops) = at(range);
                    }
                    RecordedBrush::Radial {
                        stops: range,
                        extend,
                        c0,
                        r0,
                        c1,
                        r1,
                    } => {
                        c.kind = KIND_FILL_RADIAL;
                        c.mode = extend_c(*extend);
                        c.values[..6].copy_from_slice(&[c0.x, c0.y, *r0, c1.x, c1.y, *r1]);
                        (c.stops, c.n_stops) = at(range);
                    }
                    RecordedBrush::Sweep {
                        stops: range,
                        extend,
                        center,
                        start,
                        end,
                    } => {
                        c.kind = KIND_FILL_SWEEP;
                        c.mode = extend_c(*extend);
                        c.values[..4].copy_from_slice(&[center.x, center.y, *start, *end]);
                        (c.stops, c.n_stops) = at(range);
                    }
                },
                PaintCommand::FillGlyph(g, s) => {
                    c.kind = KIND_FILL_GLYPH;
                    c.glyph = *g;
                    solid_into(&mut c, s);
                }
                PaintCommand::Image(i) => {
                    c.kind = KIND_IMAGE;
                    c.mode = match i.format {
                        ImageFormat::Png => 0,
                        ImageFormat::Svg { gzip: false } => 1,
                        ImageFormat::Svg { gzip: true } => 2,
                    };
                    c.data = i.data.as_ptr();
                    c.len = i.data.len();
                    c.width = i.width;
                    c.height = i.height;
                    if let Some(e) = i.extents {
                        c.has_extents = 1;
                        c.extents = [e.x_bearing, e.y_bearing, e.width, e.height];
                    }
                }
                PaintCommand::PushGroup(m) => {
                    c.kind = KIND_PUSH_GROUP;
                    c.mode = u32::from(*m as u8);
                }
                PaintCommand::PopGroup(m) => {
                    c.kind = KIND_POP_GROUP;
                    c.mode = u32::from(*m as u8);
                }
                PaintCommand::ColorGlyph(g) => {
                    c.kind = KIND_COLOR_GLYPH;
                    c.glyph = *g;
                }
            }
            c
        })
        .collect();
    Owned {
        commands,
        _stops: stops,
    }
}

fn paint_glyph(
    font: Option<&crate::Font<'static>>,
    glyph: u32,
    palette: u16,
    custom: Option<&[CColor]>,
    foreground: CColor,
    ppem: u16,
    out: Option<&mut CPaintCommands>,
) -> Result<i32, i32> {
    let (font, out) = (some(font)?, some(out)?);
    let custom: Option<Vec<Color>> = custom.map(|c| c.iter().map(|&x| x.into()).collect());
    let options = PaintOptions {
        palette: custom
            .as_deref()
            .map_or(Palette::Font(palette), Palette::Custom),
        foreground: foreground.into(),
        ppem,
    };
    let mut rec = Recording::new();
    let painted =
        sofel_color::paint_recording(font, glyph, &options, &mut Scratch::default(), &mut rec);
    let owned = Box::new(convert(&rec));
    out.commands = owned.commands.as_ptr();
    out.count = owned.commands.len();
    out.internal = Box::into_raw(owned).cast();
    Ok(painted_c(painted))
}

/// `sofel_paint_glyph`.
///
/// # Safety
///
/// `font` is NULL or a live font; `custom_colors` is NULL or valid for
/// `n_custom` colors; `out` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_paint_glyph(
    font: *const SofelFont,
    glyph: u32,
    palette: u16,
    custom_colors: *const CColor,
    n_custom: usize,
    foreground: CColor,
    ppem: u16,
    out: *mut CPaintCommands,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, out) = unsafe { (font_ref(font), w(out)) };
    let custom = if custom_colors.is_null() {
        None
    } else {
        // SAFETY: not NULL; the caller's conditions.
        Some(unsafe { core::slice::from_raw_parts(custom_colors, n_custom) })
    };
    paint_glyph(font, glyph, palette, custom, foreground, ppem, out).unwrap_or_else(|e| e)
}

/// `sofel_paint_commands_free`.
///
/// # Safety
///
/// `commands` is NULL or filled by `sofel_paint_glyph` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_paint_commands_free(commands: *mut CPaintCommands) {
    // SAFETY: the caller's conditions.
    let Some(c) = (unsafe { w(commands) }) else {
        return;
    };
    if !c.internal.is_null() {
        // SAFETY: `internal` came from Box::into_raw in paint_glyph and is
        // dropped once (it is cleared below).
        drop(unsafe { Box::from_raw(c.internal.cast::<Owned>()) });
    }
    c.internal = core::ptr::null_mut();
    c.commands = core::ptr::null();
    c.count = 0;
}

// ------------------------------------------------------------- palettes

/// `sofel_palette_count`.
///
/// # Safety
///
/// `font` is NULL or a live font; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_palette_count(font: *const SofelFont, count: *mut u16) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, sofel_color::palette_count))
}

/// `sofel_palette_entry_count`.
///
/// # Safety
///
/// `font` is NULL or a live font; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_palette_entry_count(font: *const SofelFont, count: *mut u16) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, count) = unsafe { (font_ref(font), w(count)) };
    status(get(font, count, sofel_color::palette_entry_count))
}

/// `sofel_palette_name_id`.
///
/// # Safety
///
/// `font` is NULL or a live font; `name_id` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_palette_name_id(
    font: *const SofelFont,
    palette: u16,
    name_id: *mut u16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, name_id) = unsafe { (font_ref(font), w(name_id)) };
    status(get(font, name_id, |f| {
        sofel_color::palette_name_id(f, palette)
    }))
}

/// `sofel_palette_flags`.
///
/// # Safety
///
/// `font` is NULL or a live font; `flags` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_palette_flags(
    font: *const SofelFont,
    palette: u16,
    flags: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, flags) = unsafe { (font_ref(font), w(flags)) };
    status(get(font, flags, |f| sofel_color::palette_flags(f, palette)))
}

/// `sofel_palette_color_name_id`.
///
/// # Safety
///
/// `font` is NULL or a live font; `name_id` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_palette_color_name_id(
    font: *const SofelFont,
    entry: u16,
    name_id: *mut u16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, name_id) = unsafe { (font_ref(font), w(name_id)) };
    status(get(font, name_id, |f| {
        sofel_color::palette_color_name_id(f, entry)
    }))
}

/// `sofel_palette_colors`.
///
/// # Safety
///
/// `font` is NULL or a live font; `colors` is NULL or writable for
/// `capacity` colors; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_palette_colors(
    font: *const SofelFont,
    palette: u16,
    colors: *mut CColor,
    capacity: usize,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, colors, count) = unsafe { (font_ref(font), out_array(colors, capacity), w(count)) };
    status((|| {
        let f = some(font)?;
        let v: Vec<CColor> = sofel_color::palette_colors(f, palette)
            .map(Into::into)
            .collect();
        crate::copy_out(&v, colors, count)
    })())
}

/// `sofel_find_palette`.
///
/// # Safety
///
/// `font` is NULL or a live font; `palette` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_find_palette(
    font: *const SofelFont,
    flags: u32,
    palette: *mut u16,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, palette) = unsafe { (font_ref(font), w(palette)) };
    status(crate::find(font, palette, |f| {
        sofel_color::find_palette(f, flags)
    }))
}

// --------------------------------------------------------------- formats

/// `sofel_color_formats`.
///
/// # Safety
///
/// `font` is NULL or a live font; `formats` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_color_formats(font: *const SofelFont, formats: *mut u32) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, formats) = unsafe { (font_ref(font), w(formats)) };
    status(get(font, formats, |f| {
        u32::from(sofel_color::color_formats(f).0)
    }))
}

/// `sofel_glyph_color_format`.
///
/// # Safety
///
/// `font` is NULL or a live font; `format` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_glyph_color_format(
    font: *const SofelFont,
    glyph: u32,
    format: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, format) = unsafe { (font_ref(font), w(format)) };
    status(crate::find(font, format, |f| {
        sofel_color::glyph_color_format(f, glyph).map(|c| match c {
            ColorFormat::ColrV1 => 1,
            ColorFormat::ColrV0 => 2,
            ColorFormat::Svg => 3,
            ColorFormat::Cbdt => 4,
            ColorFormat::Sbix => 5,
        })
    }))
}

/// `sofel_color_layers`.
///
/// # Safety
///
/// `font` is NULL or a live font; `glyphs` and `indices` are NULL or
/// writable for `capacity` elements; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_color_layers(
    font: *const SofelFont,
    glyph: u32,
    glyphs: *mut u32,
    indices: *mut i32,
    capacity: usize,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, glyphs, indices, count) = unsafe {
        (
            font_ref(font),
            out_array(glyphs, capacity),
            out_array(indices, capacity),
            w(count),
        )
    };
    status((|| {
        let (f, glyphs, indices, count) =
            (some(font)?, some(glyphs)?, some(indices)?, some(count)?);
        let layers: Vec<(u32, Option<u16>)> = sofel_color::layers(f, glyph).collect();
        *count = layers.len();
        if layers.len() > glyphs.len() {
            return Err(ERROR_BUFFER_TOO_SMALL);
        }
        for ((g, i), (dg, di)) in layers.iter().zip(glyphs.iter_mut().zip(indices.iter_mut())) {
            *dg = *g;
            *di = i.map_or(-1, i32::from);
        }
        Ok(())
    })())
}

/// `sofel_svg_document`.
///
/// # Safety
///
/// `font` is NULL or a live font; the outputs are NULL or writable
/// (`range` for 2 integers).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_svg_document(
    font: *const SofelFont,
    glyph: u32,
    data: *mut *const u8,
    len: *mut usize,
    index: *mut u32,
    range: *mut u32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, data, len, index) = unsafe { (font_ref(font), w(data), w(len), w(index)) };
    // SAFETY: the caller's conditions.
    let range = unsafe { range.cast::<[u32; 2]>().as_mut() };
    status((|| {
        let (f, data, len, index, range) = (
            some(font)?,
            some(data)?,
            some(len)?,
            some(index)?,
            some(range)?,
        );
        let d = sofel_color::svg_document(f, glyph).ok_or(ERROR_NOT_FOUND)?;
        *data = d.data.as_ptr();
        *len = d.data.len();
        *index = d.index;
        *range = [d.start_glyph, d.end_glyph];
        Ok(())
    })())
}

// ---------------------------------------------------------------- bitmaps

/// `sofel_strike_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CStrike {
    index: u32,
    table: u32,
    x_ppem: u16,
    y_ppem: u16,
    bit_depth: u8,
    resolution: u16,
    height: i16,
    width: i16,
}

fn strike_c(s: &Strike) -> CStrike {
    CStrike {
        index: s.index,
        table: match s.table {
            StrikeTable::Cblc => 0,
            StrikeTable::Eblc => 1,
            StrikeTable::Bloc => 2,
            StrikeTable::Sbix => 3,
        },
        x_ppem: s.x_ppem,
        y_ppem: s.y_ppem,
        bit_depth: s.bit_depth,
        resolution: s.resolution,
        height: s.height,
        width: s.width,
    }
}

/// `sofel_bitmap_strikes`.
///
/// # Safety
///
/// `font` is NULL or a live font; `strikes` is NULL or writable for
/// `capacity` strikes; `count` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_bitmap_strikes(
    font: *const SofelFont,
    strikes: *mut CStrike,
    capacity: usize,
    count: *mut usize,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, strikes, count) =
        unsafe { (font_ref(font), out_array(strikes, capacity), w(count)) };
    status((|| {
        let f = some(font)?;
        let v: Vec<CStrike> = sofel_bitmap::strikes(f).map(|s| strike_c(&s)).collect();
        crate::copy_out(&v, strikes, count)
    })())
}

/// The position in [`sofel_bitmap::strikes`] of the strike with `index`.
fn strike_by_index(f: &crate::Font<'_>, index: u32) -> Option<Strike> {
    sofel_bitmap::strikes(f).find(|s| s.index == index)
}

/// `sofel_choose_strike`.
///
/// # Safety
///
/// `font` is NULL or a live font; `strike` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_choose_strike(
    font: *const SofelFont,
    ppem: u16,
    strike: *mut CStrike,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, strike) = unsafe { (font_ref(font), w(strike)) };
    status(crate::find(font, strike, |f| {
        sofel_bitmap::choose_strike(f, ppem).map(|s| strike_c(&s))
    }))
}

/// `sofel_match_strike`.
///
/// # Safety
///
/// `font` is NULL or a live font; `strike` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_match_strike(
    font: *const SofelFont,
    x_ppem: u32,
    y_ppem: u32,
    strike: *mut CStrike,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, strike) = unsafe { (font_ref(font), w(strike)) };
    status(crate::find(font, strike, |f| {
        sofel_bitmap::match_strike(f, x_ppem, y_ppem).map(|s| strike_c(&s))
    }))
}

/// `sofel_glyph_bitmap_t`.
#[repr(C)]
pub struct CGlyphBitmap {
    format: u32,
    tag: u32,
    data: *const u8,
    len: usize,
    width: u16,
    height: u16,
    hori_bearing_x: i16,
    hori_bearing_y: i16,
    hori_advance: u16,
    vert_bearing_x: i16,
    vert_bearing_y: i16,
    vert_advance: u16,
    bit_depth: u8,
    flipped: u8,
}

/// `sofel_glyph_bitmap`.
///
/// # Safety
///
/// `font` is NULL or a live font; `bitmap` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_glyph_bitmap(
    font: *const SofelFont,
    strike_index: u32,
    glyph: u32,
    bitmap: *mut CGlyphBitmap,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, bitmap) = unsafe { (font_ref(font), w(bitmap)) };
    status((|| {
        let (f, out) = (some(font)?, some(bitmap)?);
        let s = strike_by_index(f, strike_index).ok_or(ERROR_NOT_FOUND)?;
        let b = sofel_bitmap::glyph_bitmap(f, &s, glyph).ok_or(ERROR_NOT_FOUND)?;
        let (format, tag) = match b.format {
            BitmapFormat::Ebdt(n) => (u32::from(n), 0),
            BitmapFormat::Sbix(t) => (0, t.to_u32()),
        };
        let m = b.metrics;
        *out = CGlyphBitmap {
            format,
            tag,
            data: b.data.as_ptr(),
            len: b.data.len(),
            width: m.width,
            height: m.height,
            hori_bearing_x: m.hori_bearing_x,
            hori_bearing_y: m.hori_bearing_y,
            hori_advance: m.hori_advance,
            vert_bearing_x: m.vert_bearing_x,
            vert_bearing_y: m.vert_bearing_y,
            vert_advance: m.vert_advance,
            bit_depth: b.bit_depth,
            flipped: u8::from(b.flipped),
        };
        Ok(())
    })())
}

// ------------------------------------------------------------ geometry

/// `sofel_sin_cos`.
///
/// # Safety
///
/// `s` and `c` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_sin_cos(x: f32, s: *mut f32, c: *mut f32) -> i32 {
    // SAFETY: the caller's conditions.
    let (s, c) = unsafe { (w(s), w(c)) };
    status((|| {
        let (s, c) = (some(s)?, some(c)?);
        (*s, *c) = sofel_geom::sin_cos(x);
        Ok(())
    })())
}

/// `sofel_tan`.
#[unsafe(no_mangle)]
pub extern "C" fn sofel_tan(x: f32) -> f32 {
    sofel_geom::tan(x)
}

/// `sofel_normalize_color_line`.
///
/// # Safety
///
/// `stops` is NULL or valid for reads and writes of `n` stops; `min` and
/// `max` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_normalize_color_line(
    stops: *mut CColorStop,
    n: usize,
    min: *mut f32,
    max: *mut f32,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (stops, min, max) = unsafe { (out_array(stops, n), w(min), w(max)) };
    status((|| {
        let (stops, min, max) = (some(stops)?, some(min)?, some(max)?);
        let mut v: Vec<ColorStop> = stops.iter().map(stop_rust).collect();
        (*min, *max) = sofel_geom::normalize_color_line(&mut v);
        for (d, s) in stops.iter_mut().zip(&v) {
            *d = stop_c(s);
        }
        Ok(())
    })())
}

/// `sofel_reduce_linear_anchors`.
///
/// # Safety
///
/// `points` is NULL or valid for reads of 6 floats; `out` is NULL or
/// writable for 4.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_reduce_linear_anchors(points: *const f32, out: *mut f32) -> i32 {
    // SAFETY: the caller's conditions.
    let (points, out) = unsafe {
        (
            r(points.cast::<[f32; 6]>()),
            out.cast::<[f32; 4]>().as_mut(),
        )
    };
    status((|| {
        let ([x0, y0, x1, y1, x2, y2], out) = (*some(points)?, some(out)?);
        let (a, b) = sofel_geom::reduce_linear_anchors(
            PointF::new(x0, y0),
            PointF::new(x1, y1),
            PointF::new(x2, y2),
        );
        *out = [a.x, a.y, b.x, b.y];
        Ok(())
    })())
}

/// `sofel_sweep_tile_func_t`.
pub type SweepTileFunc = Option<extern "C" fn(*mut c_void, f32, CColor, f32, CColor)>;

/// `sofel_sweep_gradient_tiles`.
///
/// # Safety
///
/// `stops` is NULL or valid for reads and writes of `n` stops; `emit`
/// is called with `user`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_sweep_gradient_tiles(
    stops: *mut CColorStop,
    n: usize,
    extend: u32,
    start: f32,
    end: f32,
    emit: SweepTileFunc,
    user: *mut c_void,
) -> i32 {
    // SAFETY: the caller's conditions.
    let stops = unsafe { out_array(stops, n) };
    status((|| {
        let (stops, emit) = (some(stops)?, emit.ok_or(ERROR_NULL)?);
        let mut v: Vec<ColorStop> = stops.iter().map(stop_rust).collect();
        sofel_geom::sweep_gradient_tiles(
            &mut v,
            extend_rust(extend)?,
            start,
            end,
            &mut |a0, c0, a1, c1| {
                emit(user, a0, c0.into(), a1, c1.into());
            },
        );
        for (d, s) in stops.iter_mut().zip(&v) {
            *d = stop_c(s);
        }
        Ok(())
    })())
}

/// `sofel_bitmap_layout_t`.
#[repr(C)]
#[derive(Default)]
pub struct CBitmapLayout {
    width: u16,
    rows: u16,
    pitch: u32,
    pixel_mode: u32,
    hori_bearing_x: i16,
    hori_bearing_y: i16,
    hori_advance: u16,
    vert_bearing_x: i16,
    vert_bearing_y: i16,
    vert_advance: u16,
}

fn layout_c(l: &sofel_bitmap::BitmapLayout) -> CBitmapLayout {
    use sofel_bitmap::PixelMode;
    let m = l.metrics;
    CBitmapLayout {
        width: l.width,
        rows: l.rows,
        pitch: l.pitch,
        pixel_mode: match l.pixel_mode {
            PixelMode::Mono => 1,
            PixelMode::Gray => 2,
            PixelMode::Gray2 => 3,
            PixelMode::Gray4 => 4,
            PixelMode::Bgra => 7,
        },
        hori_bearing_x: m.hori_bearing_x,
        hori_bearing_y: m.hori_bearing_y,
        hori_advance: m.hori_advance,
        vert_bearing_x: m.vert_bearing_x,
        vert_bearing_y: m.vert_bearing_y,
        vert_advance: m.vert_advance,
    }
}

fn bitmap_error(e: sofel_bitmap::BitmapError) -> i32 {
    use sofel_bitmap::BitmapError;
    match e {
        BitmapError::Missing | BitmapError::InvalidComposite => ERROR_NOT_FOUND,
        BitmapError::Invalid => crate::ERROR_INVALID_FONT,
        BitmapError::Png => crate::ERROR_NO_GLYPH_DATA,
        BitmapError::BufferTooSmall(_) => ERROR_BUFFER_TOO_SMALL,
    }
}

/// `sofel_draw_bitmap` (`pixels` NULL: the layout only).
///
/// # Safety
///
/// `font` is NULL or a live font; `pixels` is NULL or writable for
/// `capacity` bytes; `layout` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_draw_bitmap(
    font: *const SofelFont,
    strike_index: u32,
    glyph: u32,
    pixels: *mut u8,
    capacity: usize,
    layout: *mut CBitmapLayout,
) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, layout) = unsafe { (font_ref(font), w(layout)) };
    let pixels = if pixels.is_null() {
        None
    } else {
        // SAFETY: not NULL; the caller's conditions.
        Some(unsafe { core::slice::from_raw_parts_mut(pixels, capacity) })
    };
    status((|| {
        let (f, layout) = (some(font)?, some(layout)?);
        let s = strike_by_index(f, strike_index).ok_or(ERROR_NOT_FOUND)?;
        let l = sofel_bitmap::bitmap_layout(f, &s, glyph).map_err(bitmap_error)?;
        *layout = layout_c(&l);
        if let Some(out) = pixels {
            if out.len() < l.len() {
                return Err(ERROR_BUFFER_TOO_SMALL);
            }
            let l = sofel_bitmap::draw_bitmap(f, &s, glyph, out).map_err(bitmap_error)?;
            *layout = layout_c(&l);
        }
        Ok(())
    })())
}

/// `sofel_font_has_outlines`.
///
/// # Safety
///
/// `font` is NULL or a live font; `has` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_has_outlines(font: *const SofelFont, has: *mut u8) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, has) = unsafe { (font_ref(font), w(has)) };
    status(get(font, has, |f| u8::from(f.has_outlines())))
}

/// `sofel_font_has_bitmaps`.
///
/// # Safety
///
/// `font` is NULL or a live font; `has` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_font_has_bitmaps(font: *const SofelFont, has: *mut u8) -> i32 {
    // SAFETY: the caller's conditions.
    let (font, has) = unsafe { (font_ref(font), w(has)) };
    status(get(font, has, |f| u8::from(f.has_bitmaps())))
}
