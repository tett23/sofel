//! Type 1 source separation for PFB fonts.

use alloc::vec::Vec;
use core::fmt;

use crate::cipher::decrypt_eexec;
use crate::noalloc::text::has_text_header;
use crate::noalloc::type1::{PfaCipher, PfbCipher, pfa_layout, pfb_layout};
use crate::pfb::{Pfb, SectionKind};

pub use crate::noalloc::type1::{validate_pfa, validate_pfb};

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
        has_text_header(data)
    }

    /// Read a PFA font with ASCII hexadecimal eexec data.
    ///
    /// Allocates a copy of the cleartext and the decrypted private part (ADR 0094).
    pub fn parse_pfa(data: &[u8], max_output: usize) -> Result<Self, SourceError> {
        Self::parse_pfa_with_offset(data, max_output).map_err(|_| SourceError)
    }

    /// Read a PFA source and report a file-relative failure offset.
    ///
    /// Allocates a copy of the cleartext and the decrypted private part (ADR 0094).
    pub fn parse_pfa_with_offset(data: &[u8], max_output: usize) -> Result<Self, usize> {
        let layout = pfa_layout(data, max_output)?;
        let cipher: Vec<u8> = PfaCipher::new(data, layout).collect();
        let private = decrypt_eexec(&cipher).ok_or(data.len())?;
        Ok(Self {
            cleartext: data.get(..layout.start).ok_or(data.len())?.to_vec(),
            private,
        })
    }

    /// Read the PostScript and eexec sections from a PFB font.
    ///
    /// Allocates a copy of the cleartext, the joined binary sections and the decrypted private part (ADR 0094).
    pub fn parse_pfb(data: &[u8], max_output: usize) -> Result<Self, SourceError> {
        Self::parse_pfb_with_offset(data, max_output).map_err(|_| SourceError)
    }

    /// Read a PFB source and report a file-relative failure offset.
    ///
    /// Allocates a copy of the cleartext, the joined binary sections and the decrypted private part (ADR 0094).
    pub fn parse_pfb_with_offset(data: &[u8], max_output: usize) -> Result<Self, usize> {
        let layout = pfb_layout(data, max_output)?;
        let cipher: Vec<u8> = PfbCipher::new(data).collect();
        let private = decrypt_eexec(&cipher).ok_or(data.len())?;
        Ok(Self {
            cleartext: layout.cleartext.to_vec(),
            private,
        })
    }

    /// Locate a decrypted private dictionary byte in the original PFA or PFB.
    pub fn private_file_offset(&self, file: &[u8], index: usize) -> Option<usize> {
        let mut cipher_index = index.checked_add(4)?;
        if has_text_header(file) {
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
