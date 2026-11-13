//! CID-keyed Type 1 font data and charstrings (sofel ADR 0065).

use alloc::vec::Vec;
use core::fmt;

use crate::cipher::decrypt_charstring;
use crate::metrics::{Metrics, parse_metrics};

/// Invalid CID-keyed Type 1 header, map, or data bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CidError;

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
    len_iv: Vec<i32>,
    units_per_em: u16,
}

impl CidFace {
    /// Parse the PostScript header and bounded binary or hex data section.
    pub fn parse(source: &[u8], max_output: usize) -> Result<Self, CidError> {
        if !source.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
            return Err(CidError);
        }
        let start = find_bytes(source, b"StartData").ok_or(CidError)?;
        let header = source.get(..start).ok_or(CidError)?;
        let mode = header.rsplit(|&b| b == b'\n').next().unwrap_or(header);
        let data_start = start
            .checked_add(b"StartData".len())
            .and_then(|value| value.checked_add(1))
            .ok_or(CidError)?;
        let raw = source.get(data_start..).ok_or(CidError)?;
        let data = if mode.windows(5).any(|w| w == b"(Hex)") {
            decode_hex(raw, max_output)?
        } else {
            if raw.len() > max_output {
                return Err(CidError);
            }
            raw.to_vec()
        };
        if field_usize(header, b"CIDFontType")? != 0 {
            return Err(CidError);
        }
        let cid_count = field_usize(header, b"CIDCount")?;
        let cidmap_offset = field_usize(header, b"CIDMapOffset")?;
        let fd_bytes = field_usize(header, b"FDBytes")?;
        let gd_bytes = field_usize(header, b"GDBytes")?;
        let dict_count = field_usize(header, b"FDArray")?;
        if cid_count == 0 || dict_count == 0 || fd_bytes > 4 || !(1..=4).contains(&gd_bytes) {
            return Err(CidError);
        }
        let entry = fd_bytes.checked_add(gd_bytes).ok_or(CidError)?;
        let map_len = cid_count
            .checked_add(1)
            .and_then(|count| count.checked_mul(entry))
            .ok_or(CidError)?;
        let map_end = cidmap_offset.checked_add(map_len).ok_or(CidError)?;
        if map_end > data.len() {
            return Err(CidError);
        }
        let mut len_iv = Vec::new();
        let mut units_per_em = None;
        let marker = b"%ADOBeginFontDict";
        let mut rest = header;
        while let Some(pos) = find_bytes(rest, marker) {
            rest = rest
                .get(pos.checked_add(marker.len()).ok_or(CidError)?..)
                .ok_or(CidError)?;
            let end = find_bytes(rest, marker).unwrap_or(rest.len());
            let dict = rest.get(..end).ok_or(CidError)?;
            len_iv.push(field_i32(dict, b"lenIV").unwrap_or(4));
            let current_upem = matrix_upem(dict)?;
            if units_per_em.is_some_and(|upem| upem != current_upem) {
                return Err(CidError);
            }
            units_per_em = Some(current_upem);
            rest = rest.get(end..).ok_or(CidError)?;
        }
        if len_iv.len() != dict_count {
            return Err(CidError);
        }
        Ok(Self {
            data,
            cid_count,
            cidmap_offset,
            fd_bytes,
            gd_bytes,
            len_iv,
            units_per_em: units_per_em.ok_or(CidError)?,
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

    /// Decrypted Type 1 charstring for a CID.
    pub fn glyph_program(&self, cid: usize) -> Option<Vec<u8>> {
        if cid >= self.cid_count {
            return None;
        }
        let entry = self.fd_bytes.checked_add(self.gd_bytes)?;
        let map = self.cidmap_offset.checked_add(cid.checked_mul(entry)?)?;
        let fd = read_be(self.data.get(map..map.checked_add(self.fd_bytes)?)?);
        let len_iv = *self.len_iv.get(fd)?;
        let first = map.checked_add(self.fd_bytes)?;
        let off1 = read_be(self.data.get(first..first.checked_add(self.gd_bytes)?)?);
        let next = map.checked_add(entry)?.checked_add(self.fd_bytes)?;
        let off2 = read_be(self.data.get(next..next.checked_add(self.gd_bytes)?)?);
        if off1 > off2 {
            return None;
        }
        decrypt_charstring(self.data.get(off1..off2)?, len_iv)
    }

    /// Unscaled Type 1 side bearings and advances for a CID.
    pub fn glyph_metrics(&self, cid: usize) -> Option<Metrics> {
        parse_metrics(&self.glyph_program(cid)?).ok()
    }
}

fn find_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn field_usize(data: &[u8], name: &[u8]) -> Result<usize, CidError> {
    usize::try_from(field_i32(data, name)?).map_err(|_| CidError)
}

fn field_i32(data: &[u8], name: &[u8]) -> Result<i32, CidError> {
    let mut key = Vec::with_capacity(name.len().checked_add(1).ok_or(CidError)?);
    key.push(b'/');
    key.extend_from_slice(name);
    let at = data
        .windows(key.len())
        .position(|w| w == key)
        .ok_or(CidError)?;
    let rest = data
        .get(at.checked_add(key.len()).ok_or(CidError)?..)
        .ok_or(CidError)?;
    if !rest.first().is_some_and(u8::is_ascii_whitespace) {
        return Err(CidError);
    }
    let rest = rest.trim_ascii_start();
    let end = rest
        .iter()
        .position(|b| !b.is_ascii_digit() && *b != b'-')
        .unwrap_or(rest.len());
    let text = core::str::from_utf8(rest.get(..end).ok_or(CidError)?).map_err(|_| CidError)?;
    text.parse::<i32>().map_err(|_| CidError)
}

fn read_be(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0usize, |value, &byte| (value << 8) | usize::from(byte))
}

fn decode_hex(raw: &[u8], max_output: usize) -> Result<Vec<u8>, CidError> {
    let mut out = Vec::new();
    let mut high = None;
    for &byte in raw {
        if byte == b'>' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let digit =
            u8::try_from((byte as char).to_digit(16).ok_or(CidError)?).map_err(|_| CidError)?;
        if let Some(hi) = high.take() {
            if out.len() >= max_output {
                return Err(CidError);
            }
            out.push((hi << 4) | digit);
        } else {
            high = Some(digit);
        }
    }
    if high.is_some() {
        return Err(CidError);
    }
    Ok(out)
}

fn matrix_upem(dict: &[u8]) -> Result<u16, CidError> {
    let Some(at) = find_bytes(dict, b"/FontMatrix") else {
        return Ok(1000);
    };
    let after = at.checked_add(b"/FontMatrix".len()).ok_or(CidError)?;
    let data = dict.get(after..).ok_or(CidError)?.trim_ascii_start();
    let data = data.strip_prefix(b"[").ok_or(CidError)?;
    let end = data.iter().position(|&byte| byte == b']').ok_or(CidError)?;
    let values: Vec<_> = data
        .get(..end)
        .ok_or(CidError)?
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    let [a, b, c, d, e, f] = values.as_slice() else {
        return Err(CidError);
    };
    if a != d || [b, c, e, f].iter().any(|value| **value != b"0") {
        return Err(CidError);
    }
    let text = core::str::from_utf8(a).map_err(|_| CidError)?;
    let (integer, fractional) = text.split_once('.').ok_or(CidError)?;
    if !integer.is_empty() && integer != "0" {
        return Err(CidError);
    }
    let numerator = fractional.parse::<u32>().map_err(|_| CidError)?;
    let digits = u32::try_from(fractional.len()).map_err(|_| CidError)?;
    let denominator = 10u32.checked_pow(digits).ok_or(CidError)?;
    if denominator.checked_rem(numerator) != Some(0) {
        return Err(CidError);
    }
    u16::try_from(denominator.checked_div(numerator).ok_or(CidError)?).map_err(|_| CidError)
}
