//! BDF bitmap faces (sofel ADR 0065).
#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]
extern crate alloc;

/// PCF bitmap faces.
pub mod pcf;

/// Windows FNT and NE and PE32 FON bitmap faces.
pub mod winfnt;

use alloc::vec::Vec;
use core::fmt;

/// A malformed or unsupported BDF face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BdfError;

impl fmt::Display for BdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid BDF font")
    }
}

/// One BDF glyph and its packed monochrome bitmap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    /// Unicode encoding, absent for `ENCODING -1`.
    pub encoding: Option<u32>,
    /// Horizontal device advance in pixels.
    pub advance: i32,
    /// Bitmap width in pixels.
    pub width: u16,
    /// Bitmap height in pixels.
    pub height: u16,
    /// Bitmap left bearing.
    pub left: i16,
    /// Bitmap top bearing.
    pub top: i16,
    /// Packed rows, most significant sample first.
    pub bitmap: Vec<u8>,
}

/// A single-strike BDF face. Glyph zero uses `DEFAULT_CHAR` or the first glyph.
#[derive(Clone, Debug)]
pub struct BdfFont {
    x_ppem_26_6: u32,
    y_ppem_26_6: u32,
    bits_per_pixel: u8,
    glyphs: Vec<Glyph>,
    default_glyph: usize,
    encoded_count: usize,
}

impl BdfFont {
    /// True if the input begins with a BDF signature.
    pub fn is_bdf(data: &[u8]) -> bool {
        data.starts_with(b"STARTFONT ")
    }

    /// Parse a BDF 2.1 face. Malformed dimensions, rows and glyph counts fail.
    pub fn parse(data: &[u8]) -> Result<Self, BdfError> {
        Self::parse_with_offset(data).map_err(|_| BdfError)
    }

    /// Parse a BDF face, reporting the first byte of the failing line or EOF.
    pub fn parse_with_offset(data: &[u8]) -> Result<Self, usize> {
        let text = core::str::from_utf8(data).map_err(|error| error.valid_up_to())?;
        let mut next_offset = 0usize;
        let mut offset = 0usize;
        let mut remaining = text;
        let lines = core::iter::from_fn(|| {
            offset = next_offset;
            if remaining.is_empty() {
                return None;
            }
            let end = remaining.find(['\r', '\n']).unwrap_or(remaining.len());
            let (line, suffix) = remaining.split_at(end);
            let separator = if suffix.starts_with("\r\n") || suffix.starts_with("\n\r") {
                2
            } else {
                usize::from(!suffix.is_empty())
            };
            remaining = suffix.get(separator..)?;
            next_offset = text.len().saturating_sub(remaining.len());
            Some(line.trim_end())
        });
        Self::parse_lines(lines, data.len()).map_err(|_| offset)
    }

    fn parse_lines<'a>(
        mut lines: impl Iterator<Item = &'a str>,
        input_len: usize,
    ) -> Result<Self, BdfError> {
        let first = lines.next().ok_or(BdfError)?;
        if !first.starts_with("STARTFONT ") {
            return Err(BdfError);
        }
        let mut size = None;
        let mut font_name = false;
        let mut font_box = false;
        let mut declared = None;
        let mut glyphs = Vec::new();
        let mut default_char = None;
        let mut point_size = None;
        let mut pixel_size = 0;
        let mut resolution_x = None;
        let mut resolution_y = None;
        let mut ended = false;
        let mut in_properties = false;
        while let Some(line) = lines.next() {
            if line == "ENDFONT" {
                if in_properties {
                    return Err(BdfError);
                }
                ended = true;
                break;
            }
            if line.starts_with("FONT ") {
                font_name = true;
            } else if let Some(fields) = line.strip_prefix("FONTBOUNDINGBOX ") {
                let mut parts = fields.split_ascii_whitespace();
                let width = number(parts.next())?;
                let height = number(parts.next())?;
                let _left = number(parts.next())?;
                let _bottom = number(parts.next())?;
                if width < 0 || height < 0 || parts.next().is_some() {
                    return Err(BdfError);
                }
                font_box = true;
            } else if line.starts_with("STARTPROPERTIES ") {
                if in_properties {
                    return Err(BdfError);
                }
                in_properties = true;
            } else if line == "ENDPROPERTIES" {
                if !in_properties {
                    return Err(BdfError);
                }
                in_properties = false;
            } else if in_properties && let Some((key, value)) = line.split_once(' ') {
                match key {
                    "POINT_SIZE" => point_size = Some(number(Some(value.trim()))?.unsigned_abs()),
                    "PIXEL_SIZE" => {
                        pixel_size = number(Some(value.trim()))?.unsigned_abs().min(32767)
                    }
                    "RESOLUTION_X" => {
                        resolution_x = Some(number(Some(value.trim()))?.unsigned_abs())
                    }
                    "RESOLUTION_Y" => {
                        resolution_y = Some(number(Some(value.trim()))?.unsigned_abs())
                    }
                    "DEFAULT_CHAR" => {
                        default_char = u32::try_from(number(Some(value.trim()))?).ok()
                    }
                    _ => {}
                }
            } else if let Some(fields) = line.strip_prefix("SIZE ") {
                let mut parts = fields.split_ascii_whitespace();
                let points = number(parts.next())?;
                let x_res = number(parts.next())?;
                let y_res = number(parts.next())?;
                let requested_bits = parts.next().map_or(Ok(1), |field| number(Some(field)))?;
                if points <= 0
                    || x_res <= 0
                    || y_res <= 0
                    || requested_bits <= 0
                    || parts.next().is_some()
                {
                    return Err(BdfError);
                }
                let bits = if requested_bits > 4 {
                    8
                } else if requested_bits > 2 {
                    4
                } else if requested_bits > 1 {
                    2
                } else {
                    1
                };
                size = Some((
                    u32::try_from(points).map_err(|_| BdfError)?,
                    u32::try_from(x_res).map_err(|_| BdfError)?,
                    u32::try_from(y_res).map_err(|_| BdfError)?,
                    bits,
                ));
            } else if let Some(field) = line.strip_prefix("CHARS ") {
                let n = number(Some(field))?;
                declared = Some(usize::try_from(n).map_err(|_| BdfError)?);
            } else if let Some(field) = line.strip_prefix("DEFAULT_CHAR ") {
                default_char = u32::try_from(number(Some(field))?).ok();
            } else if line.starts_with("STARTCHAR ") {
                if in_properties || declared.is_none_or(|n| glyphs.len() >= n) {
                    return Err(BdfError);
                }
                let glyph = parse_glyph(&mut lines, input_len, size.ok_or(BdfError)?.3)?;
                // The BDF driver exposes a reserved empty slot for an unencoded glyph.
                glyphs.push(if glyph.encoding.is_some() {
                    glyph
                } else {
                    Glyph::default()
                });
            }
        }
        let (points, x_res, y_res, bits_per_pixel) = size.ok_or(BdfError)?;
        if !ended || !font_name || !font_box || declared != Some(glyphs.len()) || glyphs.is_empty()
        {
            return Err(BdfError);
        }
        glyphs.sort_by_key(|glyph| glyph.encoding.unwrap_or(u32::MAX));
        let encoded_count = glyphs.partition_point(|g| g.encoding.is_some());
        let default_glyph = default_char
            .and_then(|codepoint| {
                glyphs
                    .iter()
                    .rposition(|glyph| glyph.encoding.unwrap_or(0) == codepoint)
            })
            .unwrap_or(0);
        let mul_div = |a: u32, b: u32, divisor: u32| {
            let rounded = u64::from(a)
                .saturating_mul(u64::from(b))
                .saturating_add(u64::from(divisor / 2));
            u32::try_from(rounded.checked_div(u64::from(divisor)).ok_or(BdfError)?)
                .map_err(|_| BdfError)
        };
        let size = match point_size {
            Some(n) if n <= 0x504c2 => mul_div(n, 64 * 7200, 72270)?,
            None if points <= 32767 => points << 6,
            _ => 32767,
        };
        let x_res = resolution_x.unwrap_or(x_res).min(32767);
        let y_res = resolution_y.unwrap_or(y_res).min(32767);
        let y_ppem = match (pixel_size, y_res) {
            (0, 0) => size,
            (0, res) => mul_div(size, res, 72)?,
            (pixels, _) => pixels << 6,
        };
        let x_ppem = if x_res == 0 || y_res == 0 {
            y_ppem
        } else {
            mul_div(y_ppem, x_res, y_res)?
        };
        Ok(Self {
            x_ppem_26_6: x_ppem,
            y_ppem_26_6: y_ppem,
            bits_per_pixel,
            glyphs,
            default_glyph,
            encoded_count,
        })
    }

    /// Number of glyph IDs, including the missing-glyph slot zero.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len().saturating_add(1)
    }
    /// Return the BDF strike's horizontal and vertical ppem in 26.6 units.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        (self.x_ppem_26_6, self.y_ppem_26_6)
    }
    /// Bits in one packed bitmap sample (1, 2, 4 or 8).
    pub fn bits_per_pixel(&self) -> u8 {
        self.bits_per_pixel
    }
    /// Map a Unicode scalar to FreeType's one-based glyph ID.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<u32> {
        let mut low = 0;
        let mut high = self.encoded_count;
        let mut mid = high / 2;
        while low < high {
            let code = self.glyphs.get(mid)?.encoding?;
            match codepoint.cmp(&code) {
                core::cmp::Ordering::Equal => {
                    // FreeType's BDF cmap stores and returns a 16-bit glyph index.
                    let id = u16::try_from(mid & 0xffff).ok()?.wrapping_add(1);
                    return (id != 0).then_some(u32::from(id));
                }
                core::cmp::Ordering::Less => high = mid,
                core::cmp::Ordering::Greater => low = mid.checked_add(1)?,
            }
            // FreeType predicts the next index from the distance between character codes.
            let step = i64::from(codepoint).saturating_sub(i64::from(code));
            mid = isize::try_from(step)
                .ok()
                .and_then(|delta| mid.checked_add_signed(delta))
                .filter(|index| (low..high).contains(index))
                .unwrap_or_else(|| low.midpoint(high));
        }
        None
    }
    /// Return a glyph by FreeType glyph ID. ID zero uses the first glyph.
    pub fn glyph(&self, id: u32) -> Option<&Glyph> {
        let index = if id == 0 {
            self.default_glyph
        } else {
            usize::try_from(id.saturating_sub(1)).ok()?
        };
        self.glyphs.get(index)
    }
}

fn number(field: Option<&str>) -> Result<i32, BdfError> {
    field.ok_or(BdfError)?.parse().map_err(|_| BdfError)
}

fn parse_glyph<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    input_len: usize,
    bits: u8,
) -> Result<Glyph, BdfError> {
    let mut encoding = None;
    let mut advance = None;
    let mut box_data = None;
    while let Some(line) = lines.next() {
        if let Some(field) = line.strip_prefix("ENCODING ") {
            let mut fields = field.split_ascii_whitespace();
            let primary = number(fields.next())?;
            let value = if primary < 0 {
                fields
                    .next()
                    .map(|field| number(Some(field)))
                    .transpose()?
                    .unwrap_or(-1)
            } else {
                primary
            };
            encoding = Some(u32::try_from(value).ok().filter(|code| *code < 0x110000));
        } else if let Some(field) = line.strip_prefix("DWIDTH ") {
            advance = Some(number(field.split_ascii_whitespace().next())?);
        } else if let Some(field) = line.strip_prefix("BBX ") {
            let mut p = field.split_ascii_whitespace();
            let width = u16::try_from(number(p.next())?).map_err(|_| BdfError)?;
            let height = u16::try_from(number(p.next())?).map_err(|_| BdfError)?;
            let left = i16::try_from(number(p.next())?).map_err(|_| BdfError)?;
            let bottom = i16::try_from(number(p.next())?).map_err(|_| BdfError)?;
            let top = i16::try_from(
                i32::from(bottom)
                    .checked_add(i32::from(height))
                    .ok_or(BdfError)?,
            )
            .map_err(|_| BdfError)?;
            box_data = Some((width, height, left, top));
        } else if line == "BITMAP" {
            let (width, height, left, top) = box_data.ok_or(BdfError)?;
            let pitch = usize::from(width)
                .checked_mul(usize::from(bits))
                .and_then(|n| n.checked_add(7))
                .ok_or(BdfError)?
                / 8;
            let len = pitch.checked_mul(usize::from(height)).ok_or(BdfError)?;
            if len > input_len / 2 {
                return Err(BdfError);
            }
            let mut bitmap = Vec::new();
            bitmap.try_reserve_exact(len).map_err(|_| BdfError)?;
            for _ in 0..height {
                let row = lines.next().ok_or(BdfError)?;
                if row.len() != pitch.checked_mul(2).ok_or(BdfError)? {
                    return Err(BdfError);
                }
                for &[high, low] in row.as_bytes().as_chunks::<2>().0 {
                    let hi = hex(high).ok_or(BdfError)?;
                    let lo = hex(low).ok_or(BdfError)?;
                    bitmap.push((hi << 4) | lo);
                }
            }
            if lines.next() != Some("ENDCHAR") {
                return Err(BdfError);
            }
            return Ok(Glyph {
                encoding: encoding.ok_or(BdfError)?,
                advance: advance.ok_or(BdfError)?,
                width,
                height,
                left,
                top,
                bitmap,
            });
        }
    }
    Err(BdfError)
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(c.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(c.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}
