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
    let mut tail = Vec::new();
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
