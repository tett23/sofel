//! Bounds-checked Windows FNT and NE and PE32 FON bitmap faces (sofel ADR 0065).
use alloc::vec::Vec;

use crate::Glyph;

/// An invalid or unsupported Windows bitmap face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinFontError;

/// A single strike from a raw FNT or an NE or PE32 FON resource.
#[derive(Clone, Debug)]
pub struct WinFont {
    glyphs: Vec<Glyph>,
    num_faces: u32,
    first_char: u8,
    last_char: u8,
    default_glyph: usize,
    x_ppem_26_6: u32,
    y_ppem_26_6: u32,
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, WinFontError> {
    let bytes: [u8; 2] = data
        .get(offset..offset.checked_add(2).ok_or(WinFontError)?)
        .ok_or(WinFontError)?
        .try_into()
        .map_err(|_| WinFontError)?;
    Ok(u16::from_le_bytes(bytes))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, WinFontError> {
    let bytes: [u8; 4] = data
        .get(offset..offset.checked_add(4).ok_or(WinFontError)?)
        .ok_or(WinFontError)?
        .try_into()
        .map_err(|_| WinFontError)?;
    Ok(u32::from_le_bytes(bytes))
}

fn byte(data: &[u8], offset: usize) -> Result<u8, WinFontError> {
    data.get(offset).copied().ok_or(WinFontError)
}

fn fon_resource<'a>(
    data: &'a [u8],
    face: Option<u32>,
    position: &mut usize,
) -> Result<(Option<&'a [u8]>, u32), WinFontError> {
    *position = 0x3c;
    let ne = usize::try_from(u32_at(data, 0x3c)?).map_err(|_| WinFontError)?;
    *position = ne;
    if data.get(ne..).is_some_and(|s| s.starts_with(b"PE\0\0")) {
        return pe_resource(ne, data, face, position);
    }
    if data.get(ne..ne.checked_add(2).ok_or(WinFontError)?) != Some(b"NE") {
        return Err(WinFontError);
    }
    *position = ne.saturating_add(36);
    let resource = ne
        .checked_add(usize::from(u16_at(
            data,
            ne.checked_add(36).ok_or(WinFontError)?,
        )?))
        .ok_or(WinFontError)?;
    *position = ne.saturating_add(38);
    let names = ne
        .checked_add(usize::from(u16_at(
            data,
            ne.checked_add(38).ok_or(WinFontError)?,
        )?))
        .ok_or(WinFontError)?;
    if resource >= names || names > data.len() {
        return Err(WinFontError);
    }
    *position = resource;
    let shift = u16_at(data, resource)?;
    if shift > 16 {
        return Err(WinFontError);
    }
    let mut at = resource.checked_add(2).ok_or(WinFontError)?;
    loop {
        *position = at;
        if at >= names {
            return Err(WinFontError);
        }
        let kind = u16_at(data, at)?;
        if kind == 0 {
            return Err(WinFontError);
        }
        *position = at.saturating_add(2);
        let count = u16_at(data, at.checked_add(2).ok_or(WinFontError)?)?;
        let entries = at.checked_add(8).ok_or(WinFontError)?;
        let end = entries
            .checked_add(usize::from(count).checked_mul(12).ok_or(WinFontError)?)
            .ok_or(WinFontError)?;
        if end > names {
            return Err(WinFontError);
        }
        if kind == 0x8008 {
            if count == 0 {
                return Err(WinFontError);
            }
            let Some(face) = face else {
                return Ok((None, u32::from(count)));
            };
            if face >= u32::from(count) {
                return Err(WinFontError);
            }
            let entry = entries
                .checked_add(
                    usize::try_from(face)
                        .map_err(|_| WinFontError)?
                        .checked_mul(12)
                        .ok_or(WinFontError)?,
                )
                .ok_or(WinFontError)?;
            *position = entry;
            let offset = usize::from(u16_at(data, entry)?)
                .checked_shl(u32::from(shift))
                .ok_or(WinFontError)?;
            *position = entry.saturating_add(2);
            let size = usize::from(u16_at(data, entry.checked_add(2).ok_or(WinFontError)?)?)
                .checked_shl(u32::from(shift))
                .ok_or(WinFontError)?;
            return Ok((
                Some(
                    data.get(offset..offset.checked_add(size).ok_or(WinFontError)?)
                        .ok_or(WinFontError)?,
                ),
                u32::from(count),
            ));
        }
        at = end;
    }
}

fn range(data: &[u8], offset: usize, len: usize) -> Result<&[u8], WinFontError> {
    data.get(offset..offset.checked_add(len).ok_or(WinFontError)?)
        .ok_or(WinFontError)
}

fn offset32(data: &[u8], at: usize) -> Result<usize, WinFontError> {
    usize::try_from(u32_at(data, at)?).map_err(|_| WinFontError)
}

fn directory(resources: &[u8], offset: usize) -> Result<&[[u8; 8]], WinFontError> {
    let header = range(resources, offset, 16)?;
    let count = u16_at(header, 12)?
        .checked_add(u16_at(header, 14)?)
        .ok_or(WinFontError)?;
    Ok(range(
        resources,
        offset.checked_add(16).ok_or(WinFontError)?,
        usize::from(count).saturating_mul(8),
    )?
    .as_chunks::<8>()
    .0)
}

fn subdirectory(entry: &[u8]) -> Result<usize, WinFontError> {
    let offset = u32_at(entry, 4)?;
    if offset & 0x80000000 == 0 {
        return Err(WinFontError);
    }
    usize::try_from(offset & 0x7fffffff).map_err(|_| WinFontError)
}

fn pe_resource<'a>(
    base: usize,
    file: &'a [u8],
    face: Option<u32>,
    position: &mut usize,
) -> Result<(Option<&'a [u8]>, u32), WinFontError> {
    let header = file.get(base..).ok_or(WinFontError)?;
    for (offset, expected) in [(4, 0x14c), (20, 224), (24, 0x10b)] {
        *position = base.saturating_add(offset);
        if u16_at(header, offset)? != expected {
            return Err(WinFontError);
        }
    }
    *position = base.saturating_add(116);
    if u32_at(header, 116)? < 3 {
        return Err(WinFontError);
    }
    *position = base.saturating_add(136);
    let rva = u32_at(header, 136)?;
    *position = base;
    let sections = range(
        header,
        248,
        usize::from(u16_at(header, 6)?).saturating_mul(40),
    )?;
    let section = sections
        .as_chunks::<40>()
        .0
        .iter()
        .find(|sec| u32_at(*sec, 12) == Ok(rva))
        .ok_or(WinFontError)?;
    let raw = range(file, offset32(section, 20)?, offset32(section, 16)?)?;
    let resources = range(raw, 0, offset32(header, 140)?)?;
    let mut selected = None;
    let mut faces = 0u32;
    // Cap traversed directory entries by the size of the resource table.
    // Repeated references must not amplify a small directory into billions of visits.
    let mut budget = resources.len() / 8;
    for ty in directory(resources, 0)? {
        if u32_at(ty, 0)? != 8 {
            continue;
        }
        for name in directory(resources, subdirectory(ty)?)? {
            budget = budget.checked_sub(1).ok_or(WinFontError)?;
            for lang in directory(resources, subdirectory(name)?)? {
                budget = budget.checked_sub(1).ok_or(WinFontError)?;
                if face == Some(faces) {
                    let file_offset = |part: &[u8]| {
                        (part.as_ptr() as usize).saturating_sub(file.as_ptr() as usize)
                    };
                    *position = file_offset(lang).saturating_add(4);
                    let leaf = range(resources, offset32(lang, 4)?, 16)?;
                    *position = file_offset(leaf);
                    let offset = u32_at(leaf, 0)?.checked_sub(rva).ok_or(WinFontError)?;
                    let tail = raw
                        .get(usize::try_from(offset).map_err(|_| WinFontError)?..)
                        .ok_or(WinFontError)?;
                    *position = position.saturating_add(4);
                    let bytes = range(tail, 0, offset32(leaf, 4)?)?;
                    selected = Some(bytes);
                }
                faces = faces.checked_add(1).ok_or(WinFontError)?;
            }
        }
    }
    if faces == 0 {
        return Err(WinFontError);
    }
    Ok((selected, faces))
}

fn round_64(value: u32) -> Result<u32, WinFontError> {
    value.checked_add(32).map(|v| v & !63).ok_or(WinFontError)
}

fn mul_div(value: u32, numerator: u32, denominator: u32) -> Result<u32, WinFontError> {
    if denominator == 0 {
        return Err(WinFontError);
    }
    let rounded = u64::from(value)
        .checked_mul(u64::from(numerator))
        .and_then(|v| v.checked_add(u64::from(denominator / 2)))
        .and_then(|v| v.checked_div(u64::from(denominator)))
        .ok_or(WinFontError)?;
    u32::try_from(rounded).map_err(|_| WinFontError)
}

impl WinFont {
    /// Whether the input has a raw FNT or MZ-wrapped FON signature.
    pub fn is_win_font(data: &[u8]) -> bool {
        data.starts_with(b"MZ") || data.starts_with(b"\x00\x02") || data.starts_with(b"\x00\x03")
    }

    /// Parse `face` of a raw FNT or an NE or PE32 FON file.
    pub fn parse(data: &[u8], face: u32) -> Result<Self, WinFontError> {
        Self::parse_with_offset(data, face).map_err(|_| WinFontError)
    }

    /// Read the face count independently of the validity of the first strike.
    pub fn face_count(file: &[u8]) -> Result<u32, WinFontError> {
        if file.starts_with(b"MZ") {
            fon_resource(file, None, &mut 0).map(|(_, count)| count)
        } else if Self::is_win_font(file) {
            Ok(1)
        } else {
            Err(WinFontError)
        }
    }

    /// Parse a face and report the file offset of an invalid FNT field.
    pub fn parse_with_offset(file: &[u8], face: u32) -> Result<Self, usize> {
        let (data, num_faces) = if file.starts_with(b"MZ") {
            let mut position = 0;
            let (data, count) = fon_resource(file, Some(face), &mut position)
                .map_err(|_| file.len().min(position))?;
            (data.ok_or(position.min(file.len()))?, count)
        } else if face == 0 {
            (file, 1)
        } else {
            return Err(0);
        };
        let base = (data.as_ptr() as usize).saturating_sub(file.as_ptr() as usize);
        let mut at = 0;
        Self::parse_fnt(data, num_faces, &mut at)
            .map_err(|_| base.saturating_add(at.min(data.len())))
    }

    fn parse_fnt(data: &[u8], num_faces: u32, at: &mut usize) -> Result<Self, WinFontError> {
        let version = u16_at(data, 0)?;
        if !matches!(version, 0x200 | 0x300) {
            return Err(WinFontError);
        }
        if data.len() < 148 {
            *at = data.len();
            return Err(WinFontError);
        }
        *at = 2;
        let size = usize::try_from(u32_at(data, 2)?).map_err(|_| WinFontError)?;
        let header_size = if version == 0x300 { 148usize } else { 118usize };
        if size < header_size || size > data.len() {
            return Err(WinFontError);
        }
        let data = data.get(..size).ok_or(WinFontError)?;
        *at = 66;
        if u16_at(data, 66)? & 1 != 0 {
            return Err(WinFontError);
        }
        let point_size = u32::from(u16_at(data, 68)?);
        let y_res = match u16_at(data, 70)? {
            0 => 72,
            value => u32::from(value),
        };
        let x_res = match u16_at(data, 72)? {
            0 => 72,
            value => u32::from(value),
        };
        *at = 74;
        let ascent = i16::try_from(u16_at(data, 74)?).map_err(|_| WinFontError)?;
        *at = 88;
        let height = u16_at(data, 88)?;
        if height == 0 {
            return Err(WinFontError);
        }
        *at = 95;
        let first_char = byte(data, 95)?;
        let last_char = byte(data, 96)?;
        if first_char > last_char {
            return Err(WinFontError);
        }
        let count = usize::from(last_char.checked_sub(first_char).ok_or(WinFontError)?)
            .checked_add(1)
            .ok_or(WinFontError)?;
        *at = 97;
        let default_glyph = usize::from(byte(data, 97)?);
        if default_glyph >= count
            || usize::try_from(u32_at(data, 105)?).map_err(|_| WinFontError)? >= size
        {
            return Err(WinFontError);
        }
        let mut y_ppem = round_64(mul_div(
            point_size.checked_mul(64).ok_or(WinFontError)?,
            y_res,
            72,
        )?)?;
        let mut nominal = point_size.checked_mul(64).ok_or(WinFontError)?;
        if y_ppem > u32::from(height).checked_mul(64).ok_or(WinFontError)? {
            y_ppem = u32::from(height).checked_mul(64).ok_or(WinFontError)?;
            nominal = mul_div(y_ppem, 72, y_res)?;
        }
        let x_ppem = round_64(mul_div(nominal, x_res, 72)?)?;
        let entry_len = if version == 0x300 { 6usize } else { 4usize };
        *at = header_size;
        let entries_end = header_size
            .checked_add(count.checked_mul(entry_len).ok_or(WinFontError)?)
            .ok_or(WinFontError)?;
        if entries_end > size {
            return Err(WinFontError);
        }
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            let entry = header_size
                .checked_add(i.checked_mul(entry_len).ok_or(WinFontError)?)
                .ok_or(WinFontError)?;
            *at = entry;
            let width = u16_at(data, entry)?;
            *at = entry.saturating_add(2);
            let value_offset = entry.checked_add(2).ok_or(WinFontError)?;
            let offset = if version == 0x300 {
                usize::try_from(u32_at(data, value_offset)?).map_err(|_| WinFontError)?
            } else {
                usize::from(u16_at(data, value_offset)?)
            };
            let pitch = usize::from(width).div_ceil(8);
            if pitch == 0 {
                return Err(WinFontError);
            }
            let length = pitch.checked_mul(usize::from(height)).ok_or(WinFontError)?;
            let columns = data
                .get(offset..offset.checked_add(length).ok_or(WinFontError)?)
                .ok_or(WinFontError)?;
            let mut bitmap = Vec::with_capacity(length);
            for row in 0..usize::from(height) {
                for column in 0..pitch {
                    let index = column
                        .checked_mul(usize::from(height))
                        .and_then(|v| v.checked_add(row))
                        .ok_or(WinFontError)?;
                    bitmap.push(*columns.get(index).ok_or(WinFontError)?);
                }
            }
            glyphs.push(Glyph {
                encoding: Some(
                    u32::from(first_char)
                        .checked_add(u32::try_from(i).map_err(|_| WinFontError)?)
                        .ok_or(WinFontError)?,
                ),
                advance: i32::from(width),
                width,
                height,
                left: 0,
                top: ascent,
                bitmap,
            });
        }
        Ok(Self {
            glyphs,
            num_faces,
            first_char,
            last_char,
            default_glyph,
            x_ppem_26_6: x_ppem,
            y_ppem_26_6: y_ppem,
        })
    }

    /// Number of faces in the raw FNT or NE and PE32 FON container.
    pub fn num_faces(&self) -> u32 {
        self.num_faces
    }
    /// Number of glyph IDs, including the default-character slot zero.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len().saturating_add(1)
    }
    /// Strike dimensions in 26.6 pixels.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        (self.x_ppem_26_6, self.y_ppem_26_6)
    }
    /// Map the FNT character range to one-based glyph IDs.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<u32> {
        if codepoint < u32::from(self.first_char) || codepoint > u32::from(self.last_char) {
            return None;
        }
        codepoint
            .checked_sub(u32::from(self.first_char))?
            .checked_add(1)
    }
    /// Return a glyph; zero uses the default character.
    pub fn glyph(&self, id: u32) -> Option<&Glyph> {
        self.glyphs.get(if id == 0 {
            self.default_glyph
        } else {
            usize::try_from(id.checked_sub(1)?).ok()?
        })
    }
}
