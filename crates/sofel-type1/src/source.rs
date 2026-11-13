//! Type 1 source separation for PFB fonts.

use alloc::vec::Vec;
use core::fmt;

use crate::cipher::decrypt_eexec;
use crate::pfb::{Pfb, SectionKind};

/// Invalid Type 1 source or PFB container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceError;

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 font source")
    }
}

impl core::error::Error for SourceError {}

/// The cleartext program and decrypted private dictionary of a Type 1 font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type1Source {
    cleartext: Vec<u8>,
    private: Vec<u8>,
}

impl Type1Source {
    /// Recognize either cleartext Type 1 signature accepted by FreeType.
    pub fn has_text_header(data: &[u8]) -> bool {
        data.starts_with(b"%!PS-AdobeFont") || data.starts_with(b"%!FontType")
    }

    /// Read a PFA font with ASCII hexadecimal eexec data.
    pub fn parse_pfa(data: &[u8], max_output: usize) -> Result<Self, SourceError> {
        Self::parse_pfa_with_offset(data, max_output).map_err(|_| SourceError)
    }

    /// Read a PFA source and report a file-relative failure offset.
    pub fn parse_pfa_with_offset(data: &[u8], max_output: usize) -> Result<Self, usize> {
        if !Self::has_text_header(data) {
            return Err(0);
        }
        let after = eexec_end(data).ok_or(data.len())?;
        let whitespace = data
            .get(after..)
            .ok_or(data.len())?
            .iter()
            .take_while(|byte| byte.is_ascii_whitespace())
            .count();
        let start = after.checked_add(whitespace).ok_or(data.len())?;
        let mut cipher = Vec::new();
        let mut high = None;
        let mut ended = false;
        for line in data
            .get(start..)
            .ok_or(data.len())?
            .split(|byte| matches!(byte, b'\r' | b'\n'))
        {
            let line = line.trim_ascii();
            if line.starts_with(b"cleartomark") {
                ended = true;
                break;
            }
            let base = (line.as_ptr() as usize).saturating_sub(data.as_ptr() as usize);
            for (index, &byte) in line.iter().enumerate() {
                if byte.is_ascii_whitespace() {
                    continue;
                }
                let nibble = hex_nibble(byte).ok_or(base.saturating_add(index))?;
                if let Some(upper) = high.take() {
                    if cipher.len() >= max_output {
                        return Err(data.len());
                    }
                    cipher.push((upper << 4) | nibble);
                } else {
                    high = Some(nibble);
                }
            }
        }
        if !ended || high.is_some() {
            return Err(data.len());
        }
        let private = decrypt_eexec(&cipher).ok_or(data.len())?;
        Ok(Self {
            cleartext: data.get(..start).ok_or(data.len())?.to_vec(),
            private,
        })
    }

    /// Read the PostScript and eexec sections from a PFB font.
    pub fn parse_pfb(data: &[u8], max_output: usize) -> Result<Self, SourceError> {
        Self::parse_pfb_with_offset(data, max_output).map_err(|_| SourceError)
    }

    /// Read a PFB source and report a file-relative failure offset.
    pub fn parse_pfb_with_offset(data: &[u8], max_output: usize) -> Result<Self, usize> {
        let pfb = Pfb::parse_with_offset(data, max_output)?;
        let first = pfb.sections().first().ok_or(data.len())?;
        if first.kind != SectionKind::Ascii
            || !Self::has_text_header(first.data)
            || eexec_end(first.data).is_none()
        {
            return Err(6);
        }
        let mut cipher = Vec::new();
        let mut trailing = false;
        for section in pfb.sections().iter().skip(1) {
            match section.kind {
                SectionKind::Binary if !trailing => cipher.extend_from_slice(section.data),
                SectionKind::Ascii => trailing = true,
                SectionKind::Binary => {
                    return Err(
                        (section.data.as_ptr() as usize).saturating_sub(data.as_ptr() as usize)
                    );
                }
            }
        }
        let private = decrypt_eexec(&cipher).ok_or(data.len())?;
        Ok(Self {
            cleartext: first.data.to_vec(),
            private,
        })
    }

    /// Locate a decrypted private dictionary byte in the original PFA or PFB.
    pub fn private_file_offset(&self, file: &[u8], index: usize) -> Option<usize> {
        let mut cipher_index = index.checked_add(4)?;
        if Self::has_text_header(file) {
            let start = self.cleartext.len();
            let (relative, _) = file
                .get(start..)?
                .iter()
                .enumerate()
                .filter(|(_, byte)| byte.is_ascii_hexdigit())
                .nth(cipher_index.checked_mul(2)?)?;
            return start.checked_add(relative);
        }
        let pfb = Pfb::parse(file, file.len()).ok()?;
        for section in pfb
            .sections()
            .iter()
            .filter(|s| s.kind == SectionKind::Binary)
        {
            if cipher_index < section.data.len() {
                return (section.data.as_ptr() as usize)
                    .checked_sub(file.as_ptr() as usize)?
                    .checked_add(cipher_index);
            }
            cipher_index = cipher_index.checked_sub(section.data.len())?;
        }
        Some(file.len())
    }

    /// The unencrypted PostScript program before `eexec`.
    pub fn cleartext(&self) -> &[u8] {
        &self.cleartext
    }

    /// The decrypted private dictionary after the four random bytes.
    pub fn private(&self) -> &[u8] {
        &self.private
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => byte.checked_sub(b'0'),
        b'a'..=b'f' => byte.checked_sub(b'a').and_then(|n| n.checked_add(10)),
        b'A'..=b'F' => byte.checked_sub(b'A').and_then(|n| n.checked_add(10)),
        _ => None,
    }
}

/// Locate the encryption operator among top-level PostScript tokens.
fn eexec_end(data: &[u8]) -> Option<usize> {
    let mut bytes = data.iter().copied().enumerate().peekable();
    let mut procedure_depth = 0usize;
    while let Some((start, byte)) = bytes.next() {
        match byte {
            b'%' => {
                bytes
                    .by_ref()
                    .take_while(|(_, b)| !matches!(b, b'\r' | b'\n'))
                    .count();
            }
            b'(' => {
                let mut depth = 1usize;
                while depth > 0 {
                    match bytes.next()?.1 {
                        b'\\' => {
                            bytes.next()?;
                        }
                        b'(' => depth = depth.checked_add(1)?,
                        b')' => depth = depth.checked_sub(1)?,
                        _ => {}
                    }
                }
            }
            b'<' if bytes.next_if(|(_, b)| *b == b'<').is_none() => {
                bytes.find(|(_, b)| *b == b'>')?;
            }
            b'{' => procedure_depth = procedure_depth.checked_add(1)?,
            b'}' => procedure_depth = procedure_depth.saturating_sub(1),
            b if b == b'/' || !ps_delimiter(b) => {
                while bytes.next_if(|(_, next)| !ps_delimiter(*next)).is_some() {}
                let end = bytes.peek().map_or(data.len(), |(offset, _)| *offset);
                if procedure_depth == 0 && data.get(start..end) == Some(b"eexec") {
                    return Some(end);
                }
            }
            _ => {}
        }
    }
    None
}

fn ps_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace() || b"()<>[]{}/%".contains(&byte)
}
