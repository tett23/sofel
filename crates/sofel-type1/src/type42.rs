//! The `/sfnts` bytes in a Type 42 PostScript font.

use crate::encoding::Encoding;
use crate::type42_expert::EXPERT_NAMES;
use crate::type42_glyphlist::GLYPH_LIST;
use alloc::vec::Vec;
use core::fmt;

/// An invalid or truncated Type 42 wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Type42Error(usize);

impl Type42Error {
    /// Byte offset in the PostScript wrapper where parsing failed.
    pub fn offset(self) -> usize {
        self.0
    }
}

impl fmt::Display for Type42Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 42 font")
    }
}

impl core::error::Error for Type42Error {}

/// An owned sfnt extracted from a Type 42 font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type42Source {
    sfnt: Vec<u8>,
    glyphs: Vec<(Vec<u8>, u32)>,
    encoded: [Option<u32>; 256],
}

impl Type42Source {
    /// Extract the `/sfnts` array, with an upper bound on its decoded length.
    pub fn parse(data: &[u8], max_output: usize) -> Result<Self, Type42Error> {
        if !data.starts_with(b"%!PS-TrueTypeFont") {
            return Err(Type42Error(0));
        }
        let font_type = find_name(data, b"/FontType").ok_or(Type42Error(data.len()))?;
        let mut at = font_type
            .checked_add(b"/FontType".len())
            .ok_or(Type42Error(font_type))?;
        skip_space(data, &mut at);
        if token(data, &mut at) != Some(&b"42"[..]) {
            return Err(Type42Error(at));
        }
        let sfnts = find_name(data, b"/sfnts").ok_or(Type42Error(data.len()))?;
        at = sfnts.checked_add(b"/sfnts".len()).ok_or(Type42Error(at))?;
        skip_space(data, &mut at);
        if data.get(at) != Some(&b'[') {
            return Err(Type42Error(at));
        }
        at = at.checked_add(1).ok_or(Type42Error(at))?;
        let mut sfnt = Vec::new();
        let mut kind = None;
        loop {
            skip_space(data, &mut at);
            match data.get(at).copied().ok_or(Type42Error(at))? {
                b']' => break,
                b'<' => {
                    if matches!(kind, Some(b'B')) {
                        return Err(Type42Error(at));
                    }
                    kind = Some(b'H');
                    at = at.checked_add(1).ok_or(Type42Error(at))?;
                    let mut part = Vec::new();
                    let mut high = None;
                    loop {
                        let byte = *data.get(at).ok_or(Type42Error(at))?;
                        at = at.checked_add(1).ok_or(Type42Error(at))?;
                        if byte == b'>' {
                            break;
                        }
                        if byte.is_ascii_whitespace() {
                            continue;
                        }
                        let nibble = hex_nibble(byte).ok_or(Type42Error(at.saturating_sub(1)))?;
                        if let Some(upper) = high.take() {
                            if part.len() > max_output.saturating_sub(sfnt.len()) {
                                return Err(Type42Error(at));
                            }
                            part.push((upper << 4) | nibble);
                        } else {
                            high = Some(nibble);
                        }
                    }
                    if let Some(upper) = high {
                        if part.len() > max_output.saturating_sub(sfnt.len()) {
                            return Err(Type42Error(at));
                        }
                        part.push(upper << 4);
                    }
                    append_part(&mut sfnt, &part, max_output).map_err(|_| Type42Error(at))?;
                }
                b'0'..=b'9' => {
                    if matches!(kind, Some(b'H')) {
                        return Err(Type42Error(at));
                    }
                    kind = Some(b'B');
                    let length = decimal(token(data, &mut at).ok_or(Type42Error(at))?)
                        .map_err(|_| Type42Error(at))?;
                    skip_space(data, &mut at);
                    if token(data, &mut at) != Some(&b"RD"[..]) {
                        return Err(Type42Error(at));
                    }
                    let sep = *data.get(at).ok_or(Type42Error(at))?;
                    if !sep.is_ascii_whitespace() {
                        return Err(Type42Error(at));
                    }
                    at = at.checked_add(1).ok_or(Type42Error(at))?;
                    let end = at.checked_add(length).ok_or(Type42Error(at))?;
                    let part = data.get(at..end).ok_or(Type42Error(at))?;
                    append_part(&mut sfnt, part, max_output).map_err(|_| Type42Error(at))?;
                    at = end;
                }
                _ => return Err(Type42Error(at)),
            }
        }
        validate_sfnt(&sfnt).map_err(|_| Type42Error(sfnts))?;
        let glyphs = parse_charstrings(data)?;
        let mut source = Self {
            sfnt,
            glyphs,
            encoded: [None; 256],
        };
        let marker = find_name(data, b"/Encoding").ok_or(Type42Error(data.len()))?;
        let after = marker
            .checked_add(b"/Encoding".len())
            .ok_or(Type42Error(at))?;
        let declaration = data.get(after..).ok_or(Type42Error(at))?.trim_ascii_start();
        if declaration.starts_with(b"ISOLatin1Encoding") {
            for code in 0..256usize {
                let glyph = source.unicode_glyph(u32::try_from(code).map_err(|_| Type42Error(at))?);
                *source.encoded.get_mut(code).ok_or(Type42Error(at))? = glyph;
            }
        } else {
            let encoding = if declaration.starts_with(b"ExpertEncoding") {
                None
            } else {
                Some(
                    Encoding::parse(data.get(marker..).ok_or(Type42Error(at))?)
                        .map_err(|_| Type42Error(at))?,
                )
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

fn parse_charstrings(data: &[u8]) -> Result<Vec<(Vec<u8>, u32)>, Type42Error> {
    let marker = find_name(data, b"/CharStrings").ok_or(Type42Error(data.len()))?;
    let mut at = marker
        .checked_add(b"/CharStrings".len())
        .ok_or(Type42Error(marker))?;
    skip_space(data, &mut at);
    let dictionary = data.get(at..at.saturating_add(2)) == Some(b"<<");
    let count = if dictionary {
        at = at.checked_add(2).ok_or(Type42Error(at))?;
        None
    } else {
        let count = decimal(token(data, &mut at).ok_or(Type42Error(at))?)?;
        if count == 0 || count > data.len() / 4 {
            return Err(Type42Error(at));
        }
        while let Some(word) = next_word(data, &mut at) {
            if word == b"begin" {
                break;
            }
        }
        Some(count)
    };
    let mut glyphs = Vec::new();
    loop {
        skip_space(data, &mut at);
        if dictionary && data.get(at..at.saturating_add(2)) == Some(b">>") {
            break;
        }
        if count.is_some_and(|count| glyphs.len() >= count) {
            break;
        }
        let word = next_word(data, &mut at).ok_or(Type42Error(at))?;
        let name = if let Some(name) = word.strip_prefix(b"/") {
            name
        } else if let Some(name) = word
            .strip_prefix(b"(")
            .and_then(|word| word.strip_suffix(b")"))
        {
            if next_word(data, &mut at) != Some(&b"cvn"[..]) {
                return Err(Type42Error(at));
            }
            name
        } else {
            continue;
        };
        if name.is_empty() {
            return Err(Type42Error(at));
        }
        let index = decimal(next_word(data, &mut at).ok_or(Type42Error(at))?)?;
        glyphs.push((
            name.to_vec(),
            u32::try_from(index).map_err(|_| Type42Error(at))?,
        ));
        if glyphs.len() > data.len() / 4 {
            return Err(Type42Error(at));
        }
    }
    if glyphs.is_empty() {
        return Err(Type42Error(at));
    }
    let notdef = glyphs
        .iter()
        .position(|(name, _)| name == b".notdef")
        .ok_or(Type42Error(at))?;
    glyphs.swap(0, notdef);
    Ok(glyphs)
}

fn next_word<'a>(data: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    skip_space(data, at);
    token(data, at)
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

fn append_part(out: &mut Vec<u8>, part: &[u8], limit: usize) -> Result<(), Type42Error> {
    let part = if part.len() & 1 == 1 && part.last() == Some(&0) {
        part.get(..part.len().saturating_sub(1))
            .ok_or(Type42Error(0))?
    } else {
        part
    };
    let end = out.len().checked_add(part.len()).ok_or(Type42Error(0))?;
    if end > limit || part.is_empty() {
        return Err(Type42Error(0));
    }
    out.extend_from_slice(part);
    Ok(())
}

fn validate_sfnt(sfnt: &[u8]) -> Result<(), Type42Error> {
    let version = sfnt.get(..4).ok_or(Type42Error(0))?;
    if version != b"\0\x01\0\0" && version != b"true" {
        return Err(Type42Error(0));
    }
    let count = usize::from(u16::from_be_bytes(
        sfnt.get(4..6)
            .ok_or(Type42Error(0))?
            .try_into()
            .map_err(|_| Type42Error(0))?,
    ));
    let end = count
        .checked_mul(16)
        .and_then(|n| n.checked_add(12))
        .ok_or(Type42Error(0))?;
    if end > sfnt.len() {
        return Err(Type42Error(0));
    }
    let directory = sfnt.get(12..end).ok_or(Type42Error(0))?;
    for entry in directory.as_chunks::<16>().0 {
        let offset = u32::from_be_bytes(
            entry
                .get(8..12)
                .ok_or(Type42Error(0))?
                .try_into()
                .map_err(|_| Type42Error(0))?,
        );
        let length = u32::from_be_bytes(
            entry
                .get(12..16)
                .ok_or(Type42Error(0))?
                .try_into()
                .map_err(|_| Type42Error(0))?,
        );
        let offset = usize::try_from(offset).map_err(|_| Type42Error(0))?;
        let length = usize::try_from(length).map_err(|_| Type42Error(0))?;
        if offset
            .checked_add(length)
            .filter(|end| *end <= sfnt.len())
            .is_none()
        {
            return Err(Type42Error(0));
        }
    }
    Ok(())
}

fn find_name(data: &[u8], name: &[u8]) -> Option<usize> {
    let mut at = 0usize;
    while let Some(&byte) = data.get(at) {
        if byte == b'%' {
            while let Some(&next) = data.get(at) {
                at = at.saturating_add(1);
                if matches!(next, b'\r' | b'\n') {
                    break;
                }
            }
            continue;
        }
        let end = at.checked_add(name.len())?;
        if data.get(at..end) == Some(name)
            && data
                .get(end)
                .is_none_or(|next| next.is_ascii_whitespace() || b"[]<>".contains(next))
        {
            return Some(at);
        }
        at = at.saturating_add(1);
    }
    None
}

fn skip_space(data: &[u8], at: &mut usize) {
    loop {
        match data.get(*at) {
            Some(byte) if byte.is_ascii_whitespace() => *at = at.saturating_add(1),
            Some(b'%') => {
                while let Some(&next) = data.get(*at) {
                    *at = at.saturating_add(1);
                    if matches!(next, b'\r' | b'\n') {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
}

fn token<'a>(data: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    let start = *at;
    while data
        .get(*at)
        .is_some_and(|b| !b.is_ascii_whitespace() && !b"[]<>".contains(b))
    {
        *at = at.saturating_add(1);
    }
    (start < *at).then(|| data.get(start..*at)).flatten()
}

fn decimal(data: &[u8]) -> Result<usize, Type42Error> {
    data.iter().try_fold(0usize, |n, byte| {
        let d = byte
            .checked_sub(b'0')
            .filter(|d| *d < 10)
            .ok_or(Type42Error(0))?;
        n.checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(d)))
            .ok_or(Type42Error(0))
    })
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => byte.checked_sub(b'0'),
        b'A'..=b'F' => byte.checked_sub(b'A').and_then(|n| n.checked_add(10)),
        b'a'..=b'f' => byte.checked_sub(b'a').and_then(|n| n.checked_add(10)),
        _ => None,
    }
}
