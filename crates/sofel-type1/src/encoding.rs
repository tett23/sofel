//! Type 1 glyph-name encoding.

use crate::encoding_data::STANDARD_NAMES;
use core::fmt;

/// Invalid or unsupported Type 1 encoding declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodingError;

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 encoding")
    }
}

impl core::error::Error for EncodingError {}

/// The glyph name assigned to each eight-bit character code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding<'a> {
    names: [Option<&'a [u8]>; 256],
}

impl<'a> Encoding<'a> {
    /// Read a standard or explicit Type 1 encoding of at most 256 entries.
    pub fn parse(cleartext: &'a [u8]) -> Result<Self, EncodingError> {
        let marker = cleartext
            .windows(9)
            .position(|bytes| bytes == b"/Encoding")
            .ok_or(EncodingError)?;
        let after = marker.checked_add(9).ok_or(EncodingError)?;
        let declaration = cleartext
            .get(after..)
            .ok_or(EncodingError)?
            .trim_ascii_start();
        if declaration.starts_with(b"StandardEncoding") {
            let names = STANDARD_NAMES;
            return Ok(Self { names });
        }
        if let Some(body) = declaration.strip_prefix(b"[") {
            let end = body
                .iter()
                .position(|byte| *byte == b']')
                .ok_or(EncodingError)?;
            let mut names = [None; 256];
            for (code, word) in body
                .get(..end)
                .ok_or(EncodingError)?
                .split(u8::is_ascii_whitespace)
                .filter(|word| !word.is_empty())
                .enumerate()
            {
                let name = word
                    .strip_prefix(b"/")
                    .filter(|name| !name.is_empty())
                    .ok_or(EncodingError)?;
                *names.get_mut(code).ok_or(EncodingError)? = (name != b".notdef").then_some(name);
            }
            return Ok(Self { names });
        }
        let digits = declaration
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let count = decimal(declaration.get(..digits).ok_or(EncodingError)?)?;
        if count > 256 {
            return Err(EncodingError);
        }
        let body = declaration
            .get(digits..)
            .ok_or(EncodingError)?
            .trim_ascii_start()
            .strip_prefix(b"array")
            .ok_or(EncodingError)?;
        let end = body
            .windows(12)
            .position(|bytes| bytes == b"readonly def")
            .or_else(|| body.windows(3).position(|bytes| bytes == b"def"))
            .ok_or(EncodingError)?;
        let body = body.get(..end).ok_or(EncodingError)?;
        let mut names = [None; 256];
        let mut rest = body;
        while let Some(at) = rest.windows(3).position(|word| word == b"dup") {
            rest = rest
                .get(at.saturating_add(3)..)
                .ok_or(EncodingError)?
                .trim_ascii_start();
            let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
            let code = decimal(rest.get(..digits).ok_or(EncodingError)?)?;
            if code >= count {
                return Err(EncodingError);
            }
            rest = rest.get(digits..).ok_or(EncodingError)?.trim_ascii_start();
            let name = rest.strip_prefix(b"/").ok_or(EncodingError)?;
            let name_end = name
                .iter()
                .position(u8::is_ascii_whitespace)
                .ok_or(EncodingError)?;
            let glyph_name = name.get(..name_end).ok_or(EncodingError)?;
            rest = name
                .get(name_end..)
                .ok_or(EncodingError)?
                .trim_ascii_start();
            if glyph_name.is_empty() || !rest.starts_with(b"put") {
                return Err(EncodingError);
            }
            rest = rest.get(3..).ok_or(EncodingError)?;
            *names.get_mut(code).ok_or(EncodingError)? =
                (glyph_name != b".notdef").then_some(glyph_name);
        }
        Ok(Self { names })
    }

    /// The glyph name assigned to `code`, absent for `.notdef`.
    pub fn name(&self, code: u8) -> Option<&'a [u8]> {
        self.names.get(usize::from(code)).copied().flatten()
    }
}

fn decimal(bytes: &[u8]) -> Result<usize, EncodingError> {
    if bytes.is_empty() {
        return Err(EncodingError);
    }
    bytes.iter().try_fold(0usize, |value, byte| {
        let digit = byte
            .checked_sub(b'0')
            .filter(|digit| *digit < 10)
            .ok_or(EncodingError)?;
        value
            .checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(digit)))
            .ok_or(EncodingError)
    })
}
