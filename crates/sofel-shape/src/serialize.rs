//! Writing and reading buffers as text (ADR 0020, 直列化): the text and
//! JSON forms of `hb_buffer_serialize_glyphs`,
//! `hb_buffer_serialize_unicode`, `hb_buffer_deserialize_glyphs` and
//! `hb_buffer_deserialize_unicode`, byte for byte.
//!
//! The functions work on slices and vectors rather than on a buffer
//! type, so that the buffers wrap them. The font is a [`SerializeFont`]:
//! glyph names and boxes for writing, glyphs by name and by character
//! for reading (`hb_font_glyph_to_string`, `hb_font_glyph_from_string`).
//! Output is bytes, since glyph names are bytes.
//!
//! The readers are hand-written parsers of the languages of HarfBuzz's
//! Ragel grammars, with the Ragel machines' behaviour on input they do
//! not accept: an action that fails (a number out of range, a glyph name
//! that does not resolve, a glyph in a Unicode buffer) stops with the
//! items added so far and the end at the last item added; a byte the
//! grammar does not allow stops at that byte; input that ends in the
//! middle of an item succeeds without that item. The JSON reader adds an
//! item again for every space after its `,` or `]`, as HarfBuzz's
//! `@add_item` does.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-buffer-serialize.cc`,
//! `src/hb-buffer-deserialize-json.rl`,
//! `src/hb-buffer-deserialize-text-glyphs.rl`,
//! `src/hb-buffer-deserialize-text-unicode.rl` and `src/hb-font.hh`
//! (`glyph_to_string`, `glyph_from_string`), MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;
use core::fmt;

use sofel_font::GlyphExtents;

use crate::feature::{parse_int, parse_uint, tag_from_bytes, write_i32, write_u32};
use crate::glyph::{GlyphFlags, GlyphInfo, GlyphPosition};

/// The two forms (`hb_buffer_serialize_format_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SerializeFormat {
    /// `[name=cluster@dx,dy+ax,ay|...]` and `<U+0041=0|...>`
    /// (`HB_BUFFER_SERIALIZE_FORMAT_TEXT`).
    Text,
    /// `[{"g":...,"cl":...},...]` (`HB_BUFFER_SERIALIZE_FORMAT_JSON`).
    Json,
}

impl SerializeFormat {
    /// The format named `s`, as `hb_buffer_serialize_format_from_string`
    /// reads it: the first four bytes as a tag, padded with spaces, in
    /// any case (`text`, `JSON`, `texts`). `None` for other names.
    pub fn from_name(s: &[u8]) -> Option<SerializeFormat> {
        let tag = tag_from_bytes(s).to_u32() & !0x2020_2020;
        match &tag.to_be_bytes() {
            b"TEXT" => Some(SerializeFormat::Text),
            b"JSON" => Some(SerializeFormat::Json),
            _ => None,
        }
    }
    /// `text` or `json` (`hb_buffer_serialize_format_to_string`).
    pub const fn name(self) -> &'static str {
        match self {
            SerializeFormat::Text => "text",
            SerializeFormat::Json => "json",
        }
    }
}

/// What to write (`hb_buffer_serialize_flags_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct SerializeFlags(pub u32);

impl SerializeFlags {
    /// Everything but boxes and flags (`HB_BUFFER_SERIALIZE_FLAG_DEFAULT`).
    pub const DEFAULT: SerializeFlags = SerializeFlags(0);
    /// No cluster values.
    pub const NO_CLUSTERS: SerializeFlags = SerializeFlags(0x1);
    /// No offsets or advances.
    pub const NO_POSITIONS: SerializeFlags = SerializeFlags(0x2);
    /// Glyph ids instead of names.
    pub const NO_GLYPH_NAMES: SerializeFlags = SerializeFlags(0x4);
    /// The glyph boxes.
    pub const GLYPH_EXTENTS: SerializeFlags = SerializeFlags(0x8);
    /// The glyph flags ([`GlyphFlags::DEFINED`] of them).
    pub const GLYPH_FLAGS: SerializeFlags = SerializeFlags(0x10);
    /// No advances; the offsets then include the advances of the glyphs
    /// before.
    pub const NO_ADVANCES: SerializeFlags = SerializeFlags(0x20);
    /// All defined flags.
    pub const DEFINED: SerializeFlags = SerializeFlags(0x3F);

    /// Whether every flag of `other` is set.
    pub const fn contains(self, other: SerializeFlags) -> bool {
        self.0 & other.0 == other.0
    }
    /// The union.
    pub const fn union(self, other: SerializeFlags) -> SerializeFlags {
        SerializeFlags(self.0 | other.0)
    }
}

/// What serialization asks of the font (the `hb_font_t` argument). Every
/// method has a default that knows nothing, as `hb_font_get_empty`.
pub trait SerializeFont {
    /// The glyph's name (`hb_font_get_glyph_name`), whole; the writer
    /// cuts it as HarfBuzz does.
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        let _ = glyph;
        None
    }
    /// The glyph's box at the font's scale (`hb_font_get_glyph_extents`).
    fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        let _ = glyph;
        None
    }
    /// The glyph with this name (`hb_font_get_glyph_from_name`).
    fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        let _ = name;
        None
    }
    /// The glyph of a character (`hb_font_get_nominal_glyph`).
    fn nominal_glyph(&self, c: u32) -> Option<u32> {
        let _ = c;
        None
    }
}

/// A font that knows nothing (`hb_font_get_empty`): glyphs are written as
/// `gid<n>` and read from numbers only.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct EmptyFont;

impl SerializeFont for EmptyFont {}

/// The size of the buffer HarfBuzz writes names into.
const NAME_SIZE: usize = 128;

/// Appends the glyph's string as `hb_font_glyph_to_string` writes it
/// into the 128 bytes the serializer gives it: the name cut to 127 bytes
/// and at its first 0 byte (`strncpy`), or `gid<n>` without a name.
pub fn glyph_to_string(font: &dyn SerializeFont, glyph: u32, out: &mut Vec<u8>) {
    match font.glyph_name(glyph) {
        Some(name) => out.extend(
            name.iter()
                .take(NAME_SIZE.saturating_sub(1))
                .take_while(|&&b| b != 0),
        ),
        None => {
            out.extend_from_slice(b"gid");
            write_u32(out, glyph);
        }
    }
}

/// The glyph of a string as `hb_font_glyph_from_string` reads it: a
/// glyph name of the font, else a glyph id in decimal, `gid<n>`, or
/// `uni<hex>` mapped through the font's cmap. The numbers are parsed as
/// `hb_parse_uint` does over the whole string (leading whitespace and a
/// sign are allowed; `-1` wraps).
pub fn glyph_from_string(font: &dyn SerializeFont, s: &[u8]) -> Option<u32> {
    let whole =
        |t: &[u8], base: u32| parse_uint(t, base).and_then(|(v, n)| (n == t.len()).then_some(v));
    if let Some(g) = font.glyph_from_name(s) {
        return Some(g);
    }
    if let Some(g) = whole(s, 10) {
        return Some(g);
    }
    if s.len() > 3 {
        let rest = s.get(3..).unwrap_or_default();
        if s.starts_with(b"gid")
            && let Some(g) = whole(rest, 10)
        {
            return Some(g);
        }
        if s.starts_with(b"uni")
            && let Some(g) = whole(rest, 16).and_then(|u| font.nominal_glyph(u))
        {
            return Some(g);
        }
    }
    None
}

/// [`serialize_glyphs_range`] of every glyph.
pub fn serialize_glyphs(
    infos: &[GlyphInfo],
    positions: Option<&[GlyphPosition]>,
    font: &dyn SerializeFont,
    format: SerializeFormat,
    flags: SerializeFlags,
    out: &mut Vec<u8>,
) {
    serialize_glyphs_range(
        infos,
        positions,
        0,
        infos.len(),
        usize::MAX,
        font,
        format,
        flags,
        out,
    );
}

/// Appends the glyphs `start..end` as `hb_buffer_serialize_glyphs` writes
/// them, and returns how many it wrote: `end` is clamped to the glyphs
/// and `start` to `end`. `positions` is `None` for a buffer without
/// positions (they are then left out, as with
/// [`SerializeFlags::NO_POSITIONS`]); missing entries count as zero.
///
/// The text form is `[name=cluster@dx,dy+ax,ay#flags<xb,yb,w,h>|...]`:
/// `@dx,dy` only when one is not zero, `,ay` only when not zero,
/// `#flags` in upper-case hex when set. The JSON form is
/// `[{"g":"name","cl":0,"dx":0,"dy":0,"ax":0,"ay":0,"fl":1,"xb":0,"yb":0,"w":0,"h":0},...]`,
/// with `"` and `\` in names escaped. `[` and `]` are written before
/// glyph 0 and after glyph `end - 1`, so a range is written in pieces
/// that join into the whole. With [`SerializeFlags::NO_ADVANCES`], the
/// offsets are summed with the advances of all glyphs before (from
/// glyph 0), saturating.
///
/// `size` is HarfBuzz's `buf_size`: a glyph is written only when it is
/// shorter than the room left (HarfBuzz keeps a byte for the closing 0),
/// and writing stops at the first glyph that does not fit, so that the
/// caller continues from `start` plus the count returned, as hb-shape
/// does; `usize::MAX` for no limit. `dest` grows by what is written.
#[allow(clippy::too_many_arguments)]
pub fn serialize_glyphs_range(
    infos: &[GlyphInfo],
    positions: Option<&[GlyphPosition]>,
    start: usize,
    end: usize,
    size: usize,
    font: &dyn SerializeFont,
    format: SerializeFormat,
    flags: SerializeFlags,
    dest: &mut Vec<u8>,
) -> usize {
    let mut room = size;
    let end = end.min(infos.len()).max(start);
    let start = start.min(end);
    let positions = positions.filter(|_| !flags.contains(SerializeFlags::NO_POSITIONS));
    let pos = |i: usize| {
        positions
            .and_then(|p| p.get(i))
            .copied()
            .unwrap_or_default()
    };
    let no_advances = positions.is_some() && flags.contains(SerializeFlags::NO_ADVANCES);
    let (mut x, mut y) = if no_advances {
        (0..start).fold((0i32, 0i32), |(x, y), i| {
            (
                x.saturating_add(pos(i).x_advance),
                y.saturating_add(pos(i).y_advance),
            )
        })
    } else {
        (0, 0)
    };
    for (i, info) in infos.iter().enumerate().take(end).skip(start) {
        let mut item = Vec::new();
        let out = &mut item;
        let p = pos(i);
        let last = i.saturating_add(1) == end;
        match format {
            SerializeFormat::Text => {
                out.push(if i == 0 { b'[' } else { b'|' });
                if flags.contains(SerializeFlags::NO_GLYPH_NAMES) {
                    write_u32(out, info.glyph);
                } else {
                    glyph_to_string(font, info.glyph, out);
                }
                if !flags.contains(SerializeFlags::NO_CLUSTERS) {
                    out.push(b'=');
                    write_u32(out, info.cluster);
                }
                if positions.is_some() {
                    let dx = x.saturating_add(p.x_offset);
                    let dy = y.saturating_add(p.y_offset);
                    if dx != 0 || dy != 0 {
                        out.push(b'@');
                        write_i32(out, dx);
                        out.push(b',');
                        write_i32(out, dy);
                    }
                    if !flags.contains(SerializeFlags::NO_ADVANCES) {
                        out.push(b'+');
                        write_i32(out, p.x_advance);
                        if p.y_advance != 0 {
                            out.push(b',');
                            write_i32(out, p.y_advance);
                        }
                    }
                }
                let defined = info.flags.0 & GlyphFlags::DEFINED.0;
                if flags.contains(SerializeFlags::GLYPH_FLAGS) && defined != 0 {
                    out.push(b'#');
                    write_hex_upper(out, defined);
                }
                if flags.contains(SerializeFlags::GLYPH_EXTENTS)
                    && let Some(e) = font.glyph_extents(info.glyph)
                {
                    out.push(b'<');
                    write_i32(out, e.x_bearing);
                    out.push(b',');
                    write_i32(out, e.y_bearing);
                    out.push(b',');
                    write_i32(out, e.width);
                    out.push(b',');
                    write_i32(out, e.height);
                    out.push(b'>');
                }
                if last {
                    out.push(b']');
                }
            }
            SerializeFormat::Json => {
                out.push(if i == 0 { b'[' } else { b',' });
                out.extend_from_slice(b"{\"g\":");
                if flags.contains(SerializeFlags::NO_GLYPH_NAMES) {
                    write_u32(out, info.glyph);
                } else {
                    let mut name = Vec::new();
                    glyph_to_string(font, info.glyph, &mut name);
                    out.push(b'"');
                    for &b in &name {
                        if b == b'"' || b == b'\\' {
                            out.push(b'\\');
                        }
                        out.push(b);
                    }
                    out.push(b'"');
                }
                if !flags.contains(SerializeFlags::NO_CLUSTERS) {
                    out.extend_from_slice(b",\"cl\":");
                    write_u32(out, info.cluster);
                }
                if positions.is_some() {
                    out.extend_from_slice(b",\"dx\":");
                    write_i32(out, x.saturating_add(p.x_offset));
                    out.extend_from_slice(b",\"dy\":");
                    write_i32(out, y.saturating_add(p.y_offset));
                    if !flags.contains(SerializeFlags::NO_ADVANCES) {
                        out.extend_from_slice(b",\"ax\":");
                        write_i32(out, p.x_advance);
                        out.extend_from_slice(b",\"ay\":");
                        write_i32(out, p.y_advance);
                    }
                }
                let defined = info.flags.0 & GlyphFlags::DEFINED.0;
                if flags.contains(SerializeFlags::GLYPH_FLAGS) && defined != 0 {
                    out.extend_from_slice(b",\"fl\":");
                    write_u32(out, defined);
                }
                if flags.contains(SerializeFlags::GLYPH_EXTENTS)
                    && let Some(e) = font.glyph_extents(info.glyph)
                {
                    out.extend_from_slice(b",\"xb\":");
                    write_i32(out, e.x_bearing);
                    out.extend_from_slice(b",\"yb\":");
                    write_i32(out, e.y_bearing);
                    out.extend_from_slice(b",\"w\":");
                    write_i32(out, e.width);
                    out.extend_from_slice(b",\"h\":");
                    write_i32(out, e.height);
                }
                out.push(b'}');
                if last {
                    out.push(b']');
                }
            }
        }
        if room <= item.len() {
            return i.saturating_sub(start);
        }
        room = room.saturating_sub(item.len());
        dest.extend_from_slice(&item);
        if no_advances {
            x = x.saturating_add(p.x_advance);
            y = y.saturating_add(p.y_advance);
        }
    }
    end.saturating_sub(start)
}

/// [`serialize_unicode_range`] of every character.
pub fn serialize_unicode(
    codepoints: &[u32],
    clusters: &[u32],
    format: SerializeFormat,
    flags: SerializeFlags,
    out: &mut Vec<u8>,
) {
    serialize_unicode_range(
        codepoints,
        clusters,
        0,
        codepoints.len(),
        usize::MAX,
        format,
        flags,
        out,
    );
}

/// Appends the characters `start..end` as `hb_buffer_serialize_unicode`
/// writes them, and returns how many it wrote. The characters are the
/// pairs of `codepoints` and `clusters` (as many as the shorter has).
/// The text form is `<U+0041=0|...>` (at least four upper-case hex
/// digits), the JSON form `[{"u":65,"cl":0},...]`; only
/// [`SerializeFlags::NO_CLUSTERS`] applies. The brackets and `size` work
/// as for [`serialize_glyphs_range`].
#[allow(clippy::too_many_arguments)]
pub fn serialize_unicode_range(
    codepoints: &[u32],
    clusters: &[u32],
    start: usize,
    end: usize,
    size: usize,
    format: SerializeFormat,
    flags: SerializeFlags,
    dest: &mut Vec<u8>,
) -> usize {
    let mut room = size;
    let len = codepoints.len().min(clusters.len());
    let end = end.min(len).max(start);
    let start = start.min(end);
    let chars = codepoints
        .iter()
        .zip(clusters)
        .enumerate()
        .take(end)
        .skip(start);
    for (i, (&u, &cluster)) in chars {
        let mut item = Vec::new();
        let out = &mut item;
        let last = i.saturating_add(1) == end;
        match format {
            SerializeFormat::Text => {
                out.push(if i == 0 { b'<' } else { b'|' });
                out.extend_from_slice(b"U+");
                let mut hex = Vec::new();
                write_hex_upper(&mut hex, u);
                out.extend(core::iter::repeat_n(b'0', 4usize.saturating_sub(hex.len())));
                out.extend_from_slice(&hex);
                if !flags.contains(SerializeFlags::NO_CLUSTERS) {
                    out.push(b'=');
                    write_u32(out, cluster);
                }
                if last {
                    out.push(b'>');
                }
            }
            SerializeFormat::Json => {
                out.push(if i == 0 { b'[' } else { b',' });
                out.extend_from_slice(b"{\"u\":");
                write_u32(out, u);
                if !flags.contains(SerializeFlags::NO_CLUSTERS) {
                    out.extend_from_slice(b",\"cl\":");
                    write_u32(out, cluster);
                }
                out.push(b'}');
                if last {
                    out.push(b']');
                }
            }
        }
        if room <= item.len() {
            return i.saturating_sub(start);
        }
        room = room.saturating_sub(item.len());
        dest.extend_from_slice(&item);
    }
    end.saturating_sub(start)
}

/// Appends what `hb_buffer_serialize` writes for an empty buffer whose
/// content is neither glyphs nor characters
/// (`_hb_buffer_serialize_invalid`): `!!` in text, `[]` in JSON.
pub fn serialize_invalid(format: SerializeFormat, out: &mut Vec<u8>) {
    out.extend_from_slice(match format {
        SerializeFormat::Text => b"!!",
        SerializeFormat::Json => b"[]",
    });
}

/// Appends `v` in upper-case hex (`%X`).
fn write_hex_upper(out: &mut Vec<u8>, v: u32) {
    let digits = (1..8u32)
        .rev()
        .find(|&k| v >> k.saturating_mul(4) != 0)
        .map_or(1, |k| k.saturating_add(1));
    for k in (0..digits).rev() {
        let d = (v >> k.saturating_mul(4)) & 0xF;
        out.push(
            b"0123456789ABCDEF"
                .get(usize::try_from(d).unwrap_or(0))
                .copied()
                .unwrap_or(b'0'),
        );
    }
}

/// Reading stopped before the end (`hb_buffer_deserialize_*` returned
/// `false`). The items read before the stop are kept in the output.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DeserializeError {
    /// How many bytes were read: HarfBuzz's `end_ptr - buf`. At a byte
    /// the grammar does not allow, its position; when an item's action
    /// fails, where the last item was added (0 if none was).
    pub consumed: usize,
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot read the buffer after byte {}", self.consumed)
    }
}

impl core::error::Error for DeserializeError {}

/// Appends the glyphs of `text` to `infos` and `positions` as
/// `hb_buffer_deserialize_glyphs` reads them, with `font` for names
/// ([`glyph_from_string`]). The glyph flags hold the `#` or `"fl"` value
/// as it is. `Ok` when the whole text was read (HarfBuzz returns `true`);
/// input that ends in the middle of an item is read without that item.
/// Empty text is an error, as in HarfBuzz.
pub fn deserialize_glyphs(
    text: &[u8],
    format: SerializeFormat,
    font: &dyn SerializeFont,
    infos: &mut Vec<GlyphInfo>,
    positions: &mut Vec<GlyphPosition>,
) -> Result<(), DeserializeError> {
    let mut add = |item: Item| {
        infos.push(GlyphInfo {
            glyph: item.codepoint,
            cluster: item.cluster,
            flags: GlyphFlags(item.mask),
        });
        positions.push(item.pos);
    };
    match format {
        SerializeFormat::Text => run(text, |r| text_glyphs(r, font, &mut add)),
        SerializeFormat::Json => run(text, |r| json(r, Content::Glyphs, font, &mut add)),
    }
}

/// Appends the characters of `text` to `codepoints` and `clusters` as
/// `hb_buffer_deserialize_unicode` reads them; see
/// [`deserialize_glyphs`]. The JSON form's positions and flags, which a
/// Unicode buffer reads but does not use, are not kept.
pub fn deserialize_unicode(
    text: &[u8],
    format: SerializeFormat,
    codepoints: &mut Vec<u32>,
    clusters: &mut Vec<u32>,
) -> Result<(), DeserializeError> {
    let mut add = |item: Item| {
        codepoints.push(item.codepoint);
        clusters.push(item.cluster);
    };
    match format {
        SerializeFormat::Text => run(text, |r| text_unicode(r, &mut add)),
        SerializeFormat::Json => run(text, |r| json(r, Content::Unicode, &EmptyFont, &mut add)),
    }
}

/// `hb_glyph_info_t` and `hb_glyph_position_t` as the grammars fill them.
#[derive(Clone, Copy, Default)]
struct Item {
    codepoint: u32,
    cluster: u32,
    mask: u32,
    pos: GlyphPosition,
}

/// Why a machine stopped.
enum Stop {
    /// The input ended; HarfBuzz returns `true`.
    Eof,
    /// No transition on the byte at this position.
    Error(usize),
    /// An action returned `false`.
    Fail,
}

/// The reader's state: the input, the position (Ragel's `p`) and the end
/// set by the last `add_item` (`*end_ptr`).
struct Reader<'a> {
    s: &'a [u8],
    pos: usize,
    end_ptr: usize,
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Result<u8, Stop> {
        self.s.get(self.pos).copied().ok_or(Stop::Eof)
    }
    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }
    fn error<T>(&self) -> Result<T, Stop> {
        Err(Stop::Error(self.pos))
    }
    /// The bytes from `start` to the position.
    fn token(&self, start: usize) -> &'a [u8] {
        self.s.get(start..self.pos).unwrap_or_default()
    }
    /// Consumes `c` or stops.
    fn expect(&mut self, c: u8) -> Result<(), Stop> {
        if self.peek()? == c {
            self.bump();
            Ok(())
        } else {
            self.error()
        }
    }
    /// Stops unless the next byte is one of `set`, without consuming it:
    /// the leaving actions run only on a transition.
    fn follow(&self, set: &[u8]) -> Result<u8, Stop> {
        let c = self.peek()?;
        if set.contains(&c) {
            Ok(c)
        } else {
            self.error()
        }
    }
    /// `space*`.
    fn spaces(&mut self) {
        while self
            .s
            .get(self.pos)
            .is_some_and(|&b| crate::feature::is_space(b))
        {
            self.bump();
        }
    }
    /// `unum = '0' | [1-9] digit*`: its bytes.
    fn unum(&mut self) -> Result<&'a [u8], Stop> {
        let start = self.pos;
        match self.peek()? {
            b'0' => self.bump(),
            b'1'..=b'9' => {
                while self.s.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.bump();
                }
            }
            _ => return self.error(),
        }
        Ok(self.token(start))
    }
    /// `num = '-'? unum`: its bytes.
    fn num(&mut self) -> Result<&'a [u8], Stop> {
        let start = self.pos;
        if self.peek()? == b'-' {
            self.bump();
        }
        self.unum()?;
        Ok(self.token(start))
    }
}

/// `parse_uint` of `hb-buffer-serialize.cc`: the whole token in base 10.
fn uint(t: &[u8]) -> Result<u32, Stop> {
    parse_uint(t, 10)
        .and_then(|(v, n)| (n == t.len()).then_some(v))
        .ok_or(Stop::Fail)
}

/// `parse_int` of `hb-buffer-serialize.cc`.
fn int(t: &[u8]) -> Result<i32, Stop> {
    parse_int(t)
        .and_then(|(v, n)| (n == t.len()).then_some(v))
        .ok_or(Stop::Fail)
}

/// `parse_hex`.
fn hex(t: &[u8]) -> Result<u32, Stop> {
    parse_uint(t, 16)
        .and_then(|(v, n)| (n == t.len()).then_some(v))
        .ok_or(Stop::Fail)
}

/// Runs a machine over `s` and gives HarfBuzz's result.
fn run(
    s: &[u8],
    machine: impl FnOnce(&mut Reader<'_>) -> Result<(), Stop>,
) -> Result<(), DeserializeError> {
    if s.is_empty() {
        return Err(DeserializeError { consumed: 0 });
    }
    let mut r = Reader {
        s,
        pos: 0,
        end_ptr: 0,
    };
    match machine(&mut r) {
        Ok(()) | Err(Stop::Eof) => Ok(()),
        Err(Stop::Error(at)) => Err(DeserializeError { consumed: at }),
        Err(Stop::Fail) => Err(DeserializeError {
            consumed: r.end_ptr,
        }),
    }
}

/// `hb-buffer-deserialize-text-glyphs.rl`:
/// `'['? glyph_item*`, each item ending with `|` or `]` and added when
/// the next one starts or at the end of the input.
fn text_glyphs(
    r: &mut Reader<'_>,
    font: &dyn SerializeFont,
    add: &mut dyn FnMut(Item),
) -> Result<(), Stop> {
    // A `[` may also be the first byte of a glyph name; the machine
    // takes the token from after it either way.
    if r.peek()? == b'[' {
        r.bump();
    }
    let mut pending: Option<Item> = None;
    loop {
        let c = match r.peek() {
            Ok(c) => c,
            Err(stop) => {
                // The end of the input in a final state runs %add_item.
                if let Some(item) = pending.take() {
                    add(item);
                    r.end_ptr = r.pos;
                }
                return Err(stop);
            }
        };
        // Every byte but `,` starts an item (a name, or an empty name
        // followed by another part).
        if c == b',' {
            return r.error();
        }
        if let Some(item) = pending.take() {
            add(item);
            r.end_ptr = r.pos;
        }
        pending = Some(text_glyph_item(r, font)?);
    }
}

/// A glyph name's bytes: anything but `\ ] = @ + , # |`, and those but
/// `#` escaped with `\`. `<` is one too, and also starts the box.
fn is_name_byte(b: u8) -> bool {
    !matches!(b, b'\\' | b']' | b'=' | b'@' | b'+' | b',' | b'#' | b'|')
}

/// One `glyph_item`, through its `|` or `]`.
fn text_glyph_item(r: &mut Reader<'_>, font: &dyn SerializeFont) -> Result<Item, Stop> {
    let mut item = Item::default();
    let tok = r.pos;
    loop {
        let c = r.peek()?;
        if c == b'\\' {
            r.bump();
            if matches!(r.peek()?, b'\\' | b']' | b'=' | b'@' | b'+' | b',' | b'|') {
                r.bump();
            } else {
                return r.error();
            }
        } else if c == b'<' {
            // `<` both continues the name and starts the box, so the
            // machine leaves the glyph here (running parse_glyph) and
            // follows both until one dies: the name at the box's first
            // `,`, the box at a byte that is not part of its first
            // number.
            item.codepoint = glyph_from_string(font, r.token(tok)).ok_or(Stop::Fail)?;
            let at = r.pos;
            r.bump();
            let is_box = r.num().is_ok() && r.peek().is_ok_and(|b| b == b',');
            r.pos = at;
            if is_box {
                break;
            }
            r.bump();
        } else if is_name_byte(c) {
            r.bump();
        } else {
            break;
        }
    }
    let mut c = r.follow(b"]=@+#|<")?;
    item.codepoint = glyph_from_string(font, r.token(tok)).ok_or(Stop::Fail)?;
    if c == b'=' {
        r.bump();
        let t = r.unum()?;
        c = r.follow(b"@+#<|]")?;
        item.cluster = uint(t)?;
    }
    if c == b'@' {
        r.bump();
        let t = r.num()?;
        r.follow(b",")?;
        item.pos.x_offset = int(t)?;
        r.bump();
        let t = r.num()?;
        c = r.follow(b"+#<|]")?;
        item.pos.y_offset = int(t)?;
    }
    if c == b'+' {
        r.bump();
        let t = r.num()?;
        c = r.follow(b",#<|]")?;
        item.pos.x_advance = int(t)?;
        if c == b',' {
            r.bump();
            let t = r.num()?;
            c = r.follow(b"#<|]")?;
            item.pos.y_advance = int(t)?;
        }
    }
    if c == b'#' {
        r.bump();
        let t = r.unum()?;
        c = r.follow(b"<|]")?;
        item.mask = uint(t)?;
    }
    if c == b'<' {
        // Not parsed.
        r.bump();
        for &sep in b",,,>" {
            r.num()?;
            r.expect(sep)?;
        }
        c = r.follow(b"|]")?;
    }
    // `|` or `]`.
    let _ = c;
    r.bump();
    Ok(item)
}

/// `hb-buffer-deserialize-text-unicode.rl`: `'<'? unicode_item*`.
fn text_unicode(r: &mut Reader<'_>, add: &mut dyn FnMut(Item)) -> Result<(), Stop> {
    if r.peek()? == b'<' {
        r.bump();
    }
    let mut pending: Option<Item> = None;
    loop {
        let c = match r.peek() {
            Ok(c) => c,
            Err(stop) => {
                if let Some(item) = pending.take() {
                    add(item);
                    r.end_ptr = r.pos;
                }
                return Err(stop);
            }
        };
        if !matches!(c, b'U' | b'u') {
            return r.error();
        }
        if let Some(item) = pending.take() {
            add(item);
            r.end_ptr = r.pos;
        }
        r.bump();
        let mut item = Item::default();
        if r.peek()? == b'+' {
            r.bump();
        }
        let tok = r.pos;
        if !r.peek()?.is_ascii_hexdigit() {
            return r.error();
        }
        while r.s.get(r.pos).is_some_and(u8::is_ascii_hexdigit) {
            r.bump();
        }
        let t = r.token(tok);
        let mut c = r.follow(b"=|>")?;
        item.codepoint = hex(t)?;
        if c == b'=' {
            r.bump();
            let t = r.unum()?;
            c = r.follow(b"|>")?;
            item.cluster = uint(t)?;
        }
        let _ = c;
        r.bump();
        pending = Some(item);
    }
}

/// The buffer's content, for `ensure_glyphs` and `ensure_unicode`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Content {
    Glyphs,
    Unicode,
}

/// The keys of the JSON elements.
const KEYS: [&[u8]; 12] = [
    b"g", b"u", b"cl", b"dx", b"dy", b"ax", b"ay", b"fl", b"xb", b"yb", b"w", b"h",
];

/// `hb-buffer-deserialize-json.rl`: `space* '['? space* item*`, each item
/// `{...}` with `,` or `]` and spaces after it, added on each of those.
fn json(
    r: &mut Reader<'_>,
    content: Content,
    font: &dyn SerializeFont,
    add: &mut dyn FnMut(Item),
) -> Result<(), Stop> {
    r.spaces();
    if r.peek()? == b'[' {
        r.bump();
    }
    r.spaces();
    loop {
        r.expect(b'{')?;
        let mut item = Item::default();
        r.spaces();
        loop {
            json_element(r, content, font, &mut item)?;
            r.spaces();
            if r.peek()? == b',' {
                r.bump();
                r.spaces();
            } else {
                break;
            }
        }
        r.expect(b'}')?;
        r.spaces();
        if !matches!(r.peek()?, b',' | b']') {
            return r.error();
        }
        // @add_item runs on the transition into each final state: the
        // `,` or `]` and every space after it.
        add(item);
        r.end_ptr = r.pos;
        r.bump();
        while r.s.get(r.pos).is_some_and(|&b| crate::feature::is_space(b)) {
            add(item);
            r.end_ptr = r.pos;
            r.bump();
        }
    }
}

/// One element: a key, `:` and its value.
fn json_element(
    r: &mut Reader<'_>,
    content: Content,
    font: &dyn SerializeFont,
    item: &mut Item,
) -> Result<(), Stop> {
    r.expect(b'"')?;
    let start = r.pos;
    // Walk the keys byte by byte, as the machine does.
    let key = loop {
        let c = r.peek()?;
        let len = r.pos.saturating_sub(start);
        if c == b'"' {
            if let Some(k) = KEYS
                .iter()
                .find(|k| k.len() == len && **k == r.token(start))
            {
                r.bump();
                break *k;
            }
            return r.error();
        }
        let prefix = r.token(start);
        if !KEYS
            .iter()
            .any(|k| k.len() > len && k.starts_with(prefix) && k.get(len) == Some(&c))
        {
            return r.error();
        }
        r.bump();
    };
    r.spaces();
    r.expect(b':')?;
    r.spaces();
    let ensure = |wanted: Content| {
        if content == wanted {
            Ok(())
        } else {
            Err(Stop::Fail)
        }
    };
    match key {
        b"g" => {
            if r.peek()? == b'"' {
                let tok = r.pos;
                r.bump();
                loop {
                    match r.peek()? {
                        b'"' => break,
                        b'\\' => {
                            r.bump();
                            if matches!(r.peek()?, b'\\' | b'"') {
                                r.bump();
                            } else {
                                return r.error();
                            }
                        }
                        _ => r.bump(),
                    }
                }
                r.bump();
                // @ensure_glyphs on the closing quote.
                ensure(Content::Glyphs)?;
                follow_value(r)?;
                let name =
                    r.s.get(tok.saturating_add(1)..r.pos.saturating_sub(1))
                        .unwrap_or_default();
                item.codepoint = glyph_from_string(font, name).ok_or(Stop::Fail)?;
            } else {
                // @ensure_glyphs on the first digit.
                if r.peek()?.is_ascii_digit() {
                    ensure(Content::Glyphs)?;
                }
                let t = r.unum()?;
                follow_value(r)?;
                item.codepoint = uint(t)?;
            }
        }
        b"u" => {
            if r.peek()?.is_ascii_digit() {
                ensure(Content::Unicode)?;
            }
            let t = r.unum()?;
            follow_value(r)?;
            item.codepoint = uint(t)?;
        }
        b"cl" | b"fl" => {
            let t = r.unum()?;
            follow_value(r)?;
            let v = uint(t)?;
            if key == b"cl" {
                item.cluster = v;
            } else {
                item.mask = v;
            }
        }
        b"dx" | b"dy" | b"ax" | b"ay" => {
            let t = r.num()?;
            follow_value(r)?;
            let v = int(t)?;
            match key {
                b"dx" => item.pos.x_offset = v,
                b"dy" => item.pos.y_offset = v,
                b"ax" => item.pos.x_advance = v,
                _ => item.pos.y_advance = v,
            }
        }
        _ => {
            // The box: read, not parsed.
            r.num()?;
            follow_value(r)?;
        }
    }
    Ok(())
}

/// After a value: a space, `,` or `}`, on which the leaving action runs.
fn follow_value(r: &Reader<'_>) -> Result<(), Stop> {
    let c = r.peek()?;
    if c == b',' || c == b'}' || crate::feature::is_space(c) {
        Ok(())
    } else {
        r.error()
    }
}
