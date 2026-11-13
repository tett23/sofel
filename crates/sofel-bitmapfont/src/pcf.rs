//! Bounds-checked PCF metrics, encoding and bitmap reader (sofel ADR 0065).
// Table sizes and counts are checked before indexing; Clippy cannot follow
// those relationships through the parsing stages.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
use alloc::vec::Vec;

use crate::Glyph;

/// A malformed or unsupported PCF face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcfError;

/// A single-strike PCF face with one-based glyph IDs.
#[derive(Clone, Debug)]
pub struct PcfFont {
    glyphs: Vec<Glyph>,
    mappings: Vec<(u32, u32)>,
    default_glyph: usize,
    x_ppem_26_6: u32,
    y_ppem_26_6: u32,
}

fn u16_at(data: &[u8], offset: usize, big: bool) -> Result<u16, PcfError> {
    let bytes: [u8; 2] = data
        .get(offset..offset.checked_add(2).ok_or(PcfError)?)
        .ok_or(PcfError)?
        .try_into()
        .map_err(|_| PcfError)?;
    Ok(if big {
        u16::from_be_bytes(bytes)
    } else {
        u16::from_le_bytes(bytes)
    })
}

fn u32_at(data: &[u8], offset: usize, big: bool) -> Result<u32, PcfError> {
    let bytes: [u8; 4] = data
        .get(offset..offset.checked_add(4).ok_or(PcfError)?)
        .ok_or(PcfError)?
        .try_into()
        .map_err(|_| PcfError)?;
    Ok(if big {
        u32::from_be_bytes(bytes)
    } else {
        u32::from_le_bytes(bytes)
    })
}

fn table(data: &[u8], kind: u32) -> Result<&[u8], PcfError> {
    if !PcfFont::is_pcf(data) {
        return Err(PcfError);
    }
    let count = usize::try_from(u32_at(data, 4, false)?).map_err(|_| PcfError)?;
    if count == 0 || count > 64 {
        return Err(PcfError);
    }
    for i in 0..count {
        let at = 8usize
            .checked_add(i.checked_mul(16).ok_or(PcfError)?)
            .ok_or(PcfError)?;
        if u32_at(data, at, false)? != kind {
            continue;
        }
        let size = usize::try_from(u32_at(data, at + 8, false)?).map_err(|_| PcfError)?;
        let offset = usize::try_from(u32_at(data, at + 12, false)?).map_err(|_| PcfError)?;
        return data
            .get(offset..offset.checked_add(size).ok_or(PcfError)?)
            .ok_or(PcfError);
    }
    Err(PcfError)
}

fn directory_error_offset(data: &[u8]) -> usize {
    let Ok(count) = u32_at(data, 4, false).and_then(|n| usize::try_from(n).map_err(|_| PcfError))
    else {
        return 4;
    };
    if count == 0 || count > 64 {
        return 4;
    }
    for index in 0..count {
        let Some(at) = index.checked_mul(16).and_then(|n| n.checked_add(8)) else {
            return 4;
        };
        if data.get(at..at.saturating_add(16)).is_none() {
            return at;
        }
        let Ok(kind) = u32_at(data, at, false) else {
            return at;
        };
        if !matches!(kind, 1 | 4 | 8 | 32) {
            continue;
        }
        let Ok(size) = u32_at(data, at.saturating_add(8), false) else {
            return at.saturating_add(8);
        };
        let Ok(offset) = u32_at(data, at.saturating_add(12), false) else {
            return at.saturating_add(12);
        };
        let Some(end) = usize::try_from(offset)
            .ok()
            .and_then(|start| start.checked_add(usize::try_from(size).ok()?))
        else {
            return at.saturating_add(12);
        };
        if end > data.len() {
            return at.saturating_add(12);
        }
    }
    0
}

fn property(data: &[u8], name: &[u8]) -> Result<Option<i32>, PcfError> {
    let props = table(data, 1)?;
    let format = u32_at(props, 0, false)?;
    if format & !0x3f != 0 {
        return Err(PcfError);
    }
    let big = format & 4 != 0;
    let count = usize::try_from(u32_at(props, 4, big)?).map_err(|_| PcfError)?;
    if count > 256 {
        return Err(PcfError);
    }
    let records_end = 8usize
        .checked_add(count.checked_mul(9).ok_or(PcfError)?)
        .ok_or(PcfError)?;
    let strings_len_at = records_end
        .checked_add((4 - count % 4) % 4)
        .ok_or(PcfError)?;
    let strings_len = usize::try_from(u32_at(props, strings_len_at, big)?).map_err(|_| PcfError)?;
    let strings_start = strings_len_at.checked_add(4).ok_or(PcfError)?;
    let strings = props
        .get(strings_start..strings_start.checked_add(strings_len).ok_or(PcfError)?)
        .ok_or(PcfError)?;
    for i in 0..count {
        let at = 8 + i * 9;
        let off = usize::try_from(u32_at(props, at, big)?).map_err(|_| PcfError)?;
        let candidate = strings.get(off..).ok_or(PcfError)?;
        if candidate.split(|b| *b == 0).next() == Some(name) {
            if *props.get(at + 4).ok_or(PcfError)? != 0 {
                return Ok(None);
            }
            return Ok(Some(u32_at(props, at + 5, big)?.cast_signed()));
        }
    }
    Ok(None)
}

impl PcfFont {
    /// Whether the PCF magic is present.
    pub fn is_pcf(data: &[u8]) -> bool {
        data.starts_with(b"\x01fcp")
    }

    /// Parse a PCF face and identify a damaged table directory field.
    /// Errors within a table use the file start when no directory field is damaged.
    pub fn parse_positioned(data: &[u8]) -> Result<Self, (PcfError, usize)> {
        Self::parse(data).map_err(|error| (error, directory_error_offset(data)))
    }

    /// Parse a PCF face with a single monochrome strike.
    pub fn parse(data: &[u8]) -> Result<Self, PcfError> {
        let metrics = table(data, 4)?;
        let mf = u32_at(metrics, 0, false)?;
        let big = mf & 4 != 0;
        let compressed = mf & 0xffffff00 == 0x100;
        if mf & 0xffffff00 != 0 && !compressed {
            return Err(PcfError);
        }
        let count = usize::try_from(if compressed {
            u32::from(u16_at(metrics, 4, big)?)
        } else {
            u32_at(metrics, 4, big)?
        })
        .map_err(|_| PcfError)?;
        if count == 0 || count > 65534 {
            return Err(PcfError);
        }
        let start = if compressed { 6usize } else { 8usize };
        let stride = if compressed { 5usize } else { 12usize };
        metrics
            .get(
                start
                    ..start
                        .checked_add(count.checked_mul(stride).ok_or(PcfError)?)
                        .ok_or(PcfError)?,
            )
            .ok_or(PcfError)?;
        let bitmaps = table(data, 8)?;
        let bf = u32_at(bitmaps, 0, false)?;
        if bf & !0x3f != 0 {
            return Err(PcfError);
        }
        let bitmap_big = bf & 4 != 0;
        if usize::try_from(u32_at(bitmaps, 4, bitmap_big)?).map_err(|_| PcfError)? != count {
            return Err(PcfError);
        }
        let offsets_start = 8usize;
        let sizes_start = offsets_start
            .checked_add(count.checked_mul(4).ok_or(PcfError)?)
            .ok_or(PcfError)?;
        let bytes_start = sizes_start.checked_add(16).ok_or(PcfError)?;
        let pad = 1usize << (bf & 3);
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            let at = start + i * stride;
            let values = if compressed {
                let v = metrics.get(at..at + 5).ok_or(PcfError)?;
                [
                    i16::from(v[0]) - 128,
                    i16::from(v[1]) - 128,
                    i16::from(v[2]) - 128,
                    i16::from(v[3]) - 128,
                    i16::from(v[4]) - 128,
                ]
            } else {
                [0, 2, 4, 6, 8]
                    .map(|j| u16_at(metrics, at + j, big).map(u16::cast_signed))
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .map_err(|_| PcfError)?
            };
            let width =
                u16::try_from(i32::from(values[1]) - i32::from(values[0])).map_err(|_| PcfError)?;
            let height =
                u16::try_from(i32::from(values[3]) + i32::from(values[4])).map_err(|_| PcfError)?;
            let row_bytes = usize::from(width).div_ceil(8).div_ceil(pad) * pad;
            let len = row_bytes.checked_mul(usize::from(height)).ok_or(PcfError)?;
            let offset = usize::try_from(u32_at(bitmaps, offsets_start + i * 4, bitmap_big)?)
                .map_err(|_| PcfError)?;
            let begin = bytes_start.checked_add(offset).ok_or(PcfError)?;
            let mut bitmap = bitmaps
                .get(begin..begin.checked_add(len).ok_or(PcfError)?)
                .ok_or(PcfError)?
                .to_vec();
            if bf & 8 == 0 {
                for byte in &mut bitmap {
                    *byte = byte.reverse_bits();
                }
            }
            if (bf & 4 != 0) != (bf & 8 != 0) {
                let unit = 1usize << ((bf >> 4) & 3);
                if unit > 4 || bitmap.len() % unit != 0 {
                    return Err(PcfError);
                }
                for chunk in bitmap.chunks_exact_mut(unit) {
                    chunk.reverse();
                }
            }
            let mut packed =
                Vec::with_capacity(usize::from(width).div_ceil(8) * usize::from(height));
            if row_bytes != 0 {
                for row in bitmap.chunks_exact(row_bytes) {
                    packed.extend_from_slice(
                        row.get(..usize::from(width).div_ceil(8)).ok_or(PcfError)?,
                    );
                }
            }
            glyphs.push(Glyph {
                encoding: None,
                advance: i32::from(values[2]),
                width,
                height,
                left: values[0],
                top: values[3],
                bitmap: packed,
            });
        }
        let enc = table(data, 32)?;
        let ef = u32_at(enc, 0, false)?;
        if ef & !0x3f != 0 {
            return Err(PcfError);
        }
        let ebig = ef & 4 != 0;
        let first_col = u16_at(enc, 4, ebig)?;
        let last_col = u16_at(enc, 6, ebig)?;
        let first_row = u16_at(enc, 8, ebig)?;
        let last_row = u16_at(enc, 10, ebig)?;
        let default = u16_at(enc, 12, ebig)?;
        if first_col > last_col || last_col > 255 || first_row > last_row || last_row > 255 {
            return Err(PcfError);
        }
        let cols = usize::from(last_col - first_col + 1);
        let rows = usize::from(last_row - first_row + 1);
        let n = cols.checked_mul(rows).ok_or(PcfError)?;
        enc.get(
            14..14usize
                .checked_add(n.checked_mul(2).ok_or(PcfError)?)
                .ok_or(PcfError)?,
        )
        .ok_or(PcfError)?;
        let mut mappings = Vec::new();
        for i in 0..n {
            let index = u16_at(enc, 14 + i * 2, ebig)?;
            if index == 0xffff {
                continue;
            }
            if usize::from(index) >= count {
                return Err(PcfError);
            }
            let code = ((u32::from(first_row) + u32::try_from(i / cols).map_err(|_| PcfError)?)
                << 8)
                | (u32::from(first_col) + u32::try_from(i % cols).map_err(|_| PcfError)?);
            mappings.push((code, u32::from(index) + 1));
        }
        let default_glyph = mappings
            .iter()
            .find(|(code, _)| *code == u32::from(default))
            .map(|(_, id)| *id as usize - 1)
            .unwrap_or(0);
        let pixel_size = property(data, b"PIXEL_SIZE")?
            .unwrap_or(0)
            .unsigned_abs()
            .min(32767);
        let point_size = property(data, b"POINT_SIZE")?.unwrap_or(0).unsigned_abs();
        let res_y = property(data, b"RESOLUTION_Y")?.unwrap_or(0).unsigned_abs();
        let res_x = property(data, b"RESOLUTION_X")?.unwrap_or(0).unsigned_abs();
        let y = if pixel_size != 0 {
            pixel_size.checked_mul(64).ok_or(PcfError)?
        } else {
            let size = point_size
                .checked_mul(64)
                .and_then(|v| v.checked_mul(7200))
                .map(|v| (v + 36135) / 72270)
                .ok_or(PcfError)?;
            if res_y == 0 {
                size
            } else {
                size.checked_mul(res_y)
                    .map(|v| (v + 36) / 72)
                    .ok_or(PcfError)?
            }
        };
        let x = if res_y != 0 && res_x != 0 {
            y.checked_mul(res_x)
                .map(|v| (v + res_y / 2) / res_y)
                .ok_or(PcfError)?
        } else {
            y
        };
        if x == 0 || y == 0 {
            return Err(PcfError);
        }
        Ok(Self {
            glyphs,
            mappings,
            default_glyph,
            x_ppem_26_6: x,
            y_ppem_26_6: y,
        })
    }

    /// Number of glyph IDs including the missing-glyph slot.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len() + 1
    }
    /// The fixed strike size in 26.6 pixels.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        (self.x_ppem_26_6, self.y_ppem_26_6)
    }
    /// Map a codepoint to a one-based glyph ID.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<u32> {
        self.mappings
            .iter()
            .find(|(code, _)| *code == codepoint)
            .map(|(_, id)| *id)
    }
    /// Return the glyph for an ID, with ID zero using the default character.
    pub fn glyph(&self, id: u32) -> Option<&Glyph> {
        self.glyphs.get(if id == 0 {
            self.default_glyph
        } else {
            usize::try_from(id - 1).ok()?
        })
    }
}
