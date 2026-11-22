//! CID-keyed Type 1 font data and charstrings (sofel ADR 0065).

pub use crate::noalloc::cid::{CidError, CidMatrix, validate};
use crate::noalloc::cid::{FontDict, read_be, scan};
use alloc::vec::Vec;

use crate::cipher::decrypt_charstring;
use crate::metrics::{Metrics, parse_metrics};

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
    native_blue_zones: Vec<Option<crate::face::NativeBlueZones>>,
}

impl CidFace {
    /// Parse the PostScript header and bounded binary or hex data section.
    ///
    /// Allocates the binary data (a copy, or the decoded hexadecimal data) and the Font DICTs (ADR 0094).
    pub fn parse(source: &[u8], max_output: usize) -> Result<Self, CidError> {
        let mut data = Vec::new();
        let mut dicts = Vec::new();
        let found = scan(
            source,
            max_output,
            &mut |byte| data.push(byte),
            &mut |dict| dicts.push(dict),
        )?;
        if let Some(raw) = found.raw {
            data = raw.to_vec();
        }
        let native_blue_zones = {
            let marker = b"%ADOBeginFontDict";
            let mut rest = found.header;
            let mut zones = Vec::new();
            while let Some(at) = rest
                .windows(marker.len())
                .position(|window| window == marker)
            {
                let Some(after) = rest.get(at.saturating_add(marker.len())..) else {
                    break;
                };
                let end = after
                    .windows(marker.len())
                    .position(|window| window == marker)
                    .unwrap_or(after.len());
                let Some(dict) = after.get(..end) else {
                    break;
                };
                zones.push(crate::face::native_blue_zones(dict));
                let Some(next) = after.get(end..) else {
                    break;
                };
                rest = next;
            }
            zones
        };
        Ok(Self {
            data,
            cid_count: found.cid_count,
            cidmap_offset: found.cidmap_offset,
            fd_bytes: found.fd_bytes,
            gd_bytes: found.gd_bytes,
            dicts,
            units_per_em: found.units_per_em,
            native_stem_blues_empty: crate::face::blue_zones_empty(found.header),
            native_blue_zones,
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

    /// Blue-zone arrays and settings of the Font DICT selected by `cid`.
    pub fn native_blue_zones(&self, cid: usize) -> Option<&crate::face::NativeBlueZones> {
        if cid >= self.cid_count {
            return None;
        }
        let entry = self.fd_bytes.checked_add(self.gd_bytes)?;
        let map = self.cidmap_offset.checked_add(cid.checked_mul(entry)?)?;
        let fd = read_be(self.data.get(map..map.checked_add(self.fd_bytes)?)?);
        self.native_blue_zones.get(fd)?.as_ref()
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
    ///
    /// Allocates the decrypted charstring (ADR 0094).
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

    /// Whether a valid CID has no charstring bytes in CIDMap.
    pub fn glyph_is_empty(&self, cid: usize) -> Option<bool> {
        if cid >= self.cid_count {
            return None;
        }
        let entry = self.fd_bytes.checked_add(self.gd_bytes)?;
        let map = self.cidmap_offset.checked_add(cid.checked_mul(entry)?)?;
        let first = map.checked_add(self.fd_bytes)?;
        let off1 = read_be(self.data.get(first..first.checked_add(self.gd_bytes)?)?);
        let next = map.checked_add(entry)?.checked_add(self.fd_bytes)?;
        let off2 = read_be(self.data.get(next..next.checked_add(self.gd_bytes)?)?);
        (off1 <= off2 && off2 <= self.data.len()).then_some(off1 == off2)
    }

    /// Decrypted Font DICT subroutine selected by a CID and local index.
    ///
    /// Allocates the decrypted subroutine (ADR 0094).
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
    ///
    /// Allocates the decrypted charstring (ADR 0094); the operand stack is a fixed array (ADR 0098).
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
