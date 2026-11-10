//! FreeType's embedded bitmap decoder (`tt_sbit_decoder_*` of
//! `src/sfnt/ttsbit.c`): the index lookup, the metrics, the byte- and
//! bit-aligned blits and the compound glyphs of `EBDT`, `CBDT` and
//! `bdat`, writing pixels into the caller's buffer.

use sofel_font::read::cbdt::GlyphMetrics;
use sofel_font::read::cblc::Cblc;

/// The pixel format of a decoded bitmap (FreeType's `FT_Pixel_Mode`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PixelMode {
    /// 1 bit per pixel, most significant bit first (`FT_PIXEL_MODE_MONO`).
    Mono,
    /// 2 bits per pixel (`FT_PIXEL_MODE_GRAY2`).
    Gray2,
    /// 4 bits per pixel (`FT_PIXEL_MODE_GRAY4`).
    Gray4,
    /// 8 bits per pixel (`FT_PIXEL_MODE_GRAY`).
    Gray,
    /// 32 bits per pixel, blue, green, red, alpha, premultiplied
    /// (`FT_PIXEL_MODE_BGRA`).
    Bgra,
}

/// Why a glyph bitmap could not be decoded, with FreeType's error.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BitmapError {
    /// No bitmap for the glyph in the strike (`Missing_Bitmap`).
    Missing,
    /// A component of a compound glyph is missing (`Invalid_Composite`).
    InvalidComposite,
    /// The data is broken (`Invalid_File_Format`, `Invalid_Table`,
    /// `Invalid_Argument`).
    Invalid,
    /// A PNG image, which is not decoded (`Unimplemented_Feature`).
    Png,
    /// The caller's buffer is shorter than this many bytes.
    BufferTooSmall(usize),
}

/// A decoded bitmap: its size, layout and metrics.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BitmapLayout {
    /// Pixels per row.
    pub width: u16,
    /// Rows.
    pub rows: u16,
    /// Bytes per row.
    pub pitch: u32,
    /// The pixel format.
    pub pixel_mode: PixelMode,
    /// The metrics (`bitmap_left` is `hori_bearing_x`, `bitmap_top`
    /// `hori_bearing_y`).
    pub metrics: crate::BitmapMetrics,
}

impl BitmapLayout {
    /// The bytes the pixels take: `pitch × rows`.
    pub fn len(&self) -> usize {
        usize::try_from(self.pitch)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::from(self.rows))
    }
    /// Whether the bitmap has no pixels.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

struct Decoder<'a, 'o> {
    eblc: &'a [u8],
    ebdt: &'a [u8],
    array: usize,
    count: u32,
    bit_depth: u8,
    metrics: Option<GlyphMetrics>,
    /// width, rows, pitch once allocated.
    bitmap: Option<(u32, u32, u32)>,
    out: Option<&'o mut [u8]>,
    metrics_only: bool,
}

fn u16_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at.checked_add(2)?)
        .and_then(|b| b.first_chunk::<2>())
        .map(|b| u32::from(u16::from_be_bytes(*b)))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at.checked_add(4)?)
        .and_then(|b| b.first_chunk::<4>())
        .map(|b| u32::from_be_bytes(*b))
}

fn to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

type Res = Result<(), BitmapError>;

impl Decoder<'_, '_> {
    /// `tt_sbit_decoder_load_metrics`.
    fn load_metrics(&mut self, p: &mut usize, limit: usize, big: bool) -> Res {
        let need = if big { 8 } else { 5 };
        let end = p.checked_add(need).ok_or(BitmapError::Invalid)?;
        if end > limit {
            return Err(BitmapError::Invalid);
        }
        let b = self.ebdt.get(*p..end).ok_or(BitmapError::Invalid)?;
        let m = if big {
            GlyphMetrics::big(b)
        } else {
            GlyphMetrics::small(b)
        };
        self.metrics = Some(m.ok_or(BitmapError::Invalid)?);
        *p = end;
        Ok(())
    }

    /// The same, reading from the location table (index formats 2 and 5).
    fn load_index_metrics(&mut self, at: usize) -> bool {
        match self
            .eblc
            .get(at..at.saturating_add(8))
            .and_then(GlyphMetrics::big)
        {
            Some(m) => {
                self.metrics = Some(m);
                true
            }
            None => false,
        }
    }

    /// `tt_sbit_decoder_alloc_bitmap`.
    fn alloc(&mut self) -> Res {
        let m = self.metrics.ok_or(BitmapError::Invalid)?;
        let (w, h) = (u32::from(m.width), u32::from(m.height));
        let pitch = match self.bit_depth {
            1 => w.saturating_add(7) >> 3,
            2 => w.saturating_add(3) >> 2,
            4 => w.saturating_add(1) >> 1,
            8 => w,
            32 => w.saturating_mul(4),
            _ => return Err(BitmapError::Invalid),
        };
        self.bitmap = Some((w, h, pitch));
        if self.metrics_only {
            return Ok(());
        }
        let size = to_usize(pitch).saturating_mul(to_usize(h));
        let out = self.out.as_deref_mut().ok_or(BitmapError::Invalid)?;
        let dst = out
            .get_mut(..size)
            .ok_or(BitmapError::BufferTooSmall(size))?;
        dst.fill(0);
        Ok(())
    }

    fn line_bits(&self) -> Option<(u32, u32, u32)> {
        let m = self.metrics?;
        let w = u32::from(m.width);
        Some((
            w,
            u32::from(m.height),
            w.saturating_mul(u32::from(self.bit_depth)),
        ))
    }

    fn bounds(&self, x: i32, y: i32, w: u32, h: u32) -> Option<(usize, usize, u32)> {
        let (bw, bh, pitch) = self.bitmap?;
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        if x.checked_add(w)? > bw || y.checked_add(h)? > bh {
            return None;
        }
        Some((
            to_usize(y)
                .saturating_mul(to_usize(pitch))
                .saturating_add(to_usize(x >> 3)),
            to_usize(pitch),
            x & 7,
        ))
    }

    fn or_byte(&mut self, at: usize, v: u32) {
        if let Some(b) = self.out.as_deref_mut().and_then(|o| o.get_mut(at)) {
            *b |= u8::try_from(v & 0xFF).unwrap_or(0);
        }
    }

    /// `tt_sbit_decoder_load_byte_aligned`.
    fn byte_aligned(&mut self, mut p: usize, limit: usize, x: i32, y: i32) -> Res {
        let (w, h, line_bits) = self.line_bits().ok_or(BitmapError::Invalid)?;
        let (mut line, pitch, x_pos) = self.bounds(x, y, w, h).ok_or(BitmapError::Invalid)?;
        let row_bytes = to_usize(line_bits.saturating_add(7) >> 3);
        if p.saturating_add(row_bytes.saturating_mul(to_usize(h))) > limit {
            return Err(BitmapError::Invalid);
        }
        let byte = |d: &[u8], p: usize| u32::from(d.get(p).copied().unwrap_or(0));
        for _ in 0..h {
            let mut pw = line;
            let mut wb = line_bits;
            if x_pos == 0 {
                while wb >= 8 {
                    let v = byte(self.ebdt, p);
                    p = p.saturating_add(1);
                    self.or_byte(pw, v);
                    pw = pw.saturating_add(1);
                    wb = wb.saturating_sub(8);
                }
                if wb > 0 {
                    let v = byte(self.ebdt, p) & (0xFF00u32 >> wb);
                    p = p.saturating_add(1);
                    self.or_byte(pw, v);
                }
            } else {
                let mut wval: u32 = 0;
                while wb >= 8 {
                    wval |= byte(self.ebdt, p);
                    p = p.saturating_add(1);
                    self.or_byte(pw, wval >> x_pos);
                    pw = pw.saturating_add(1);
                    wval <<= 8;
                    wb = wb.saturating_sub(8);
                }
                if wb > 0 {
                    wval |= byte(self.ebdt, p) & (0xFF00u32 >> wb);
                    p = p.saturating_add(1);
                }
                self.or_byte(pw, wval >> x_pos);
                if x_pos.saturating_add(wb) > 8 {
                    pw = pw.saturating_add(1);
                    wval <<= 8;
                    self.or_byte(pw, wval >> x_pos);
                }
            }
            line = line.saturating_add(pitch);
        }
        Ok(())
    }

    /// `tt_sbit_decoder_load_bit_aligned`.
    fn bit_aligned(&mut self, mut p: usize, limit: usize, x: i32, y: i32) -> Res {
        let (w, h, line_bits) = self.line_bits().ok_or(BitmapError::Invalid)?;
        let (mut line, pitch, x_pos) = self.bounds(x, y, w, h).ok_or(BitmapError::Invalid)?;
        let total = to_usize(line_bits.saturating_mul(h).saturating_add(7) >> 3);
        if p.saturating_add(total) > limit {
            return Err(BitmapError::Invalid);
        }
        if line_bits == 0 || h == 0 {
            return Ok(());
        }
        let byte = |d: &[u8], p: usize| u32::from(d.get(p).copied().unwrap_or(0));
        // The higher byte of `rval` is a buffer (FreeType's FT_UShort).
        let mut rval: u32 = 0;
        let mut nbits: u32 = 0;
        for row in 0..h {
            let mut pw = line;
            let mut wb = line_bits;
            if x_pos != 0 {
                wb = line_bits.min(8u32.saturating_sub(x_pos));
                if row == 0 {
                    rval = byte(self.ebdt, p);
                    p = p.saturating_add(1);
                    nbits = x_pos;
                } else if nbits < wb {
                    if p < limit {
                        rval |= byte(self.ebdt, p);
                        p = p.saturating_add(1);
                    }
                    nbits = nbits.saturating_add(8u32.saturating_sub(wb));
                } else {
                    rval >>= 8;
                    nbits = nbits.saturating_sub(wb);
                }
                let mask = !(0xFFu32 << wb) << (8u32.saturating_sub(wb).saturating_sub(x_pos));
                self.or_byte(pw, ((rval >> nbits) & 0xFF) & mask);
                pw = pw.saturating_add(1);
                rval = (rval << 8) & 0xFFFF;
                wb = line_bits.saturating_sub(wb);
            }
            while wb >= 8 {
                rval |= byte(self.ebdt, p);
                p = p.saturating_add(1);
                self.or_byte(pw, (rval >> nbits) & 0xFF);
                pw = pw.saturating_add(1);
                rval = (rval << 8) & 0xFFFF;
                wb = wb.saturating_sub(8);
            }
            if wb > 0 {
                if nbits < wb {
                    if p < limit {
                        rval |= byte(self.ebdt, p);
                        p = p.saturating_add(1);
                    }
                    self.or_byte(pw, ((rval >> nbits) & 0xFF) & (0xFF00u32 >> wb));
                    nbits = nbits.saturating_add(8u32.saturating_sub(wb));
                    rval = (rval << 8) & 0xFFFF;
                } else {
                    self.or_byte(pw, ((rval >> nbits) & 0xFF) & (0xFF00u32 >> wb));
                    nbits = nbits.saturating_sub(wb);
                }
            }
            line = line.saturating_add(pitch);
        }
        Ok(())
    }

    /// `tt_sbit_decoder_load_compound`.
    fn compound(&mut self, mut p: usize, limit: usize, x: i32, y: i32, recurse: u32) -> Res {
        let saved = self.metrics.ok_or(BitmapError::Invalid)?;
        if p.saturating_add(2) > limit {
            return Err(BitmapError::Invalid);
        }
        let n = u16_at(self.ebdt, p).ok_or(BitmapError::Invalid)?;
        p = p.saturating_add(2);
        if p.saturating_add(to_usize(n).saturating_mul(4)) > limit {
            return Err(BitmapError::Invalid);
        }
        let mut result = Ok(());
        for _ in 0..n {
            let g = u16_at(self.ebdt, p).ok_or(BitmapError::Invalid)?;
            let dx = i8::from_be_bytes([self.ebdt.get(p.saturating_add(2)).copied().unwrap_or(0)]);
            let dy = i8::from_be_bytes([self.ebdt.get(p.saturating_add(3)).copied().unwrap_or(0)]);
            p = p.saturating_add(4);
            let saved_only = self.metrics_only;
            self.metrics_only = false;
            result = self.load_image(
                g,
                x.saturating_add(i32::from(dx)),
                y.saturating_add(i32::from(dy)),
                recurse.saturating_add(1),
            );
            self.metrics_only = saved_only;
            if result.is_err() {
                break;
            }
        }
        let (bw, bh, _) = self.bitmap.unwrap_or_default();
        self.metrics = Some(GlyphMetrics {
            width: u8::try_from(bw & 0xFF).unwrap_or(0),
            height: u8::try_from(bh & 0xFF).unwrap_or(0),
            ..saved
        });
        result
    }

    /// `tt_sbit_decoder_load_bitmap`.
    fn load_bitmap(
        &mut self,
        format: u32,
        start: u32,
        size: u32,
        x: i32,
        y: i32,
        recurse: u32,
    ) -> Res {
        if size == 0
            || start
                .checked_add(size)
                .is_none_or(|e| to_usize(e) > self.ebdt.len())
        {
            return Err(BitmapError::Invalid);
        }
        let mut p = to_usize(start);
        let limit = p.saturating_add(to_usize(size));
        match format {
            1 | 2 | 8 | 17 => self.load_metrics(&mut p, limit, false)?,
            6 | 7 | 9 | 18 => self.load_metrics(&mut p, limit, true)?,
            _ => {}
        }
        enum Loader {
            Byte,
            Bit,
            Compound,
        }
        let loader = match format {
            1 | 6 => Loader::Byte,
            2 | 7 => {
                let m = self.metrics.ok_or(BitmapError::Invalid)?;
                let (w, h) = (u32::from(m.width), u32::from(m.height));
                let bit_size = w.saturating_mul(h).saturating_add(7) >> 3;
                let byte_size = h.saturating_mul(w.saturating_add(7) >> 3);
                if bit_size < byte_size && to_usize(byte_size) == limit.saturating_sub(p) {
                    Loader::Byte
                } else {
                    Loader::Bit
                }
            }
            5 => Loader::Bit,
            8 | 9 => {
                if format == 8 {
                    if p.saturating_add(1) > limit {
                        return Ok(());
                    }
                    p = p.saturating_add(1);
                }
                Loader::Compound
            }
            17..=19 => {
                if self.bitmap.is_none() {
                    self.alloc()?;
                }
                if self.metrics_only {
                    return Ok(());
                }
                return Err(BitmapError::Png);
            }
            _ => return Err(BitmapError::Invalid),
        };
        if self.bitmap.is_none() {
            self.alloc()?;
        }
        if self.metrics_only {
            return Ok(());
        }
        match loader {
            Loader::Byte => self.byte_aligned(p, limit, x, y),
            Loader::Bit => self.bit_aligned(p, limit, x, y),
            Loader::Compound => self.compound(p, limit, x, y, recurse),
        }
    }

    /// `tt_sbit_decoder_load_image`.
    fn load_image(&mut self, glyph: u32, x: i32, y: i32, recurse: u32) -> Res {
        if recurse > 100 {
            return Err(BitmapError::Invalid);
        }
        let missing = if recurse > 0 {
            BitmapError::InvalidComposite
        } else {
            BitmapError::Missing
        };
        let d = self.eblc;
        let mut found = None;
        for i in 0..to_usize(self.count) {
            let at = self.array.saturating_add(i.saturating_mul(8));
            let (s, e) = (
                u16_at(d, at).unwrap_or(0),
                u16_at(d, at.saturating_add(2)).unwrap_or(0),
            );
            if glyph >= s && glyph <= e {
                found = Some((s, u32_at(d, at.saturating_add(4)).unwrap_or(0)));
                break;
            }
        }
        let Some((start, offset)) = found else {
            return Err(missing);
        };
        if to_usize(offset) > d.len().saturating_sub(self.array) {
            return Err(BitmapError::Invalid);
        }
        let mut p = self.array.saturating_add(to_usize(offset));
        let limit = d.len();
        if p.saturating_add(8) > limit {
            return Err(missing);
        }
        let index_format = u16_at(d, p).unwrap_or(0);
        let image_format = u16_at(d, p.saturating_add(2)).unwrap_or(0);
        let image_offset = u32_at(d, p.saturating_add(4)).unwrap_or(0);
        p = p.saturating_add(8);
        let k = glyph.saturating_sub(start);
        let (image_start, image_end) = match index_format {
            1 => {
                p = p.saturating_add(to_usize(k).saturating_mul(4));
                if p.saturating_add(8) > limit {
                    return Err(missing);
                }
                let (s, e) = (
                    u32_at(d, p).unwrap_or(0),
                    u32_at(d, p.saturating_add(4)).unwrap_or(0),
                );
                if s == e {
                    return Err(missing);
                }
                (s, e)
            }
            2 => {
                if p.saturating_add(12) > limit {
                    return Err(missing);
                }
                let size = u32_at(d, p).unwrap_or(0);
                if !self.load_index_metrics(p.saturating_add(4)) {
                    return Err(missing);
                }
                let s = size.wrapping_mul(k);
                (s, s.wrapping_add(size))
            }
            3 => {
                p = p.saturating_add(to_usize(k).saturating_mul(2));
                if p.saturating_add(4) > limit {
                    return Err(missing);
                }
                let (s, e) = (
                    u16_at(d, p).unwrap_or(0),
                    u16_at(d, p.saturating_add(2)).unwrap_or(0),
                );
                if s == e {
                    return Err(missing);
                }
                (s, e)
            }
            4 => {
                if p.saturating_add(4) > limit {
                    return Err(missing);
                }
                let n = u32_at(d, p).unwrap_or(0);
                p = p.saturating_add(4);
                if p.saturating_add(4) > limit
                    || to_usize(n) > (limit.saturating_sub(p) >> 2).saturating_sub(1)
                {
                    return Err(missing);
                }
                let hit = (0..to_usize(n)).find(|&m| {
                    u16_at(d, p.saturating_add(m.saturating_mul(4))).unwrap_or(u32::MAX) == glyph
                });
                let Some(m) = hit else {
                    return Err(missing);
                };
                let q = p.saturating_add(m.saturating_mul(4));
                (
                    u16_at(d, q.saturating_add(2)).unwrap_or(0),
                    u16_at(d, q.saturating_add(6)).unwrap_or(0),
                )
            }
            5 | 19 => {
                if p.saturating_add(16) > limit {
                    return Err(missing);
                }
                let size = u32_at(d, p).unwrap_or(0);
                if !self.load_index_metrics(p.saturating_add(4)) {
                    return Err(missing);
                }
                let n = u32_at(d, p.saturating_add(12)).unwrap_or(0);
                p = p.saturating_add(16);
                if to_usize(n) > limit.saturating_sub(p) >> 1 {
                    return Err(missing);
                }
                let hit = (0..to_usize(n)).find(|&m| {
                    u16_at(d, p.saturating_add(m.saturating_mul(2))).unwrap_or(u32::MAX) == glyph
                });
                let Some(m) = hit else {
                    return Err(missing);
                };
                let s = size.wrapping_mul(u32::try_from(m).unwrap_or(u32::MAX));
                (s, s.wrapping_add(size))
            }
            _ => return Err(missing),
        };
        if image_start > image_end {
            return Err(missing);
        }
        self.load_bitmap(
            image_format,
            image_offset.wrapping_add(image_start),
            image_end.wrapping_sub(image_start),
            x,
            y,
            recurse,
        )
    }
}

/// FreeType's `tt_sbit_decoder_init` and `tt_sbit_decoder_load_image`
/// for `glyph` in BitmapSize `strike` of `eblc` with the data table
/// `ebdt`: with `out`, the pixels are written there; without, only the
/// layout is computed (`FT_LOAD_BITMAP_METRICS_ONLY`).
pub(crate) fn decode(
    eblc: &Cblc<'_>,
    ebdt: &[u8],
    strike: usize,
    glyph: u32,
    out: Option<&mut [u8]>,
) -> Result<BitmapLayout, BitmapError> {
    let d = eblc.data();
    let at = 8usize.saturating_add(strike.saturating_mul(48));
    // 8 + 48 * strike + 3 * 4 + 34 + 1 > table size.
    if at.saturating_add(47) > d.len() {
        return Err(BitmapError::Invalid);
    }
    let array = u32_at(d, at).unwrap_or(0);
    let count = u32_at(d, at.saturating_add(8)).unwrap_or(0);
    let bit_depth = d.get(at.saturating_add(46)).copied().unwrap_or(0);
    if to_usize(array) > d.len() || to_usize(count) > (d.len().saturating_sub(to_usize(array))) / 8
    {
        return Err(BitmapError::Invalid);
    }
    let metrics_only = out.is_none();
    let mut dec = Decoder {
        eblc: d,
        ebdt,
        array: to_usize(array),
        count,
        bit_depth,
        metrics: None,
        bitmap: None,
        out,
        metrics_only,
    };
    dec.load_image(glyph, 0, 0, 0)?;
    let m = dec.metrics.ok_or(BitmapError::Invalid)?;
    let (w, h, pitch) = dec
        .bitmap
        .unwrap_or((u32::from(m.width), u32::from(m.height), 0));
    let pixel_mode = match bit_depth {
        1 => PixelMode::Mono,
        2 => PixelMode::Gray2,
        4 => PixelMode::Gray4,
        8 => PixelMode::Gray,
        _ => PixelMode::Bgra,
    };
    Ok(BitmapLayout {
        width: u16::try_from(w).unwrap_or(0),
        rows: u16::try_from(h).unwrap_or(0),
        pitch,
        pixel_mode,
        metrics: crate::BitmapMetrics {
            width: u16::from(m.width),
            height: u16::from(m.height),
            hori_bearing_x: i16::from(m.hori_bearing_x),
            hori_bearing_y: i16::from(m.hori_bearing_y),
            hori_advance: u16::from(m.hori_advance),
            vert_bearing_x: i16::from(m.vert_bearing_x),
            vert_bearing_y: i16::from(m.vert_bearing_y),
            vert_advance: u16::from(m.vert_advance),
        },
    })
}
