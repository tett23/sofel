//! The `/sfnts` bytes in a Type 42 PostScript font.

use alloc::vec::Vec;
use core::fmt;

/// An invalid or truncated Type 42 wrapper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Type42Error;

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
}

impl Type42Source {
    /// Extract the `/sfnts` array, with an upper bound on its decoded length.
    pub fn parse(data: &[u8], max_output: usize) -> Result<Self, Type42Error> {
        if !data.starts_with(b"%!PS-TrueTypeFont") {
            return Err(Type42Error);
        }
        let font_type = find_name(data, b"/FontType").ok_or(Type42Error)?;
        let mut at = font_type
            .checked_add(b"/FontType".len())
            .ok_or(Type42Error)?;
        skip_space(data, &mut at);
        if token(data, &mut at) != Some(&b"42"[..]) {
            return Err(Type42Error);
        }
        let sfnts = find_name(data, b"/sfnts").ok_or(Type42Error)?;
        at = sfnts.checked_add(b"/sfnts".len()).ok_or(Type42Error)?;
        skip_space(data, &mut at);
        if data.get(at) != Some(&b'[') {
            return Err(Type42Error);
        }
        at = at.checked_add(1).ok_or(Type42Error)?;
        let mut sfnt = Vec::new();
        let mut kind = None;
        loop {
            skip_space(data, &mut at);
            match data.get(at).copied().ok_or(Type42Error)? {
                b']' => break,
                b'<' => {
                    if matches!(kind, Some(b'B')) {
                        return Err(Type42Error);
                    }
                    kind = Some(b'H');
                    at = at.checked_add(1).ok_or(Type42Error)?;
                    let mut part = Vec::new();
                    let mut high = None;
                    loop {
                        let byte = *data.get(at).ok_or(Type42Error)?;
                        at = at.checked_add(1).ok_or(Type42Error)?;
                        if byte == b'>' {
                            break;
                        }
                        if byte.is_ascii_whitespace() {
                            continue;
                        }
                        let nibble = hex_nibble(byte).ok_or(Type42Error)?;
                        if let Some(upper) = high.take() {
                            if part.len() > max_output.saturating_sub(sfnt.len()) {
                                return Err(Type42Error);
                            }
                            part.push((upper << 4) | nibble);
                        } else {
                            high = Some(nibble);
                        }
                    }
                    if let Some(upper) = high {
                        if part.len() > max_output.saturating_sub(sfnt.len()) {
                            return Err(Type42Error);
                        }
                        part.push(upper << 4);
                    }
                    append_part(&mut sfnt, &part, max_output)?;
                }
                b'0'..=b'9' => {
                    if matches!(kind, Some(b'H')) {
                        return Err(Type42Error);
                    }
                    kind = Some(b'B');
                    let length = decimal(token(data, &mut at).ok_or(Type42Error)?)?;
                    skip_space(data, &mut at);
                    if token(data, &mut at) != Some(&b"RD"[..]) {
                        return Err(Type42Error);
                    }
                    let sep = *data.get(at).ok_or(Type42Error)?;
                    if !sep.is_ascii_whitespace() {
                        return Err(Type42Error);
                    }
                    at = at.checked_add(1).ok_or(Type42Error)?;
                    let end = at.checked_add(length).ok_or(Type42Error)?;
                    let part = data.get(at..end).ok_or(Type42Error)?;
                    append_part(&mut sfnt, part, max_output)?;
                    at = end;
                }
                _ => return Err(Type42Error),
            }
        }
        validate_sfnt(&sfnt)?;
        Ok(Self { sfnt })
    }

    /// Decoded sfnt bytes for the existing font reader.
    pub fn sfnt(&self) -> &[u8] {
        &self.sfnt
    }
}

fn append_part(out: &mut Vec<u8>, part: &[u8], limit: usize) -> Result<(), Type42Error> {
    let part = if part.len() & 1 == 1 && part.last() == Some(&0) {
        part.get(..part.len().saturating_sub(1))
            .ok_or(Type42Error)?
    } else {
        part
    };
    let end = out.len().checked_add(part.len()).ok_or(Type42Error)?;
    if end > limit || part.is_empty() {
        return Err(Type42Error);
    }
    out.extend_from_slice(part);
    Ok(())
}

fn validate_sfnt(sfnt: &[u8]) -> Result<(), Type42Error> {
    let version = sfnt.get(..4).ok_or(Type42Error)?;
    if version != b"\0\x01\0\0" && version != b"true" {
        return Err(Type42Error);
    }
    let count = usize::from(u16::from_be_bytes(
        sfnt.get(4..6)
            .ok_or(Type42Error)?
            .try_into()
            .map_err(|_| Type42Error)?,
    ));
    let end = count
        .checked_mul(16)
        .and_then(|n| n.checked_add(12))
        .ok_or(Type42Error)?;
    if end > sfnt.len() {
        return Err(Type42Error);
    }
    let directory = sfnt.get(12..end).ok_or(Type42Error)?;
    for entry in directory.as_chunks::<16>().0 {
        let offset = u32::from_be_bytes(
            entry
                .get(8..12)
                .ok_or(Type42Error)?
                .try_into()
                .map_err(|_| Type42Error)?,
        );
        let length = u32::from_be_bytes(
            entry
                .get(12..16)
                .ok_or(Type42Error)?
                .try_into()
                .map_err(|_| Type42Error)?,
        );
        let offset = usize::try_from(offset).map_err(|_| Type42Error)?;
        let length = usize::try_from(length).map_err(|_| Type42Error)?;
        if offset
            .checked_add(length)
            .filter(|end| *end <= sfnt.len())
            .is_none()
        {
            return Err(Type42Error);
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
                if next == b'\n' {
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
                    if next == b'\n' {
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
            .ok_or(Type42Error)?;
        n.checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(d)))
            .ok_or(Type42Error)
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
