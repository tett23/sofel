use core::fmt;

use crate::noalloc::{Event, GlyphHead, Problem};

/// A malformed or unsupported BDF face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BdfError;

impl fmt::Display for BdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid BDF font")
    }
}

impl core::error::Error for BdfError {}

/// The face values found by [`scan`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Summary {
    pub(crate) x_ppem_26_6: u32,
    pub(crate) y_ppem_26_6: u32,
    pub(crate) bits_per_pixel: u8,
    pub(crate) default_char: Option<u32>,
}

/// Read a BDF face, passing each glyph's pixel bytes to `byte` and then
/// the glyph to `glyph`. Fails with the first byte of the failing line,
/// or the end of the data.
pub(crate) fn scan(data: &[u8], event: &mut dyn FnMut(Event)) -> Result<Summary, usize> {
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
    scan_lines(lines, data.len(), event).map_err(|_| offset)
}

fn scan_lines<'a>(
    mut lines: impl Iterator<Item = &'a str>,
    input_len: usize,
    event: &mut dyn FnMut(Event),
) -> Result<Summary, BdfError> {
    let first = lines.next().ok_or(BdfError)?;
    if !first.starts_with("STARTFONT ") {
        return Err(BdfError);
    }
    let mut size = None;
    let mut font_name = false;
    let mut font_box = false;
    let mut declared = None;
    let mut glyphs = 0usize;
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
                "PIXEL_SIZE" => pixel_size = number(Some(value.trim()))?.unsigned_abs().min(32767),
                "RESOLUTION_X" => resolution_x = Some(number(Some(value.trim()))?.unsigned_abs()),
                "RESOLUTION_Y" => resolution_y = Some(number(Some(value.trim()))?.unsigned_abs()),
                "DEFAULT_CHAR" => default_char = u32::try_from(number(Some(value.trim()))?).ok(),
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
            if in_properties || declared.is_none_or(|n| glyphs >= n) {
                return Err(BdfError);
            }
            let head = parse_glyph(&mut lines, input_len, size.ok_or(BdfError)?.3, event)?;
            event(Event::Glyph(head));
            glyphs = glyphs.saturating_add(1);
        }
    }
    let (points, x_res, y_res, bits_per_pixel) = size.ok_or(BdfError)?;
    if !ended || !font_name || !font_box || declared != Some(glyphs) || glyphs == 0 {
        return Err(BdfError);
    }
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
    Ok(Summary {
        x_ppem_26_6: x_ppem,
        y_ppem_26_6: y_ppem,
        bits_per_pixel,
        default_char,
    })
}

/// Validate a BDF face: its header, properties, glyph count, and every
/// glyph's values and hexadecimal rows.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Problem)) {
    if let Err(offset) = scan(data, &mut |_| {}) {
        report(Problem {
            face: None,
            offset,
            reason: "invalid BDF font",
        });
    }
}

fn number(field: Option<&str>) -> Result<i32, BdfError> {
    field.ok_or(BdfError)?.parse().map_err(|_| BdfError)
}

fn parse_glyph<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    input_len: usize,
    bits: u8,
    event: &mut dyn FnMut(Event),
) -> Result<GlyphHead, BdfError> {
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
            for _ in 0..height {
                let row = lines.next().ok_or(BdfError)?;
                if row.len() != pitch.checked_mul(2).ok_or(BdfError)? {
                    return Err(BdfError);
                }
                for &[high, low] in row.as_bytes().as_chunks::<2>().0 {
                    let hi = hex(high).ok_or(BdfError)?;
                    let lo = hex(low).ok_or(BdfError)?;
                    event(Event::Pixel((hi << 4) | lo));
                }
            }
            if lines.next() != Some("ENDCHAR") {
                return Err(BdfError);
            }
            return Ok(GlyphHead {
                encoding: encoding.ok_or(BdfError)?,
                advance: advance.ok_or(BdfError)?,
                width,
                height,
                left,
                top,
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
