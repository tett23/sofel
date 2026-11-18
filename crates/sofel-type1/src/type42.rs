//! The `/sfnts` bytes in a Type 42 PostScript font.

use crate::encoding::Encoding;
use crate::noalloc::type42::{Declared, SfntCheck, charstrings, encoding, sfnts};
use crate::type42_expert::EXPERT_NAMES;
use crate::type42_glyphlist::GLYPH_LIST;
use alloc::vec::Vec;

pub use crate::noalloc::type42::{Type42Error, validate};

/// An owned sfnt extracted from a Type 42 font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type42Source {
    sfnt: Vec<u8>,
    glyphs: Vec<(Vec<u8>, u32)>,
    encoded: [Option<u32>; 256],
}

impl Type42Source {
    /// Extract the `/sfnts` array, with an upper bound on its decoded length.
    ///
    /// Allocates the joined `/sfnts` data and the table of glyph names (ADR 0094).
    pub fn parse(data: &[u8], max_output: usize) -> Result<Self, Type42Error> {
        let mut sfnt = Vec::new();
        let found = sfnts(data, max_output, &mut |bytes| sfnt.extend_from_slice(bytes))?;
        let mut directory = SfntCheck::new(sfnt.len());
        sfnt.iter().for_each(|byte| directory.feed(*byte));
        if !directory.finish() {
            return Err(Type42Error(found.marker));
        }
        let mut glyphs: Vec<(Vec<u8>, u32)> = Vec::new();
        charstrings(data, &mut |name, index| glyphs.push((name.to_vec(), index)))?;
        let notdef = glyphs
            .iter()
            .position(|(name, _)| name == b".notdef")
            .ok_or(Type42Error(found.end))?;
        glyphs.swap(0, notdef);
        let at = found.end;
        let mut source = Self {
            sfnt,
            glyphs,
            encoded: [None; 256],
        };
        let declared = encoding(data, at)?;
        if let Declared::IsoLatin1 = declared {
            for code in 0..256usize {
                let glyph = source.unicode_glyph(u32::try_from(code).map_err(|_| Type42Error(at))?);
                *source.encoded.get_mut(code).ok_or(Type42Error(at))? = glyph;
            }
        } else {
            let encoding = match declared {
                Declared::Type1(text) => Some(Encoding::parse(text).map_err(|_| Type42Error(at))?),
                _ => None,
            };
            for code in 0..256usize {
                let name = match encoding.as_ref() {
                    Some(encoding) => {
                        encoding.name(u8::try_from(code).map_err(|_| Type42Error(at))?)
                    }
                    None => EXPERT_NAMES.get(code).copied().flatten(),
                };
                let glyph = name
                    .and_then(|name| {
                        source
                            .glyphs
                            .iter()
                            .position(|(glyph_name, _)| glyph_name == name)
                    })
                    .and_then(|index| u32::try_from(index).ok());
                *source.encoded.get_mut(code).ok_or(Type42Error(at))? = glyph;
            }
        }
        Ok(source)
    }

    /// Decoded sfnt bytes for the existing font reader.
    pub fn sfnt(&self) -> &[u8] {
        &self.sfnt
    }

    /// The number of Type 42 glyphs, which can differ from the embedded sfnt.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len()
    }

    /// The PostScript name at a Type 42 glyph index.
    pub fn glyph_name(&self, glyph: usize) -> Option<&[u8]> {
        self.glyphs.get(glyph).map(|(name, _)| name.as_slice())
    }

    /// The embedded TrueType glyph selected by a Type 42 glyph index.
    pub fn embedded_glyph(&self, glyph: usize) -> Option<u32> {
        self.glyphs.get(glyph).map(|(_, index)| *index)
    }

    /// The Type 42 glyph selected by the PostScript encoding.
    pub fn encoded_glyph(&self, code: u8) -> Option<u32> {
        self.encoded.get(usize::from(code)).copied().flatten()
    }

    /// The Type 42 glyph selected by its PostScript Unicode glyph name.
    pub fn unicode_glyph(&self, codepoint: u32) -> Option<u32> {
        let mut variant = None;
        for (index, (name, _)) in self.glyphs.iter().enumerate() {
            if glyph_unicode(name) != Some(codepoint) {
                continue;
            }
            let index = u32::try_from(index).ok()?;
            if !name.contains(&b'.') {
                return Some(index);
            }
            variant.get_or_insert(index);
        }
        if variant.is_some() {
            return variant;
        }
        let extra = match codepoint {
            0x0394 => b"Delta".as_slice(),
            0x03A9 => b"Omega".as_slice(),
            0x2215 => b"fraction".as_slice(),
            0x00AD => b"hyphen".as_slice(),
            0x02C9 => b"macron".as_slice(),
            0x03BC => b"mu".as_slice(),
            0x2219 => b"periodcentered".as_slice(),
            0x00A0 => b"space".as_slice(),
            0x021A => b"Tcommaaccent".as_slice(),
            0x021B => b"tcommaaccent".as_slice(),
            _ => return None,
        };
        self.glyphs
            .iter()
            .position(|(name, _)| name == extra)
            .and_then(|index| u32::try_from(index).ok())
    }
}

/// Single Unicode value represented by an Adobe or `uniXXXX` glyph name.
pub fn glyph_unicode(name: &[u8]) -> Option<u32> {
    let name = name.split(|byte| *byte == b'.').next()?;
    if let Some(hex) = name.strip_prefix(b"uni").filter(|hex| hex.len() == 4) {
        return hex_codepoint(hex);
    }
    if let Some(hex) = name
        .strip_prefix(b"u")
        .filter(|hex| (4..=6).contains(&hex.len()))
    {
        return hex_codepoint(hex);
    }
    GLYPH_LIST
        .binary_search_by(|(candidate, _)| candidate.cmp(&name))
        .ok()
        .and_then(|index| GLYPH_LIST.get(index).map(|(_, value)| *value))
}

fn hex_codepoint(hex: &[u8]) -> Option<u32> {
    let value = hex.iter().try_fold(0u32, |n, byte| {
        let nibble = match byte {
            b'0'..=b'9' => byte.checked_sub(b'0'),
            b'A'..=b'F' => byte.checked_sub(b'A').and_then(|n| n.checked_add(10)),
            _ => None,
        }?;
        n.checked_mul(16)?.checked_add(u32::from(nibble))
    })?;
    char::from_u32(value).map(|_| value)
}
