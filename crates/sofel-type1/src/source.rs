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
    /// Read a PFA font with ASCII hexadecimal eexec data.
    pub fn parse_pfa(data: &[u8], max_output: usize) -> Result<Self, SourceError> {
        if !data.starts_with(b"%!PS-AdobeFont") {
            return Err(SourceError);
        }
        let marker = data
            .windows(5)
            .position(|window| window == b"eexec")
            .ok_or(SourceError)?;
        let after = marker.checked_add(5).ok_or(SourceError)?;
        let line_end = data
            .get(after..)
            .ok_or(SourceError)?
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(SourceError)?;
        let start = after
            .checked_add(line_end)
            .and_then(|n| n.checked_add(1))
            .ok_or(SourceError)?;
        let mut cipher = Vec::new();
        let mut high = None;
        let mut ended = false;
        for line in data
            .get(start..)
            .ok_or(SourceError)?
            .split(|byte| *byte == b'\n')
        {
            let line = line.trim_ascii();
            if line.starts_with(b"cleartomark") {
                ended = true;
                break;
            }
            for &byte in line {
                if byte.is_ascii_whitespace() {
                    continue;
                }
                let nibble = hex_nibble(byte).ok_or(SourceError)?;
                if let Some(upper) = high.take() {
                    if cipher.len() >= max_output {
                        return Err(SourceError);
                    }
                    cipher.push((upper << 4) | nibble);
                } else {
                    high = Some(nibble);
                }
            }
        }
        if !ended || high.is_some() {
            return Err(SourceError);
        }
        let private = decrypt_eexec(&cipher).ok_or(SourceError)?;
        Ok(Self {
            cleartext: data.get(..start).ok_or(SourceError)?.to_vec(),
            private,
        })
    }

    /// Read the PostScript and eexec sections from a PFB font.
    pub fn parse_pfb(data: &[u8], max_output: usize) -> Result<Self, SourceError> {
        let pfb = Pfb::parse(data, max_output).map_err(|_| SourceError)?;
        let first = pfb.sections().first().ok_or(SourceError)?;
        if first.kind != SectionKind::Ascii
            || !first.data.starts_with(b"%!PS-AdobeFont")
            || !first.data.windows(5).any(|window| window == b"eexec")
        {
            return Err(SourceError);
        }
        let mut cipher = Vec::new();
        let mut trailing = false;
        for section in pfb.sections().iter().skip(1) {
            match section.kind {
                SectionKind::Binary if !trailing => cipher.extend_from_slice(section.data),
                SectionKind::Ascii => trailing = true,
                SectionKind::Binary => return Err(SourceError),
            }
        }
        let private = decrypt_eexec(&cipher).ok_or(SourceError)?;
        Ok(Self {
            cleartext: first.data.to_vec(),
            private,
        })
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
