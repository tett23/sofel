//! Builders for small sfnt fonts whose values are known by construction.
//!
//! Shared by the `sofel-read` tests and the `sofel-conformance` tests
//! (ADR 0007, ADR 0008). Builders write exactly what they are told, so
//! tests can also build broken data on purpose.

#![allow(
    dead_code,
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing
)]

pub fn u16be(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

#[derive(Default, Clone)]
pub struct Buf(pub Vec<u8>);

impl Buf {
    pub fn new() -> Self {
        Buf(Vec::new())
    }
    pub fn u8(mut self, v: u8) -> Self {
        self.0.push(v);
        self
    }
    pub fn u16(mut self, v: u16) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn i16(mut self, v: i16) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u24(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes()[1..]);
        self
    }
    pub fn u32(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn i32(mut self, v: i32) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn i64(mut self, v: i64) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn bytes(mut self, v: &[u8]) -> Self {
        self.0.extend_from_slice(v);
        self
    }
    pub fn tag(self, t: &[u8; 4]) -> Self {
        self.bytes(t)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn done(self) -> Vec<u8> {
        self.0
    }
}

pub fn checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for chunk in data.chunks(4) {
        let mut w = [0u8; 4];
        w[..chunk.len()].copy_from_slice(chunk);
        sum = sum.wrapping_add(u32::from_be_bytes(w));
    }
    sum
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

/// Table directory entries for `tables`, laid out after a directory that
/// starts at `dir_start` (file offset) and whose table data starts at
/// `data_start`. Returns (directory bytes, table data bytes).
fn directory(
    sfnt_version: u32,
    tables: &[(&[u8; 4], Vec<u8>)],
    data_start: usize,
) -> (Vec<u8>, Vec<u8>) {
    let n = tables.len() as u16;
    let mut entry_selector = 0u16;
    while (1u32 << (entry_selector + 1)) <= n as u32 {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = n * 16 - search_range.min(n * 16);
    let mut dir = Buf::new()
        .u32(sfnt_version)
        .u16(n)
        .u16(search_range)
        .u16(entry_selector)
        .u16(range_shift);
    let mut data = Vec::new();
    for (tag, bytes) in tables {
        let offset = data_start + data.len();
        let sum = if *tag == b"head" && bytes.len() >= 12 {
            let mut z = bytes.clone();
            z[8..12].copy_from_slice(&[0, 0, 0, 0]);
            checksum(&z)
        } else {
            checksum(bytes)
        };
        dir = dir
            .tag(tag)
            .u32(sum)
            .u32(offset as u32)
            .u32(bytes.len() as u32);
        data.extend_from_slice(bytes);
        pad4(&mut data);
    }
    (dir.done(), data)
}

/// A single-font sfnt file with the given tables, in the given order.
pub fn sfnt(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    sfnt_with_version(0x0001_0000, tables)
}

pub fn sfnt_with_version(version: u32, tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let dir_len = 12 + 16 * tables.len();
    let (mut dir, data) = directory(version, tables, dir_len);
    dir.extend_from_slice(&data);
    dir
}

/// A TTC (version 1 or 2) holding one font per entry of `fonts`.
pub fn ttc(version2: bool, fonts: &[Vec<(&[u8; 4], Vec<u8>)>]) -> Vec<u8> {
    let header_len = 12 + 4 * fonts.len() + if version2 { 12 } else { 0 };
    let mut offsets = Vec::new();
    let mut body = Vec::new();
    for tables in fonts {
        let dir_start = header_len + body.len();
        let dir_len = 12 + 16 * tables.len();
        let (dir, data) = directory(0x0001_0000, tables, dir_start + dir_len);
        offsets.push(dir_start as u32);
        body.extend_from_slice(&dir);
        body.extend_from_slice(&data);
        pad4(&mut body);
    }
    let mut head = Buf::new()
        .tag(b"ttcf")
        .u16(if version2 { 2 } else { 1 })
        .u16(0)
        .u32(fonts.len() as u32);
    for o in offsets {
        head = head.u32(o);
    }
    if version2 {
        head = head.u32(0).u32(0).u32(0);
    }
    let mut out = head.done();
    out.extend_from_slice(&body);
    out
}

pub struct HeadSpec {
    pub units_per_em: u16,
    pub index_to_loc_format: i16,
    pub magic: u32,
    pub major: u16,
}

impl Default for HeadSpec {
    fn default() -> Self {
        HeadSpec {
            units_per_em: 1000,
            index_to_loc_format: 0,
            magic: 0x5F0F_3CF5,
            major: 1,
        }
    }
}

/// A 54-byte head table. Fields not in `HeadSpec` have fixed, distinct values.
pub fn head(spec: HeadSpec) -> Vec<u8> {
    Buf::new()
        .u16(spec.major)
        .u16(0)
        .i32(0x0002_8000) // fontRevision 2.5
        .u32(0) // checksumAdjustment
        .u32(spec.magic)
        .u16(0x000B) // flags
        .u16(spec.units_per_em)
        .i64(3_000_000_000) // created
        .i64(3_100_000_000) // modified
        .i16(-50) // xMin
        .i16(-200) // yMin
        .i16(950) // xMax
        .i16(800) // yMax
        .u16(0x0001) // macStyle
        .u16(9) // lowestRecPPEM
        .i16(2) // fontDirectionHint
        .i16(spec.index_to_loc_format)
        .i16(0) // glyphDataFormat
        .done()
}

pub fn maxp05(num_glyphs: u16) -> Vec<u8> {
    Buf::new().u32(0x0000_5000).u16(num_glyphs).done()
}

/// maxp version 1.0; the TrueType maxima are 1..=13 in field order.
pub fn maxp10(num_glyphs: u16) -> Vec<u8> {
    let mut b = Buf::new().u32(0x0001_0000).u16(num_glyphs);
    for v in 1..=13u16 {
        b = b.u16(v);
    }
    b.done()
}

/// MAXP (24-bit numGlyphs), version 0.5.
pub fn maxp_wide05(num_glyphs: u32) -> Vec<u8> {
    Buf::new().u32(0x0000_5000).u24(num_glyphs).done()
}

/// hhea with distinct field values and the given numberOfHMetrics.
pub fn hhea(num_long_metrics: u16) -> Vec<u8> {
    hhea_fields(Buf::new()).u16(num_long_metrics).done()
}

/// HHEA (24-bit numberOfHMetrics).
pub fn hhea_wide(num_long_metrics: u32) -> Vec<u8> {
    hhea_fields(Buf::new()).u24(num_long_metrics).done()
}

fn hhea_fields(b: Buf) -> Buf {
    b.u16(1)
        .u16(0)
        .i16(800) // ascender
        .i16(-200) // descender
        .i16(90) // lineGap
        .u16(1200) // advanceWidthMax
        .i16(-30) // minLeftSideBearing
        .i16(-40) // minRightSideBearing
        .i16(1100) // xMaxExtent
        .i16(1) // caretSlopeRise
        .i16(0) // caretSlopeRun
        .i16(5) // caretOffset
        .i16(0)
        .i16(0)
        .i16(0)
        .i16(0)
        .i16(0) // metricDataFormat
}

/// hmtx with `(advance, lsb)` long metrics followed by `lsbs`.
pub fn hmtx(long: &[(u16, i16)], lsbs: &[i16]) -> Vec<u8> {
    let mut b = Buf::new();
    for (a, l) in long {
        b = b.u16(*a).i16(*l);
    }
    for l in lsbs {
        b = b.i16(*l);
    }
    b.done()
}

/// A cmap table with `(platform, encoding, subtable)` records. Subtables
/// are laid out in record order right after the record array.
pub fn cmap(records: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut b = Buf::new().u16(0).u16(records.len() as u16);
    let mut offset = 4 + 8 * records.len();
    let mut body = Vec::new();
    for (p, e, sub) in records {
        b = b.u16(*p).u16(*e).u32(offset as u32);
        body.extend_from_slice(sub);
        offset += sub.len();
    }
    let mut out = b.done();
    out.extend_from_slice(&body);
    out
}

pub fn cmap0(ids: &[u8; 256]) -> Vec<u8> {
    Buf::new().u16(0).u16(262).u16(0).bytes(ids).done()
}

/// A format 2 subtable. `sub_header_keys` holds the subHeader index
/// (not yet multiplied by 8) for each high byte. Each sub header is
/// `(first_code, id_delta, glyph_ids)`; its idRangeOffset is computed.
pub fn cmap2(sub_header_keys: &[u16; 256], sub_headers: &[(u16, i16, Vec<u16>)]) -> Vec<u8> {
    let n = sub_headers.len();
    let mut glyph_arrays = Vec::new();
    let mut array_offsets = Vec::new();
    for (_, _, ids) in sub_headers {
        array_offsets.push(glyph_arrays.len());
        glyph_arrays.extend_from_slice(ids);
    }
    let length = 6 + 512 + 8 * n + 2 * glyph_arrays.len();
    let mut b = Buf::new().u16(2).u16(length as u16).u16(0);
    for k in sub_header_keys {
        b = b.u16(k * 8);
    }
    for (i, (first, delta, ids)) in sub_headers.iter().enumerate() {
        // idRangeOffset is relative to the idRangeOffset field itself.
        let field_pos = 6 + 512 + 8 * i + 6;
        let array_pos = 6 + 512 + 8 * n + 2 * array_offsets[i];
        b = b
            .u16(*first)
            .u16(ids.len() as u16)
            .i16(*delta)
            .u16((array_pos - field_pos) as u16);
    }
    for g in glyph_arrays {
        b = b.u16(g);
    }
    b.done()
}

/// A segment of a format 4 subtable. With `glyphs`, the segment uses
/// idRangeOffset into the glyph id array; otherwise idDelta only.
pub struct Seg {
    pub start: u16,
    pub end: u16,
    pub delta: i16,
    pub glyphs: Option<Vec<u16>>,
}

pub fn seg(start: u16, end: u16, delta: i16) -> Seg {
    Seg {
        start,
        end,
        delta,
        glyphs: None,
    }
}

pub fn seg_ids(start: u16, end: u16, delta: i16, glyphs: Vec<u16>) -> Seg {
    Seg {
        start,
        end,
        delta,
        glyphs: Some(glyphs),
    }
}

/// A format 4 subtable with correct search parameters. Add the final
/// 0xFFFF segment yourself when you want it.
pub fn cmap4(segs: &[Seg]) -> Vec<u8> {
    let n = segs.len();
    let seg_x2 = (n * 2) as u16;
    let mut entry_selector = 0u16;
    while n > 0 && (1usize << (entry_selector + 1)) <= n {
        entry_selector += 1;
    }
    let search_range = if n == 0 {
        0
    } else {
        (1u16 << entry_selector) * 2
    };
    let range_shift = seg_x2 - search_range;
    let mut glyph_array: Vec<u16> = Vec::new();
    let mut range_offsets = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        match &s.glyphs {
            None => range_offsets.push(0u16),
            Some(g) => {
                // offset from &idRangeOffset[i] to &glyphIdArray[k]
                let from_field = 2 * (n - i);
                let ro = from_field + 2 * glyph_array.len();
                range_offsets.push(ro as u16);
                glyph_array.extend_from_slice(g);
            }
        }
    }
    let length = 16 + 8 * n + 2 * glyph_array.len();
    let mut b = Buf::new()
        .u16(4)
        .u16(length as u16)
        .u16(0)
        .u16(seg_x2)
        .u16(search_range)
        .u16(entry_selector)
        .u16(range_shift);
    for s in segs {
        b = b.u16(s.end);
    }
    b = b.u16(0);
    for s in segs {
        b = b.u16(s.start);
    }
    for s in segs {
        b = b.i16(s.delta);
    }
    for r in &range_offsets {
        b = b.u16(*r);
    }
    for g in glyph_array {
        b = b.u16(g);
    }
    b.done()
}

pub fn cmap6(first: u16, ids: &[u16]) -> Vec<u8> {
    let mut b = Buf::new()
        .u16(6)
        .u16((10 + 2 * ids.len()) as u16)
        .u16(0)
        .u16(first)
        .u16(ids.len() as u16);
    for g in ids {
        b = b.u16(*g);
    }
    b.done()
}

/// A format 8 subtable with the given is32 bitmap and groups.
pub fn cmap8(is32: &[u8; 8192], groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let length = 16 + 8192 + 12 * groups.len();
    let mut b = Buf::new()
        .u16(8)
        .u16(0)
        .u32(length as u32)
        .u32(0)
        .bytes(is32)
        .u32(groups.len() as u32);
    for (s, e, g) in groups {
        b = b.u32(*s).u32(*e).u32(*g);
    }
    b.done()
}

pub fn cmap10(start: u32, ids: &[u16]) -> Vec<u8> {
    let mut b = Buf::new()
        .u16(10)
        .u16(0)
        .u32((20 + 2 * ids.len()) as u32)
        .u32(0)
        .u32(start)
        .u32(ids.len() as u32);
    for g in ids {
        b = b.u16(*g);
    }
    b.done()
}

/// Format 12 (`format` 12) or 13 (`format` 13) with `(start, end, glyph)`.
pub fn cmap12_13(format: u16, groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut b = Buf::new()
        .u16(format)
        .u16(0)
        .u32((16 + 12 * groups.len()) as u32)
        .u32(0)
        .u32(groups.len() as u32);
    for (s, e, g) in groups {
        b = b.u32(*s).u32(*e).u32(*g);
    }
    b.done()
}

/// One variation selector record of a format 14 or 15 subtable:
/// default ranges `(start, additional_count)` and non-default mappings
/// `(unicode, glyph)`. `None` means the offset is 0.
pub struct Uvs {
    pub selector: u32,
    pub defaults: Option<Vec<(u32, u8)>>,
    pub mappings: Option<Vec<(u32, u32)>>,
}

/// Format 14 (`wide` false, 16-bit glyphs) or 15 (`wide` true, 24-bit glyphs).
pub fn cmap14_15(wide: bool, records: &[Uvs]) -> Vec<u8> {
    let header = 10 + 11 * records.len();
    let mut tail: Vec<u8> = Vec::new();
    let mut recs = Buf::new();
    for r in records {
        let def_off = match &r.defaults {
            None => 0,
            Some(ranges) => {
                let off = header + tail.len();
                let mut t = Buf::new().u32(ranges.len() as u32);
                for (s, c) in ranges {
                    t = t.u24(*s).u8(*c);
                }
                tail.extend_from_slice(&t.done());
                off
            }
        };
        let nondef_off = match &r.mappings {
            None => 0,
            Some(maps) => {
                let off = header + tail.len();
                let mut t = Buf::new().u32(maps.len() as u32);
                for (u, g) in maps {
                    t = t.u24(*u);
                    t = if wide { t.u24(*g) } else { t.u16(*g as u16) };
                }
                tail.extend_from_slice(&t.done());
                off
            }
        };
        recs = recs
            .u24(r.selector)
            .u32(def_off as u32)
            .u32(nondef_off as u32);
    }
    let length = header + tail.len();
    let mut out = Buf::new()
        .u16(if wide { 15 } else { 14 })
        .u32(length as u32)
        .u32(records.len() as u32)
        .done();
    out.extend_from_slice(&recs.done());
    out.extend_from_slice(&tail);
    out
}

/// A complete, valid font: head, hhea, maxp (version 0.5), hmtx and a
/// cmap with a (3, 1) format 4 and a (3, 10) format 12 subtable.
/// Glyphs: 0 = .notdef, 1..=26 = 'A'..='Z', 27 = U+1F600.
pub fn sample_font() -> Vec<u8> {
    sfnt(&sample_tables())
}

pub fn sample_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let num_glyphs = 28u16;
    let long: Vec<(u16, i16)> = (0..20).map(|g| (500 + g as u16, g as i16)).collect();
    let lsbs: Vec<i16> = (20..num_glyphs).map(|g| -(g as i16)).collect();
    let cmap_table = cmap(&[
        (
            3,
            1,
            cmap4(&[seg(0x41, 0x5A, 1 - 0x41), seg(0xFFFF, 0xFFFF, 1)]),
        ),
        (
            3,
            10,
            cmap12_13(12, &[(0x41, 0x5A, 1), (0x1F600, 0x1F600, 27)]),
        ),
    ]);
    vec![
        (b"cmap", cmap_table),
        (b"head", head(HeadSpec::default())),
        (b"hhea", hhea(20)),
        (b"hmtx", hmtx(&long, &lsbs)),
        (b"maxp", maxp05(num_glyphs)),
    ]
}

/// `sample_tables()` plus an empty `glyf` and a short-format `loca` in
/// which every glyph is empty, so that FreeType's TrueType driver accepts
/// the font (it needs both tables).
pub fn sample_tables_with_glyf() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let mut tables = sample_tables();
    tables.push((b"glyf", Vec::new()));
    tables.push((b"loca", vec![0u8; 2 * 29]));
    tables.sort_by_key(|(t, _)| **t);
    tables
}

// ---------------------------------------------------------------- glyf

/// A point of a simple glyph, in absolute font units.
#[derive(Clone, Copy, Debug)]
pub enum Pt {
    On(i16, i16),
    Off(i16, i16),
    /// An off-curve point with the cubic flag 0x80.
    Cubic(i16, i16),
}

impl Pt {
    fn xy(self) -> (i16, i16) {
        match self {
            Pt::On(x, y) | Pt::Off(x, y) | Pt::Cubic(x, y) => (x, y),
        }
    }
}

pub const ON_CURVE: u8 = 0x01;
pub const X_SHORT: u8 = 0x02;
pub const Y_SHORT: u8 = 0x04;
pub const REPEAT: u8 = 0x08;
pub const X_SAME_OR_POSITIVE: u8 = 0x10;
pub const Y_SAME_OR_POSITIVE: u8 = 0x20;
pub const OVERLAP_SIMPLE: u8 = 0x40;
pub const CUBIC: u8 = 0x80;

fn bbox(points: &[(i16, i16)]) -> [i16; 4] {
    if points.is_empty() {
        return [0; 4];
    }
    let xs = points.iter().map(|p| p.0);
    let ys = points.iter().map(|p| p.1);
    [
        xs.clone().min().unwrap(),
        ys.clone().min().unwrap(),
        xs.max().unwrap(),
        ys.max().unwrap(),
    ]
}

/// A simple glyph. Flags use the short forms where they fit and the
/// repeat flag for runs of equal flags.
pub fn simple_glyph(contours: &[&[Pt]], overlap: bool, instructions: &[u8]) -> Vec<u8> {
    let points: Vec<Pt> = contours.iter().flat_map(|c| c.iter().copied()).collect();
    let xy: Vec<(i16, i16)> = points.iter().map(|p| p.xy()).collect();
    let b = bbox(&xy);
    let mut out = Buf::new()
        .i16(contours.len() as i16)
        .i16(b[0])
        .i16(b[1])
        .i16(b[2])
        .i16(b[3]);
    let mut end = 0i32;
    for c in contours {
        end += c.len() as i32;
        out = out.u16((end - 1) as u16);
    }
    out = out.u16(instructions.len() as u16).bytes(instructions);
    let (mut flags, mut xs, mut ys) = (Vec::new(), Vec::new(), Vec::new());
    let (mut px, mut py) = (0i32, 0i32);
    for (i, p) in points.iter().enumerate() {
        let (x, y) = p.xy();
        let (dx, dy) = (x as i32 - px, y as i32 - py);
        (px, py) = (x as i32, y as i32);
        let mut f = match p {
            Pt::On(..) => ON_CURVE,
            Pt::Off(..) => 0,
            Pt::Cubic(..) => CUBIC,
        };
        if i == 0 && overlap {
            f |= OVERLAP_SIMPLE;
        }
        if dx == 0 {
            f |= X_SAME_OR_POSITIVE;
        } else if dx.abs() <= 255 {
            f |= X_SHORT | if dx > 0 { X_SAME_OR_POSITIVE } else { 0 };
            xs.push(dx.unsigned_abs() as u8);
        } else {
            xs.extend_from_slice(&(dx as i16).to_be_bytes());
        }
        if dy == 0 {
            f |= Y_SAME_OR_POSITIVE;
        } else if dy.abs() <= 255 {
            f |= Y_SHORT | if dy > 0 { Y_SAME_OR_POSITIVE } else { 0 };
            ys.push(dy.unsigned_abs() as u8);
        } else {
            ys.extend_from_slice(&(dy as i16).to_be_bytes());
        }
        flags.push(f);
    }
    // Run-length encode equal flags.
    let mut encoded = Vec::new();
    let mut i = 0;
    while i < flags.len() {
        let mut run = 1;
        while i + run < flags.len() && flags[i + run] == flags[i] && run < 256 {
            run += 1;
        }
        if run > 1 {
            encoded.push(flags[i] | REPEAT);
            encoded.push((run - 1) as u8);
        } else {
            encoded.push(flags[i]);
        }
        i += run;
    }
    out.bytes(&encoded).bytes(&xs).bytes(&ys).done()
}

pub const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
pub const ARGS_ARE_XY_VALUES: u16 = 0x0002;
pub const ROUND_XY_TO_GRID: u16 = 0x0004;
pub const WE_HAVE_A_SCALE: u16 = 0x0008;
pub const MORE_COMPONENTS: u16 = 0x0020;
pub const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
pub const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
pub const USE_MY_METRICS: u16 = 0x0200;
pub const OVERLAP_COMPOUND: u16 = 0x0400;
pub const SCALED_COMPONENT_OFFSET: u16 = 0x0800;
pub const GID_IS_24_BIT: u16 = 0x2000;

#[derive(Clone, Copy, Debug)]
pub enum Args {
    /// Offsets; words when they do not fit in i8.
    Xy(i16, i16),
    /// Point numbers; words when they do not fit in u8.
    Points(u16, u16),
}

/// The transform, as F2Dot14 raw values in stored order.
#[derive(Clone, Copy, Debug)]
pub enum Xf {
    None,
    Scale(i16),
    XyScale(i16, i16),
    /// xx, yx, xy, yy (the order in the font).
    TwoByTwo(i16, i16, i16, i16),
}

#[derive(Clone, Copy, Debug)]
pub struct Comp {
    pub glyph: u32,
    pub args: Args,
    pub xf: Xf,
    /// Extra flags, e.g. SCALED_COMPONENT_OFFSET or OVERLAP_COMPOUND.
    pub flags: u16,
}

pub fn comp(glyph: u32, args: Args) -> Comp {
    Comp {
        glyph,
        args,
        xf: Xf::None,
        flags: 0,
    }
}

/// A composite glyph. With `wide_ids`, glyph ids are written as uint24
/// with GID_IS_24_BIT.
pub fn composite_glyph(comps: &[Comp], b: [i16; 4], wide_ids: bool) -> Vec<u8> {
    let mut out = Buf::new().i16(-1).i16(b[0]).i16(b[1]).i16(b[2]).i16(b[3]);
    for (i, c) in comps.iter().enumerate() {
        let mut f = c.flags;
        if i + 1 < comps.len() {
            f |= MORE_COMPONENTS;
        }
        let words = match c.args {
            Args::Xy(a, b) => {
                f |= ARGS_ARE_XY_VALUES;
                !(i8::try_from(a).is_ok() && i8::try_from(b).is_ok())
            }
            Args::Points(a, b) => !(u8::try_from(a).is_ok() && u8::try_from(b).is_ok()),
        };
        if words {
            f |= ARG_1_AND_2_ARE_WORDS;
        }
        f |= match c.xf {
            Xf::None => 0,
            Xf::Scale(_) => WE_HAVE_A_SCALE,
            Xf::XyScale(..) => WE_HAVE_AN_X_AND_Y_SCALE,
            Xf::TwoByTwo(..) => WE_HAVE_A_TWO_BY_TWO,
        };
        if wide_ids {
            f |= GID_IS_24_BIT;
        }
        out = out.u16(f);
        out = if wide_ids {
            out.u24(c.glyph)
        } else {
            out.u16(c.glyph as u16)
        };
        out = match (c.args, words) {
            (Args::Xy(a, b), true) => out.i16(a).i16(b),
            (Args::Xy(a, b), false) => out.u8(a as i8 as u8).u8(b as i8 as u8),
            (Args::Points(a, b), true) => out.u16(a).u16(b),
            (Args::Points(a, b), false) => out.u8(a as u8).u8(b as u8),
        };
        out = match c.xf {
            Xf::None => out,
            Xf::Scale(s) => out.i16(s),
            Xf::XyScale(x, y) => out.i16(x).i16(y),
            Xf::TwoByTwo(a, b, c2, d) => out.i16(a).i16(b).i16(c2).i16(d),
        };
    }
    out.done()
}

/// glyf and loca tables for `glyphs`. Short loca pads each glyph to an
/// even length.
pub fn glyf_loca(glyphs: &[Vec<u8>], long: bool) -> (Vec<u8>, Vec<u8>) {
    let mut glyf = Vec::new();
    let mut loca = Buf::new();
    for g in glyphs {
        loca = if long {
            loca.u32(glyf.len() as u32)
        } else {
            loca.u16((glyf.len() / 2) as u16)
        };
        glyf.extend_from_slice(g);
        if !long && glyf.len() % 2 == 1 {
            glyf.push(0);
        }
    }
    loca = if long {
        loca.u32(glyf.len() as u32)
    } else {
        loca.u16((glyf.len() / 2) as u16)
    };
    (glyf, loca.done())
}

/// A font with the given glyphs: head (with the loca format), maxp,
/// hhea and hmtx (advance 1000, lsb = the glyph's xMin, so that FreeType
/// does not shift outlines), a small cmap, and glyf/loca, or GLYF/LOCA
/// with `extended`.
pub fn glyph_font(glyphs: &[Vec<u8>], long: bool, extended: bool) -> Vec<u8> {
    let n = glyphs.len() as u16;
    let (glyf, loca) = glyf_loca(glyphs, long);
    let metrics: Vec<(u16, i16)> = glyphs
        .iter()
        .map(|g| {
            (
                1000,
                if g.len() >= 4 {
                    i16::from_be_bytes([g[2], g[3]])
                } else {
                    0
                },
            )
        })
        .collect();
    let mut tables: Vec<(&'static [u8; 4], Vec<u8>)> = vec![
        (
            b"cmap",
            cmap(&[(
                3,
                1,
                cmap4(&[seg(0x41, 0x41, 1 - 0x41), seg(0xFFFF, 0xFFFF, 1)]),
            )]),
        ),
        (
            b"head",
            head(HeadSpec {
                index_to_loc_format: if long { 1 } else { 0 },
                ..Default::default()
            }),
        ),
        (b"hhea", hhea(n)),
        (b"hmtx", hmtx(&metrics, &[])),
        (b"maxp", maxp05(n)),
    ];
    if extended {
        tables.push((b"GLYF", glyf));
        tables.push((b"LOCA", loca));
    } else {
        tables.push((b"glyf", glyf));
        tables.push((b"loca", loca));
    }
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

// ------------------------------------------------- OS/2, name, post, VORG

/// The fields of an OS/2 table that tests vary; the rest have fixed,
/// distinct values (see `os2`).
#[derive(Clone)]
pub struct Os2Spec {
    pub version: u16,
    pub weight_class: u16,
    pub fs_type: u16,
    pub fs_selection: u16,
    pub first_char: u16,
    pub last_char: u16,
    pub typo: (i16, i16, i16),
    pub win: (u16, u16),
    pub x_height: i16,
    pub cap_height: i16,
}

impl Default for Os2Spec {
    fn default() -> Self {
        Os2Spec {
            version: 4,
            weight_class: 400,
            fs_type: 8,
            fs_selection: 0x0040,
            first_char: 0x20,
            last_char: 0xFFFF,
            typo: (750, -250, 100),
            win: (900, 300),
            x_height: 500,
            cap_height: 700,
        }
    }
}

/// An OS/2 table of the length its version needs (versions above 5 get
/// the version 5 fields).
pub fn os2(spec: Os2Spec) -> Vec<u8> {
    let mut b = Buf::new()
        .u16(spec.version)
        .i16(510) // xAvgCharWidth
        .u16(spec.weight_class)
        .u16(5) // usWidthClass
        .u16(spec.fs_type)
        .i16(650) // ySubscriptXSize
        .i16(600) // ySubscriptYSize
        .i16(10) // ySubscriptXOffset
        .i16(75) // ySubscriptYOffset
        .i16(651) // ySuperscriptXSize
        .i16(601) // ySuperscriptYSize
        .i16(11) // ySuperscriptXOffset
        .i16(350) // ySuperscriptYOffset
        .i16(50) // yStrikeoutSize
        .i16(260) // yStrikeoutPosition
        .i16(0x0801) // sFamilyClass
        .bytes(&[2, 11, 5, 3, 2, 2, 2, 2, 2, 4]) // panose
        .u32(0x8000_0003)
        .u32(0x4000_0000)
        .u32(0x0000_0010)
        .u32(0x0000_0001) // ulUnicodeRange1..4
        .tag(b"SOFL")
        .u16(spec.fs_selection)
        .u16(spec.first_char)
        .u16(spec.last_char)
        .i16(spec.typo.0)
        .i16(spec.typo.1)
        .i16(spec.typo.2)
        .u16(spec.win.0)
        .u16(spec.win.1);
    if spec.version >= 1 {
        b = b.u32(0x2000_0001).u32(0x8000_0000); // ulCodePageRange1..2
    }
    if spec.version >= 2 {
        b = b
            .i16(spec.x_height)
            .i16(spec.cap_height)
            .u16(0) // usDefaultChar
            .u16(0x20) // usBreakChar
            .u16(3); // usMaxContext
    }
    if spec.version >= 5 {
        b = b.u16(80).u16(720); // usLower/UpperOpticalPointSize
    }
    b.done()
}

/// A name record: `(platform, encoding, language, name id, string)`.
pub type NameRec<'a> = (u16, u16, u16, u16, &'a [u8]);

/// A name table of `format` 0 or 1 whose strings follow the records
/// (and, for format 1, the language tag records) in record order, then
/// the language tag strings.
pub fn name(format: u16, records: &[NameRec<'_>], lang_tags: &[&[u8]]) -> Vec<u8> {
    let header = 6
        + 12 * records.len()
        + if format == 1 {
            2 + 4 * lang_tags.len()
        } else {
            0
        };
    let mut b = Buf::new()
        .u16(format)
        .u16(records.len() as u16)
        .u16(header as u16);
    let mut pool = Vec::new();
    for (p, e, l, id, s) in records {
        b = b
            .u16(*p)
            .u16(*e)
            .u16(*l)
            .u16(*id)
            .u16(s.len() as u16)
            .u16(pool.len() as u16);
        pool.extend_from_slice(s);
    }
    if format == 1 {
        b = b.u16(lang_tags.len() as u16);
        for t in lang_tags {
            b = b.u16(t.len() as u16).u16(pool.len() as u16);
            pool.extend_from_slice(t);
        }
    }
    b.bytes(&pool).done()
}

/// UTF-16BE bytes of `s`.
pub fn utf16be(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// The 32-byte post header with distinct field values.
pub fn post_header(version: u32) -> Buf {
    Buf::new()
        .u32(version)
        .i32(-0x000C_8000) // italicAngle -12.5
        .i16(-75) // underlinePosition
        .i16(50) // underlineThickness
        .u32(1) // isFixedPitch
        .u32(2)
        .u32(3)
        .u32(4)
        .u32(5) // min/maxMemType42, min/maxMemType1
}

/// A post table of version 2.0 with the given name indices and Pascal
/// strings.
pub fn post2(indices: &[u16], names: &[&str]) -> Vec<u8> {
    let mut b = post_header(0x0002_0000).u16(indices.len() as u16);
    for i in indices {
        b = b.u16(*i);
    }
    for n in names {
        b = b.u8(n.len() as u8).bytes(n.as_bytes());
    }
    b.done()
}

/// A VORG table: version 1 (16-bit glyph ids) or 2 (24-bit glyph ids
/// and record count), with `(glyph, vertOriginY)` records as given.
pub fn vorg(major: u16, default_y: i16, records: &[(u32, i16)]) -> Vec<u8> {
    let mut b = Buf::new().u16(major).u16(0).i16(default_y);
    b = if major == 2 {
        b.u24(records.len() as u32)
    } else {
        b.u16(records.len() as u16)
    };
    for (g, y) in records {
        b = if major == 2 {
            b.u24(*g)
        } else {
            b.u16(*g as u16)
        };
        b = b.i16(*y);
    }
    b.done()
}

// CFF and CFF2 (ADR 0021).

/// A CFF INDEX of `items` with offset size `off_size` (16-bit count, or
/// 32-bit for CFF2 when `wide`).
pub fn cff_index(items: &[Vec<u8>], off_size: u8, wide: bool) -> Vec<u8> {
    let mut b = if wide {
        Buf::new().u32(items.len() as u32)
    } else {
        Buf::new().u16(items.len() as u16)
    };
    if items.is_empty() {
        return b.done();
    }
    b = b.u8(off_size);
    let mut off = 1u32;
    let put = |b: Buf, v: u32| {
        let bytes = v.to_be_bytes();
        b.bytes(&bytes[4 - off_size as usize..])
    };
    b = put(b, off);
    for it in items {
        off += it.len() as u32;
        b = put(b, off);
    }
    for it in items {
        b = b.bytes(it);
    }
    b.done()
}

/// A DICT integer in its shortest encoding.
pub fn d_int(v: i32) -> Vec<u8> {
    match v {
        -107..=107 => vec![(v + 139) as u8],
        108..=1131 => {
            let w = v - 108;
            vec![(w / 256 + 247) as u8, (w % 256) as u8]
        }
        -1131..=-108 => {
            let w = -v - 108;
            vec![(w / 256 + 251) as u8, (w % 256) as u8]
        }
        -32768..=32767 => {
            let b = (v as i16).to_be_bytes();
            vec![28, b[0], b[1]]
        }
        _ => {
            let b = v.to_be_bytes();
            vec![29, b[0], b[1], b[2], b[3]]
        }
    }
}

/// A DICT integer in the five-byte encoding (29).
pub fn d_int5(v: i32) -> Vec<u8> {
    let b = v.to_be_bytes();
    vec![29, b[0], b[1], b[2], b[3]]
}

/// A DICT real from its text (`0-9 . E E- -`), terminated.
pub fn d_real(s: &str) -> Vec<u8> {
    let mut nibbles = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let n = match c {
            '0'..='9' => c as u8 - b'0',
            '.' => 0xA,
            'E' => {
                if chars.peek() == Some(&'-') {
                    chars.next();
                    0xC
                } else {
                    0xB
                }
            }
            '-' => 0xE,
            _ => unreachable!("bad real {s}"),
        };
        nibbles.push(n);
    }
    nibbles.push(0xF);
    if nibbles.len() % 2 == 1 {
        nibbles.push(0xF);
    }
    let mut out = vec![30];
    for p in nibbles.chunks(2) {
        out.push(p[0] << 4 | p[1]);
    }
    out
}

/// A DICT operator (`0x0C00 | b` for `12 b`).
pub fn d_op(op: u16) -> Vec<u8> {
    if op >= 0x0C00 {
        vec![12, (op & 0xFF) as u8]
    } else {
        vec![op as u8]
    }
}

/// DICT data: each entry is operands then an operator.
pub fn dict(entries: &[(&[i32], u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (operands, op) in entries {
        for v in *operands {
            out.extend(d_int(*v));
        }
        out.extend(d_op(*op));
    }
    out
}

/// A charstring integer in its shortest encoding (28 for the 16-bit
/// form).
pub fn cs_int(v: i32) -> Vec<u8> {
    match v {
        -107..=107 => vec![(v + 139) as u8],
        108..=1131 => {
            let w = v - 108;
            vec![(w / 256 + 247) as u8, (w % 256) as u8]
        }
        -1131..=-108 => {
            let w = -v - 108;
            vec![(w / 256 + 251) as u8, (w % 256) as u8]
        }
        _ => {
            let b = (v as i16).to_be_bytes();
            vec![28, b[0], b[1]]
        }
    }
}

/// A charstring 16.16 number (255).
pub fn cs_fixed(v: i32) -> Vec<u8> {
    let b = v.to_be_bytes();
    vec![255, b[0], b[1], b[2], b[3]]
}

/// A charstring from integer operands and operators: `Cs::I(v)` pushes,
/// `Cs::Op(o)` runs, `Cs::F(v)` pushes a 16.16 number, `Cs::Raw(b)` is
/// copied.
#[derive(Clone, Debug)]
pub enum Cs {
    I(i32),
    F(i32),
    Op(u16),
    Raw(Vec<u8>),
}

pub fn charstring(items: &[Cs]) -> Vec<u8> {
    let mut out = Vec::new();
    for it in items {
        match it {
            Cs::I(v) => out.extend(cs_int(*v)),
            Cs::F(v) => out.extend(cs_fixed(*v)),
            Cs::Op(o) => out.extend(d_op(*o)),
            Cs::Raw(b) => out.extend(b),
        }
    }
    out
}

/// A CID-keyed part of a CFF table: ROS, the Font DICTs (each with a
/// Private DICT and its local subroutines) and the FDSelect bytes.
#[derive(Clone, Debug, Default)]
pub struct CidSpec {
    /// Registry and ordering SIDs and the supplement.
    pub ros: (i32, i32, i32),
    /// Extra operators of each Font DICT, its Private DICT and its local
    /// subroutines.
    pub fds: Vec<(Vec<u8>, Vec<u8>, Vec<Vec<u8>>)>,
    /// The FDSelect, format byte included.
    pub fd_select: Vec<u8>,
}

/// The parts of a CFF (version 1) table. `top` holds the Top DICT
/// operators other than charset, CharStrings, Private, FDArray and
/// FDSelect, which the builder adds with five-byte offsets.
#[derive(Clone, Debug, Default)]
pub struct Cff1Spec {
    pub names: Vec<Vec<u8>>,
    pub top: Vec<u8>,
    pub strings: Vec<Vec<u8>>,
    pub gsubrs: Vec<Vec<u8>>,
    /// The charset bytes (format byte included); `None` for the ISOAdobe
    /// charset (offset 0).
    pub charset: Option<Vec<u8>>,
    /// The encoding bytes; `None` for the Standard encoding.
    pub encoding: Option<Vec<u8>>,
    pub charstrings: Vec<Vec<u8>>,
    /// The Private DICT operators (Subrs is added); `None` for no
    /// Private DICT.
    pub private: Option<Vec<u8>>,
    pub lsubrs: Vec<Vec<u8>>,
    pub cid: Option<CidSpec>,
}

impl Cff1Spec {
    /// A font named "Test" with `charstrings` and an empty Private DICT.
    pub fn new(charstrings: Vec<Vec<u8>>) -> Cff1Spec {
        Cff1Spec {
            names: vec![b"Test".to_vec()],
            charstrings,
            private: Some(Vec::new()),
            ..Default::default()
        }
    }
}

fn private_with_subrs(private: &[u8], lsubrs: &[Vec<u8>]) -> Vec<u8> {
    let mut p = private.to_vec();
    if !lsubrs.is_empty() {
        let len = p.len() + 6;
        p.extend(d_int5(len as i32));
        p.push(19);
    }
    p
}

/// A CFF table built from `spec`.
pub fn cff1(spec: &Cff1Spec) -> Vec<u8> {
    // The Top DICT has a fixed size: the extra operators plus 5-byte
    // offsets.
    let extra_len = 6
        + 6
        + if spec.private.is_some() && spec.cid.is_none() {
            11
        } else {
            0
        }
        + if spec.encoding.is_some() { 6 } else { 0 }
        + if spec.cid.is_some() {
            7 + 7 + 3 * 5 + 2
        } else {
            0
        };
    let top_len = spec.top.len() + extra_len;
    let header = vec![1u8, 0, 4, 4];
    let names = cff_index(&spec.names, 1, false);
    let top_index_len = cff_index(&[vec![0u8; top_len]], 4, false).len();
    let strings = cff_index(&spec.strings, 4, false);
    let gsubrs = cff_index(&spec.gsubrs, 4, false);
    let mut at = header.len() + names.len() + top_index_len + strings.len() + gsubrs.len();
    let charset_at = at;
    let charset = spec.charset.clone().unwrap_or_default();
    at += charset.len();
    let encoding_at = at;
    let encoding = spec.encoding.clone().unwrap_or_default();
    at += encoding.len();
    let cs_at = at;
    let cs = cff_index(&spec.charstrings, 4, false);
    at += cs.len();
    let private_at = at;
    let private = spec
        .private
        .as_ref()
        .map(|p| private_with_subrs(p, &spec.lsubrs))
        .unwrap_or_default();
    at += private.len();
    let lsubrs = cff_index(&spec.lsubrs, 4, false);
    if !spec.lsubrs.is_empty() {
        at += lsubrs.len();
    }
    // CID part.
    let mut cid_bytes = Vec::new();
    let mut fd_array_at = 0;
    let mut fd_select_at = 0;
    if let Some(cid) = &spec.cid {
        // Font DICTs: extra + Private (2 x 5-byte + op).
        let privates: Vec<Vec<u8>> = cid
            .fds
            .iter()
            .map(|(_, p, l)| private_with_subrs(p, l))
            .collect();
        let fd_dict_lens: Vec<usize> = cid.fds.iter().map(|(e, _, _)| e.len() + 11).collect();
        let fd_index_len = cff_index(
            &fd_dict_lens
                .iter()
                .map(|l| vec![0u8; *l])
                .collect::<Vec<_>>(),
            4,
            false,
        )
        .len();
        fd_array_at = at;
        let mut p_at = at + fd_index_len;
        let mut dicts = Vec::new();
        let mut tail: Vec<u8> = Vec::new();
        for ((extra, _, l), p) in cid.fds.iter().zip(&privates) {
            let mut d = extra.clone();
            d.extend(d_int5(p.len() as i32));
            d.extend(d_int5(p_at as i32));
            d.push(18);
            dicts.push(d);
            tail.extend(p);
            p_at += p.len();
            if !l.is_empty() {
                let li = cff_index(l, 4, false);
                tail.extend(&li);
                p_at += li.len();
            }
        }
        cid_bytes.extend(cff_index(&dicts, 4, false));
        cid_bytes.extend(tail);
        fd_select_at = fd_array_at + cid_bytes.len();
        cid_bytes.extend(&cid.fd_select);
    }
    let mut top = Vec::new();
    if let Some(cid) = &spec.cid {
        for v in [cid.ros.0, cid.ros.1, cid.ros.2] {
            top.extend(d_int5(v));
        }
        top.extend([12, 30]);
    }
    top.extend(&spec.top);
    top.extend(d_int5(if spec.charset.is_some() {
        charset_at as i32
    } else {
        0
    }));
    top.push(15);
    if spec.encoding.is_some() {
        top.extend(d_int5(encoding_at as i32));
        top.push(16);
    }
    top.extend(d_int5(cs_at as i32));
    top.push(17);
    if spec.private.is_some() && spec.cid.is_none() {
        top.extend(d_int5(private.len() as i32));
        top.extend(d_int5(private_at as i32));
        top.push(18);
    }
    if spec.cid.is_some() {
        top.extend(d_int5(fd_array_at as i32));
        top.extend([12, 36]);
        top.extend(d_int5(fd_select_at as i32));
        top.extend([12, 37]);
    }
    assert_eq!(top.len(), top_len, "top dict size");
    let mut out = header;
    out.extend(names);
    out.extend(cff_index(&[top], 4, false));
    out.extend(strings);
    out.extend(gsubrs);
    out.extend(charset);
    out.extend(encoding);
    out.extend(cs);
    out.extend(private);
    if !spec.lsubrs.is_empty() {
        out.extend(lsubrs);
    }
    out.extend(cid_bytes);
    out
}

/// The parts of a CFF2 table. The Top DICT gets CharStrings, FDArray,
/// FDSelect (with more than one Font DICT) and vstore (when given).
#[derive(Clone, Debug, Default)]
pub struct Cff2Spec {
    pub top: Vec<u8>,
    pub gsubrs: Vec<Vec<u8>>,
    pub charstrings: Vec<Vec<u8>>,
    /// Each Font DICT's Private DICT and local subroutines.
    pub fds: Vec<(Vec<u8>, Vec<Vec<u8>>)>,
    /// The FDSelect, format byte included (with more than one Font DICT).
    pub fd_select: Vec<u8>,
    /// The Item Variation Store (without the 2-byte length).
    pub vstore: Option<Vec<u8>>,
}

/// A CFF2 table built from `spec`.
pub fn cff2(spec: &Cff2Spec) -> Vec<u8> {
    let extra =
        6 + 7 + if spec.fds.len() > 1 { 7 } else { 0 } + if spec.vstore.is_some() { 6 } else { 0 };
    let top_len = spec.top.len() + extra;
    let header_len = 5;
    let gsubrs = cff_index(&spec.gsubrs, 4, true);
    let mut at = header_len + top_len + gsubrs.len();
    let vstore_at = at;
    let vstore = spec.vstore.as_ref().map(|v| {
        let mut b = (v.len() as u16).to_be_bytes().to_vec();
        b.extend(v);
        b
    });
    at += vstore.as_ref().map_or(0, |v| v.len());
    let cs_at = at;
    let cs = cff_index(&spec.charstrings, 4, true);
    at += cs.len();
    let fd_select_at = at;
    if spec.fds.len() > 1 {
        at += spec.fd_select.len();
    }
    let fd_array_at = at;
    let privates: Vec<Vec<u8>> = spec
        .fds
        .iter()
        .map(|(p, l)| private_with_subrs(p, l))
        .collect();
    let fd_index_len = cff_index(&vec![vec![0u8; 11]; spec.fds.len()], 4, true).len();
    let mut p_at = at + fd_index_len;
    let mut dicts = Vec::new();
    let mut tail: Vec<u8> = Vec::new();
    for ((_, l), p) in spec.fds.iter().zip(&privates) {
        let mut d = d_int5(p.len() as i32);
        d.extend(d_int5(p_at as i32));
        d.push(18);
        dicts.push(d);
        tail.extend(p);
        p_at += p.len();
        if !l.is_empty() {
            let li = cff_index(l, 4, true);
            tail.extend(&li);
            p_at += li.len();
        }
    }
    let mut top = spec.top.clone();
    top.extend(d_int5(cs_at as i32));
    top.push(17);
    top.extend(d_int5(fd_array_at as i32));
    top.extend([12, 36]);
    if spec.fds.len() > 1 {
        top.extend(d_int5(fd_select_at as i32));
        top.extend([12, 37]);
    }
    if spec.vstore.is_some() {
        top.extend(d_int5(vstore_at as i32));
        top.push(24);
    }
    assert_eq!(top.len(), top_len);
    let mut out = vec![2u8, 0, 5];
    out.extend((top_len as u16).to_be_bytes());
    out.extend(top);
    out.extend(gsubrs);
    if let Some(v) = vstore {
        out.extend(v);
    }
    out.extend(cs);
    if spec.fds.len() > 1 {
        out.extend(&spec.fd_select);
    }
    out.extend(cff_index(&dicts, 4, true));
    out.extend(tail);
    out
}

/// An Item Variation Store: `axis_count` axes, the regions as
/// `(start, peak, end)` per axis, and ItemVariationData region indexes.
pub fn item_variation_store(
    axis_count: u16,
    regions: &[Vec<(i16, i16, i16)>],
    data: &[Vec<u16>],
) -> Vec<u8> {
    let header = 8 + 4 * data.len();
    let region_list = 4 + regions.len() * axis_count as usize * 6;
    let mut b = Buf::new().u16(1).u32(header as u32).u16(data.len() as u16);
    let mut at = header + region_list;
    for d in data {
        b = b.u32(at as u32);
        at += 6 + 2 * d.len();
    }
    b = b.u16(axis_count).u16(regions.len() as u16);
    for r in regions {
        for (s, p, e) in r {
            b = b.i16(*s).i16(*p).i16(*e);
        }
    }
    for d in data {
        b = b.u16(0).u16(0).u16(d.len() as u16);
        for i in d {
            b = b.u16(*i);
        }
    }
    b.done()
}

/// An OpenType font with CFF outlines: the sample tables (head, hhea,
/// hmtx, maxp with `num_glyphs`, cmap) plus `extra` tables, with sfnt
/// version `OTTO`.
pub fn otto_font(num_glyphs: u16, extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    let long: Vec<(u16, i16)> = (0..num_glyphs.max(1)).map(|g| (500 + g, 0)).collect();
    let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap(&[(3, 1, cmap4(&[seg(0xFFFF, 0xFFFF, 1)]))])),
        (b"head", head(HeadSpec::default())),
        (b"hhea", hhea(num_glyphs.max(1))),
        (b"hmtx", hmtx(&long, &[])),
        (b"maxp", maxp05(num_glyphs)),
    ];
    tables.extend(extra);
    tables.sort_by(|a, b| a.0.cmp(b.0));
    sfnt_with_version(u32::from_be_bytes(*b"OTTO"), &tables)
}

// ---------------------------------------------------------------- BASE

/// A BaseCoord table (ADR 0017).
#[derive(Clone, Debug)]
pub enum Coord {
    /// Format 1: a coordinate.
    F1(i16),
    /// Format 2: a coordinate, a 16-bit reference glyph and a point.
    F2(i16, u16, u16),
    /// Format 3: a coordinate and a Device table (its bytes).
    F3(i16, Option<Vec<u8>>),
    /// Format 4: a coordinate, a 24-bit reference glyph and a point.
    F4(i16, u32, u16),
    /// Bytes as given.
    Raw(Vec<u8>),
}

/// A MinMax table: minimum, maximum and FeatMinMax records.
#[derive(Clone, Debug, Default)]
pub struct MinMaxSpec {
    pub min: Option<Coord>,
    pub max: Option<Coord>,
    pub features: Vec<([u8; 4], Option<Coord>, Option<Coord>)>,
}

/// A BaseScript table: BaseValues (default index and coordinates), the
/// default MinMax and BaseLangSys records.
#[derive(Clone, Debug, Default)]
pub struct ScriptSpec {
    pub values: Option<(u16, Vec<Option<Coord>>)>,
    pub default_min_max: Option<MinMaxSpec>,
    pub lang_sys: Vec<([u8; 4], Option<MinMaxSpec>)>,
}

/// An Axis table: the BaseTagList and the BaseScriptList.
#[derive(Clone, Debug, Default)]
pub struct AxisSpec {
    pub tags: Option<Vec<[u8; 4]>>,
    pub scripts: Option<Vec<([u8; 4], Option<ScriptSpec>)>>,
}

/// `header` followed by the given children; the 16-bit offset of each
/// (from the start of `header`) is written at its position, 0 for
/// `None`.
fn with_children(mut header: Vec<u8>, children: Vec<(usize, Option<Vec<u8>>)>) -> Vec<u8> {
    let mut tail = Vec::new();
    for (pos, child) in children {
        let off = match child {
            Some(c) => {
                let off = header.len() + tail.len();
                tail.extend_from_slice(&c);
                off as u16
            }
            None => 0,
        };
        header[pos..pos + 2].copy_from_slice(&off.to_be_bytes());
    }
    header.extend_from_slice(&tail);
    header
}

/// A Device table with delta format 1 to 3 and the given packed words,
/// or a VariationIndex table (format 0x8000) with `(outer, inner)` as
/// `(start, end)`.
pub fn device(start: u16, end: u16, format: u16, words: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(start).u16(end).u16(format);
    for w in words {
        b = b.u16(*w);
    }
    b.done()
}

pub fn base_coord(c: &Coord) -> Vec<u8> {
    match c {
        Coord::F1(v) => Buf::new().u16(1).i16(*v).done(),
        Coord::F2(v, g, p) => Buf::new().u16(2).i16(*v).u16(*g).u16(*p).done(),
        Coord::F3(v, d) => with_children(
            Buf::new().u16(3).i16(*v).u16(0).done(),
            vec![(4, d.clone())],
        ),
        Coord::F4(v, g, p) => Buf::new().u16(4).i16(*v).u24(*g).u16(*p).done(),
        Coord::Raw(b) => b.clone(),
    }
}

pub fn min_max(m: &MinMaxSpec) -> Vec<u8> {
    let mut b = Buf::new().u16(0).u16(0).u16(m.features.len() as u16);
    for (t, _, _) in &m.features {
        b = b.tag(t).u16(0).u16(0);
    }
    let mut children = vec![
        (0, m.min.as_ref().map(base_coord)),
        (2, m.max.as_ref().map(base_coord)),
    ];
    for (i, (_, min, max)) in m.features.iter().enumerate() {
        children.push((6 + 8 * i + 4, min.as_ref().map(base_coord)));
        children.push((6 + 8 * i + 6, max.as_ref().map(base_coord)));
    }
    with_children(b.done(), children)
}

pub fn base_script(s: &ScriptSpec) -> Vec<u8> {
    let mut b = Buf::new().u16(0).u16(0).u16(s.lang_sys.len() as u16);
    for (t, _) in &s.lang_sys {
        b = b.tag(t).u16(0);
    }
    let values = s.values.as_ref().map(|(default, coords)| {
        let mut v = Buf::new().u16(*default).u16(coords.len() as u16);
        for _ in coords {
            v = v.u16(0);
        }
        let children = coords
            .iter()
            .enumerate()
            .map(|(i, c)| (4 + 2 * i, c.as_ref().map(base_coord)))
            .collect();
        with_children(v.done(), children)
    });
    let mut children = vec![(0, values), (2, s.default_min_max.as_ref().map(min_max))];
    for (i, (_, m)) in s.lang_sys.iter().enumerate() {
        children.push((6 + 6 * i + 4, m.as_ref().map(min_max)));
    }
    with_children(b.done(), children)
}

pub fn base_axis(a: &AxisSpec) -> Vec<u8> {
    let tags = a.tags.as_ref().map(|tags| {
        let mut b = Buf::new().u16(tags.len() as u16);
        for t in tags {
            b = b.tag(t);
        }
        b.done()
    });
    let scripts = a.scripts.as_ref().map(|scripts| {
        let mut b = Buf::new().u16(scripts.len() as u16);
        for (t, _) in scripts {
            b = b.tag(t).u16(0);
        }
        let children = scripts
            .iter()
            .enumerate()
            .map(|(i, (_, s))| (2 + 6 * i + 4, s.as_ref().map(base_script)))
            .collect();
        with_children(b.done(), children)
    });
    with_children(
        Buf::new().u16(0).u16(0).done(),
        vec![(0, tags), (2, scripts)],
    )
}

/// A BASE table, version 1.0.
pub fn base(h: Option<&AxisSpec>, v: Option<&AxisSpec>) -> Vec<u8> {
    with_children(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).done(),
        vec![(4, h.map(base_axis)), (6, v.map(base_axis))],
    )
}

/// A BASE table, version 1.1, with an ItemVariationStore (its bytes)
/// placed after the axes, or a null offset.
pub fn base11(h: Option<&AxisSpec>, v: Option<&AxisSpec>, store: Option<&[u8]>) -> Vec<u8> {
    let mut t = with_children(
        Buf::new().u32(0x0001_0001).u16(0).u16(0).u32(0).done(),
        vec![(4, h.map(base_axis)), (6, v.map(base_axis))],
    );
    if let Some(s) = store {
        let off = t.len() as u32;
        t[8..12].copy_from_slice(&off.to_be_bytes());
        t.extend_from_slice(s);
    }
    t
}

/// An axis with one script whose BaseValues has a format 1 coordinate
/// for each `(tag, value)`.
pub fn simple_axis(script: &[u8; 4], values: &[([u8; 4], i16)]) -> AxisSpec {
    AxisSpec {
        tags: Some(values.iter().map(|(t, _)| *t).collect()),
        scripts: Some(vec![(
            *script,
            Some(ScriptSpec {
                values: Some((0, values.iter().map(|(_, v)| Some(Coord::F1(*v))).collect())),
                ..Default::default()
            }),
        )]),
    }
}

// ---- OpenType layout tables (ADR 0020) ----

/// `head` followed by `kids` in order. For each kid, its offset from the
/// start of `head` is written, `width` bytes wide (2, 3 or 4), at byte
/// `at` of `head`. A kid with no bytes gets a null offset.
pub fn link(mut head: Vec<u8>, kids: Vec<(usize, usize, Vec<u8>)>) -> Vec<u8> {
    for (at, width, kid) in kids {
        let offset = if kid.is_empty() { 0 } else { head.len() as u32 };
        let bytes = offset.to_be_bytes();
        head[at..at + width].copy_from_slice(&bytes[4 - width..]);
        head.extend_from_slice(&kid);
    }
    head
}

/// `n` zero bytes, for offsets that `link` fills in.
pub fn zeros(b: Buf, n: usize) -> Buf {
    b.bytes(&vec![0; n])
}

pub fn coverage1(glyphs: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(1).u16(glyphs.len() as u16);
    for g in glyphs {
        b = b.u16(*g);
    }
    b.done()
}

/// Format 2 with `(start, end, startCoverageIndex)` records.
pub fn coverage2(ranges: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut b = Buf::new().u16(2).u16(ranges.len() as u16);
    for (s, e, i) in ranges {
        b = b.u16(*s).u16(*e).u16(*i);
    }
    b.done()
}

pub fn coverage3(glyphs: &[u32]) -> Vec<u8> {
    let mut b = Buf::new().u16(3).u24(glyphs.len() as u32);
    for g in glyphs {
        b = b.u24(*g);
    }
    b.done()
}

pub fn coverage4(ranges: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut b = Buf::new().u16(4).u24(ranges.len() as u32);
    for (s, e, i) in ranges {
        b = b.u24(*s).u24(*e).u24(*i);
    }
    b.done()
}

pub fn class_def1(start: u16, classes: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(1).u16(start).u16(classes.len() as u16);
    for c in classes {
        b = b.u16(*c);
    }
    b.done()
}

pub fn class_def2(ranges: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut b = Buf::new().u16(2).u16(ranges.len() as u16);
    for (s, e, c) in ranges {
        b = b.u16(*s).u16(*e).u16(*c);
    }
    b.done()
}

pub fn class_def3(start: u32, classes: &[u32]) -> Vec<u8> {
    let mut b = Buf::new().u16(3).u24(start).u24(classes.len() as u32);
    for c in classes {
        b = b.u24(*c);
    }
    b.done()
}

pub fn class_def4(ranges: &[(u32, u32, u16)]) -> Vec<u8> {
    let mut b = Buf::new().u16(4).u24(ranges.len() as u32);
    for (s, e, c) in ranges {
        b = b.u24(*s).u24(*e).u16(*c);
    }
    b.done()
}

/// A Lookup table with 16-bit offsets to `subtables`.
pub fn lookup(ty: u16, flag: u16, subtables: Vec<Vec<u8>>, mark_set: Option<u16>) -> Vec<u8> {
    let n = subtables.len();
    let mut b = zeros(Buf::new().u16(ty).u16(flag).u16(n as u16), 2 * n);
    if let Some(s) = mark_set {
        b = b.u16(s);
    }
    let kids = subtables
        .into_iter()
        .enumerate()
        .map(|(i, s)| (6 + 2 * i, 2, s))
        .collect();
    link(b.done(), kids)
}

/// An extension subtable (format 1) pointing to `subtable` of type `ty`.
pub fn extension(ty: u16, subtable: Vec<u8>) -> Vec<u8> {
    link(
        Buf::new().u16(1).u16(ty).u32(0).done(),
        vec![(4, 4, subtable)],
    )
}

pub fn lookup_list(lookups: Vec<Vec<u8>>, wide: bool) -> Vec<u8> {
    let w = if wide { 4 } else { 2 };
    let n = lookups.len();
    let b = zeros(Buf::new().u16(n as u16), w * n);
    let kids = lookups
        .into_iter()
        .enumerate()
        .map(|(i, l)| (2 + w * i, w, l))
        .collect();
    link(b.done(), kids)
}

/// A FeatureList of `(tag, lookup indices)`.
pub fn feature_list(features: &[(&[u8; 4], &[u16])]) -> Vec<u8> {
    let n = features.len();
    let mut b = Buf::new().u16(n as u16);
    for (t, _) in features {
        b = b.tag(t).u16(0);
    }
    let kids = features
        .iter()
        .enumerate()
        .map(|(i, (_, l))| {
            let mut f = Buf::new().u16(0).u16(l.len() as u16);
            for x in *l {
                f = f.u16(*x);
            }
            (2 + 6 * i + 4, 2, f.done())
        })
        .collect();
    link(b.done(), kids)
}

/// A LangSys table: required feature index and feature indices.
pub fn lang_sys(required: u16, features: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(0).u16(required).u16(features.len() as u16);
    for f in features {
        b = b.u16(*f);
    }
    b.done()
}

/// A Script table: the default LangSys (empty for none) and tagged ones.
pub fn script(default: Vec<u8>, langs: Vec<(&[u8; 4], Vec<u8>)>) -> Vec<u8> {
    let n = langs.len();
    let mut b = Buf::new().u16(0).u16(n as u16);
    for (t, _) in &langs {
        b = b.tag(t).u16(0);
    }
    let mut kids = vec![(0, 2, default)];
    kids.extend(
        langs
            .into_iter()
            .enumerate()
            .map(|(i, (_, l))| (4 + 6 * i + 4, 2, l)),
    );
    link(b.done(), kids)
}

pub fn script_list(scripts: Vec<(&[u8; 4], Vec<u8>)>) -> Vec<u8> {
    let n = scripts.len();
    let mut b = Buf::new().u16(n as u16);
    for (t, _) in &scripts {
        b = b.tag(t).u16(0);
    }
    let kids = scripts
        .into_iter()
        .enumerate()
        .map(|(i, (_, s))| (2 + 6 * i + 4, 2, s))
        .collect();
    link(b.done(), kids)
}

/// A GSUB or GPOS table of `version` (0x00010000, 0x00010001 with a null
/// FeatureVariations offset, or 0x00010002 with the lists behind the
/// 32-bit offsets and the 16-bit ones null).
pub fn gsubgpos(version: u32, scripts: Vec<u8>, features: Vec<u8>, lookups: Vec<u8>) -> Vec<u8> {
    let mut b = Buf::new().u32(version).u16(0).u16(0).u16(0);
    if version >= 0x0001_0001 {
        b = b.u32(0);
    }
    if version >= 0x0001_0002 {
        b = b.u32(0).u32(0).u32(0);
        return link(
            b.done(),
            vec![(14, 4, scripts), (18, 4, features), (22, 4, lookups)],
        );
    }
    link(
        b.done(),
        vec![(4, 2, scripts), (6, 2, features), (8, 2, lookups)],
    )
}

/// A GSUB or GPOS 1.0 table with one script (`DFLT`, default LangSys
/// using every feature), one feature (`test`) using every lookup, and
/// `lookups` (16-bit offsets).
pub fn simple_layout(lookups: Vec<Vec<u8>>) -> Vec<u8> {
    let n = lookups.len() as u16;
    let indices: Vec<u16> = (0..n).collect();
    gsubgpos(
        0x0001_0000,
        script_list(vec![(b"DFLT", script(lang_sys(0xFFFF, &[0]), vec![]))]),
        feature_list(&[(b"test", &indices)]),
        lookup_list(lookups, false),
    )
}
