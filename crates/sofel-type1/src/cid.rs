//! CID-keyed Type 1 font data and charstrings (sofel ADR 0065).

use alloc::vec::Vec;
use core::fmt;

use crate::cipher::decrypt_charstring;
use crate::metrics::{Metrics, parse_metrics};

/// Invalid CID-keyed Type 1 header, map, or data bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CidError {
    offset: usize,
}

impl CidError {
    const fn invalid() -> Self {
        Self { offset: 0 }
    }

    const fn at(offset: usize) -> Self {
        Self { offset }
    }

    /// Byte position in the original file where reading failed.
    pub const fn offset(self) -> usize {
        self.offset
    }
}

impl fmt::Display for CidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid CID-keyed Type 1 font")
    }
}

impl core::error::Error for CidError {}

/// One CID-keyed Type 1 face. Glyph indices are CIDs.
#[derive(Clone, Debug)]
pub struct CidFace {
    data: Vec<u8>,
    cid_count: usize,
    cidmap_offset: usize,
    fd_bytes: usize,
    gd_bytes: usize,
    dicts: Vec<FontDict>,
    units_per_em: u16,
    native_stem_blues_empty: bool,
}

#[derive(Clone, Debug)]
struct FontDict {
    len_iv: i32,
    subr_count: usize,
    subr_map_offset: usize,
    sd_bytes: usize,
    matrix: CidMatrix,
}

/// Font DICT transform, normalized to 16.16 font units as FreeType does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CidMatrix {
    /// Horizontal contribution from x.
    pub xx: i32,
    /// Vertical contribution from x.
    pub yx: i32,
    /// Horizontal contribution from y.
    pub xy: i32,
    /// Vertical contribution from y.
    pub yy: i32,
    /// Horizontal translation in font units.
    pub dx: i32,
    /// Vertical translation in font units.
    pub dy: i32,
}

impl CidFace {
    /// Parse the PostScript header and bounded binary or hex data section.
    pub fn parse(source: &[u8], max_output: usize) -> Result<Self, CidError> {
        if !source.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
            return Err(CidError::invalid());
        }
        let start = find_start_data(source).ok_or(CidError::invalid())?;
        let header = source.get(..start).ok_or(CidError::invalid())?;
        let native_stem_blues_empty = crate::face::blue_zones_empty(header);
        let mode = header.rsplit(|&b| b == b'\n').next().unwrap_or(header);
        let data_start = start
            .checked_add(b"StartData".len())
            .and_then(|value| value.checked_add(1))
            .ok_or(CidError::invalid())?;
        let raw = source.get(data_start..).ok_or(CidError::invalid())?;
        let data = if mode.windows(5).any(|w| w == b"(Hex)") {
            decode_hex(raw, max_output, data_start)?
        } else {
            if raw.len() > max_output {
                return Err(CidError::at(data_start.saturating_add(max_output)));
            }
            raw.to_vec()
        };
        if field_usize(header, b"CIDFontType")? != 0 {
            return Err(CidError::invalid());
        }
        let cid_count = field_usize(header, b"CIDCount")?;
        let cidmap_offset = field_usize(header, b"CIDMapOffset")?;
        let fd_bytes = field_usize(header, b"FDBytes")?;
        let gd_bytes = field_usize(header, b"GDBytes")?;
        let dict_count = field_usize(header, b"FDArray")?;
        if cid_count == 0 || dict_count == 0 || fd_bytes > 4 || !(1..=4).contains(&gd_bytes) {
            return Err(CidError::invalid());
        }
        let entry = fd_bytes.checked_add(gd_bytes).ok_or(CidError::invalid())?;
        let map_len = cid_count
            .checked_add(1)
            .and_then(|count| count.checked_mul(entry))
            .ok_or(CidError::invalid())?;
        let map_end = cidmap_offset
            .checked_add(map_len)
            .ok_or(CidError::invalid())?;
        if map_end > data.len() {
            return Err(CidError::at(source.len()));
        }
        let mut dicts = Vec::new();
        let mut units_per_em = 1000;
        let marker = b"%ADOBeginFontDict";
        let mut rest = header;
        while let Some(pos) = find_bytes(rest, marker) {
            rest = rest
                .get(pos.checked_add(marker.len()).ok_or(CidError::invalid())?..)
                .ok_or(CidError::invalid())?;
            let dict_base = header.len().saturating_sub(rest.len());
            let end = find_bytes(rest, marker).unwrap_or(rest.len());
            let dict = rest.get(..end).ok_or(CidError::invalid())?;
            let locate = |error: CidError| CidError::at(dict_base.saturating_add(error.offset()));
            let subr_count = optional_field_usize(dict, b"SubrCount")
                .map_err(locate)?
                .unwrap_or(0);
            let subr_map_offset = optional_field_usize(dict, b"SubrMapOffset")
                .map_err(locate)?
                .unwrap_or(0);
            let sd_bytes = optional_field_usize(dict, b"SDBytes")
                .map_err(locate)?
                .unwrap_or(0);
            if subr_count > 0 {
                let map_end = subr_count
                    .checked_add(1)
                    .and_then(|count| count.checked_mul(sd_bytes))
                    .and_then(|size| subr_map_offset.checked_add(size))
                    .ok_or(CidError::invalid())?;
                if !(1..=4).contains(&sd_bytes) {
                    return Err(CidError::invalid());
                }
                if map_end > data.len() {
                    return Err(CidError::at(source.len()));
                }
            }
            let (matrix, current_upem) = parse_matrix(dict).map_err(|_| {
                CidError::at(
                    dict_base.saturating_add(find_bytes(dict, b"/FontMatrix").unwrap_or(0)),
                )
            })?;
            dicts.push(FontDict {
                len_iv: optional_field_i32(dict, b"lenIV")
                    .map_err(locate)?
                    .unwrap_or(4),
                subr_count,
                subr_map_offset,
                sd_bytes,
                matrix,
            });
            if let Some(current_upem) = current_upem {
                units_per_em = current_upem;
            }
            rest = rest.get(end..).ok_or(CidError::invalid())?;
        }
        if dicts.len() != dict_count {
            return Err(CidError::invalid());
        }
        Ok(Self {
            data,
            cid_count,
            cidmap_offset,
            fd_bytes,
            gd_bytes,
            dicts,
            units_per_em,
            native_stem_blues_empty,
        })
    }

    /// Number of CIDs exposed as glyph indices.
    pub fn num_glyphs(&self) -> usize {
        self.cid_count
    }

    /// Units per em derived from the uniform Font DICT matrix.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Whether the native single-stem path can ignore PostScript blue zones.
    pub fn native_stem_blues_empty(&self) -> bool {
        self.native_stem_blues_empty
    }

    /// Font DICT transform selected by the CID's FDBytes field.
    pub fn matrix(&self, cid: usize) -> Option<CidMatrix> {
        if cid >= self.cid_count {
            return None;
        }
        let entry = self.fd_bytes.checked_add(self.gd_bytes)?;
        let map = self.cidmap_offset.checked_add(cid.checked_mul(entry)?)?;
        let fd = read_be(self.data.get(map..map.checked_add(self.fd_bytes)?)?);
        Some(self.dicts.get(fd)?.matrix)
    }

    /// Decrypted Type 1 charstring for a CID.
    pub fn glyph_program(&self, cid: usize) -> Option<Vec<u8>> {
        if cid >= self.cid_count {
            return None;
        }
        let entry = self.fd_bytes.checked_add(self.gd_bytes)?;
        let map = self.cidmap_offset.checked_add(cid.checked_mul(entry)?)?;
        let fd = read_be(self.data.get(map..map.checked_add(self.fd_bytes)?)?);
        let len_iv = self.dicts.get(fd)?.len_iv;
        let first = map.checked_add(self.fd_bytes)?;
        let off1 = read_be(self.data.get(first..first.checked_add(self.gd_bytes)?)?);
        let next = map.checked_add(entry)?.checked_add(self.fd_bytes)?;
        let off2 = read_be(self.data.get(next..next.checked_add(self.gd_bytes)?)?);
        if off1 > off2 {
            return None;
        }
        decrypt_charstring(self.data.get(off1..off2)?, len_iv)
    }

    /// Decrypted Font DICT subroutine selected by a CID and local index.
    pub fn subroutine_program(&self, cid: usize, index: usize) -> Option<Vec<u8>> {
        if cid >= self.cid_count {
            return None;
        }
        let entry = self.fd_bytes.checked_add(self.gd_bytes)?;
        let map = self.cidmap_offset.checked_add(cid.checked_mul(entry)?)?;
        let fd = read_be(self.data.get(map..map.checked_add(self.fd_bytes)?)?);
        let dict = self.dicts.get(fd)?;
        if index >= dict.subr_count {
            return None;
        }
        let first = dict
            .subr_map_offset
            .checked_add(index.checked_mul(dict.sd_bytes)?)?;
        let second = first.checked_add(dict.sd_bytes)?;
        let end = second.checked_add(dict.sd_bytes)?;
        let off1 = read_be(self.data.get(first..second)?);
        let off2 = read_be(self.data.get(second..end)?);
        if off1 > off2 {
            return None;
        }
        decrypt_charstring(self.data.get(off1..off2)?, dict.len_iv)
    }

    /// Unscaled Type 1 side bearings and advances for a CID.
    pub fn glyph_metrics(&self, cid: usize) -> Option<Metrics> {
        let mut metrics = parse_metrics(&self.glyph_program(cid)?).ok()?;
        let matrix = self.matrix(cid)?;
        metrics.advance_x = mul_fix(metrics.advance_x, matrix.xx).saturating_add(matrix.dx);
        metrics.advance_y = mul_fix(metrics.advance_y, matrix.yy).saturating_add(matrix.dy);
        Some(metrics)
    }
}

fn mul_fix(a: i32, b: i32) -> i32 {
    let product = i64::from(a).saturating_mul(i64::from(b));
    let rounded = product.saturating_add(0x8000).saturating_add(product >> 63) >> 16;
    i32::try_from(rounded).unwrap_or(if rounded < 0 { i32::MIN } else { i32::MAX })
}

fn find_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn find_start_data(source: &[u8]) -> Option<usize> {
    let mut at = 0usize;
    while let Some(&byte) = source.get(at) {
        if byte == b'%' {
            while source.get(at).is_some_and(|&value| value != b'\n') {
                at = at.checked_add(1)?;
            }
        } else if byte == b'(' {
            let mut depth = 1usize;
            at = at.checked_add(1)?;
            while depth != 0 {
                let &value = source.get(at)?;
                if value == b'\\' {
                    at = at.checked_add(2)?;
                    continue;
                }
                if value == b'(' {
                    depth = depth.checked_add(1)?;
                }
                if value == b')' {
                    depth = depth.checked_sub(1)?;
                }
                at = at.checked_add(1)?;
            }
        } else if byte == b'/' {
            at = at.checked_add(1)?;
            while source.get(at).is_some_and(|&value| word_byte(value)) {
                at = at.checked_add(1)?;
            }
        } else if word_byte(byte) {
            let start = at;
            while source.get(at).is_some_and(|&value| word_byte(value)) {
                at = at.checked_add(1)?;
            }
            if source.get(start..at) == Some(&b"StartData"[..]) {
                return Some(start);
            }
        } else {
            at = at.checked_add(1)?;
        }
    }
    None
}

fn word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn field_usize(data: &[u8], name: &[u8]) -> Result<usize, CidError> {
    let (value, at) = field_i32(data, name)?;
    usize::try_from(value).map_err(|_| CidError::at(at))
}

fn optional_field_usize(data: &[u8], name: &[u8]) -> Result<Option<usize>, CidError> {
    optional_field_i32(data, name)?
        .map(|value| usize::try_from(value).map_err(|_| CidError::invalid()))
        .transpose()
}

fn optional_field_i32(data: &[u8], name: &[u8]) -> Result<Option<i32>, CidError> {
    let mut key = Vec::with_capacity(name.len().saturating_add(1));
    key.push(b'/');
    key.extend_from_slice(name);
    if find_bytes(data, &key).is_none() {
        return Ok(None);
    }
    field_i32(data, name).map(|(value, _)| Some(value))
}

fn field_i32(data: &[u8], name: &[u8]) -> Result<(i32, usize), CidError> {
    let mut key = Vec::with_capacity(name.len().checked_add(1).ok_or(CidError::invalid())?);
    key.push(b'/');
    key.extend_from_slice(name);
    let at = data
        .windows(key.len())
        .position(|w| w == key)
        .ok_or(CidError::at(data.len()))?;
    let after_key = at.checked_add(key.len()).ok_or(CidError::at(at))?;
    let untrimmed = data.get(after_key..).ok_or(CidError::at(after_key))?;
    if !untrimmed.first().is_some_and(u8::is_ascii_whitespace) {
        return Err(CidError::at(after_key));
    }
    let rest = untrimmed.trim_ascii_start();
    let value_at = after_key.saturating_add(untrimmed.len().saturating_sub(rest.len()));
    let end = rest
        .iter()
        .position(|b| !b.is_ascii_digit() && *b != b'-')
        .unwrap_or(rest.len());
    let text = core::str::from_utf8(rest.get(..end).ok_or(CidError::at(value_at))?)
        .map_err(|_| CidError::at(value_at))?;
    text.parse::<i32>()
        .map(|value| (value, value_at))
        .map_err(|_| CidError::at(value_at))
}

fn read_be(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0usize, |value, &byte| (value << 8) | usize::from(byte))
}

fn decode_hex(raw: &[u8], max_output: usize, data_start: usize) -> Result<Vec<u8>, CidError> {
    let mut out = Vec::new();
    let mut high = None;
    for (index, &byte) in raw.iter().enumerate() {
        if byte == b'>' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let offset = data_start.saturating_add(index);
        let digit = u8::try_from((byte as char).to_digit(16).ok_or(CidError::at(offset))?)
            .map_err(|_| CidError::at(offset))?;
        if let Some(hi) = high.take() {
            if out.len() >= max_output {
                return Err(CidError::at(offset));
            }
            out.push((hi << 4) | digit);
        } else {
            high = Some(digit);
        }
    }
    if high.is_some() {
        return Err(CidError::at(data_start.saturating_add(raw.len())));
    }
    Ok(out)
}

fn parse_matrix(dict: &[u8]) -> Result<(CidMatrix, Option<u16>), CidError> {
    let Some(at) = find_bytes(dict, b"/FontMatrix") else {
        return Ok((
            CidMatrix {
                xx: 65536,
                yx: 0,
                xy: 0,
                yy: 65536,
                dx: 0,
                dy: 0,
            },
            None,
        ));
    };
    let after = at
        .checked_add(b"/FontMatrix".len())
        .ok_or(CidError::invalid())?;
    let data = dict
        .get(after..)
        .ok_or(CidError::invalid())?
        .trim_ascii_start();
    let data = data.strip_prefix(b"[").ok_or(CidError::invalid())?;
    let end = data
        .iter()
        .position(|&byte| byte == b']')
        .ok_or(CidError::invalid())?;
    let values: Vec<_> = data
        .get(..end)
        .ok_or(CidError::invalid())?
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    let [_a, _b, _c, _d, _e, _f] = values.as_slice() else {
        return Err(CidError::invalid());
    };
    let mut fixed = [0i32; 6];
    for (target, value) in fixed.iter_mut().zip(values) {
        let text = core::str::from_utf8(value).map_err(|_| CidError::invalid())?;
        let number = text.parse::<f64>().map_err(|_| CidError::invalid())?;
        let scaled = number * 1000.0 * 65536.0;
        if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
            return Err(CidError::invalid());
        }
        // The explicit range check above makes this rounded float conversion bounded.
        #[allow(clippy::cast_possible_truncation)]
        let rounded = (scaled + if scaled < 0.0 { -0.5 } else { 0.5 }) as i32;
        *target = rounded;
    }
    let scale = fixed[3].unsigned_abs();
    if scale == 0 {
        return Ok((
            CidMatrix {
                xx: 0,
                yx: 0,
                xy: 0,
                yy: 0,
                dx: 0,
                dy: 0,
            },
            None,
        ));
    }
    let upem = if scale == 65536 {
        None
    } else {
        Some(
            u16::try_from(
                (1000u64 << 16)
                    .saturating_add(u64::from(scale / 2))
                    .checked_div(u64::from(scale))
                    .ok_or(CidError::invalid())?,
            )
            .map_err(|_| CidError::invalid())?,
        )
    };
    let normalize = |value: i32| -> Result<i32, CidError> {
        let numerator = i64::from(value).saturating_mul(65536);
        i32::try_from(
            numerator
                .checked_div(i64::from(scale))
                .ok_or(CidError::invalid())?,
        )
        .map_err(|_| CidError::invalid())
    };
    let matrix = CidMatrix {
        xx: normalize(fixed[0])?,
        yx: normalize(fixed[1])?,
        xy: normalize(fixed[2])?,
        yy: normalize(fixed[3])?,
        dx: normalize(fixed[4])? >> 16,
        dy: normalize(fixed[5])? >> 16,
    };
    Ok((matrix, upem))
}
