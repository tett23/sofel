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
#[derive(Clone, Debug, PartialEq, Eq)]
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
                glyphs.push(parse_glyph(&mut lines, input_len, size.ok_or(BdfError)?.3)?);
            }
        }
        let (points, x_res, y_res, bits_per_pixel) = size.ok_or(BdfError)?;
        if !ended || !font_name || !font_box || declared != Some(glyphs.len()) || glyphs.is_empty()
        {
            return Err(BdfError);
        }
        glyphs.sort_by_key(|glyph| glyph.encoding.unwrap_or(u32::MAX));
        let default_glyph = default_char
            .and_then(|codepoint| {
                glyphs
                    .iter()
                    .position(|glyph| glyph.encoding == Some(codepoint))
            })
            .unwrap_or(0);
        let ppem = |resolution: u32| {
            points
                .checked_mul(resolution)?
                .checked_mul(64)?
                .checked_add(36)
                .map(|v| v / 72)
                .filter(|v| *v > 0)
        };
        Ok(Self {
            x_ppem_26_6: ppem(x_res).ok_or(BdfError)?,
            y_ppem_26_6: ppem(y_res).ok_or(BdfError)?,
            bits_per_pixel,
            glyphs,
            default_glyph,
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
        self.glyphs
            .iter()
            .position(|g| g.encoding == Some(codepoint))
            .and_then(|i| u32::try_from(i.saturating_add(1)).ok())
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
            let value = number(field.split_ascii_whitespace().next())?;
            encoding = Some(u32::try_from(value).ok());
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
