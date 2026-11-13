//! Index Type 1 charstrings after eexec decryption.

use crate::cipher::decrypt_charstring;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

/// Invalid or truncated Type 1 charstring dictionary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharstringError;

impl fmt::Display for CharstringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 charstrings")
    }
}

impl core::error::Error for CharstringError {}

/// Glyph programs and local subroutines borrowed from a decrypted dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charstrings<'a> {
    glyphs: Vec<(&'a [u8], &'a [u8])>,
    subrs: Vec<Option<&'a [u8]>>,
    len_iv: i32,
}

impl<'a> Charstrings<'a> {
    /// Read `/Subrs` and `/CharStrings` binary objects with checked lengths.
    pub fn parse(data: &'a [u8]) -> Result<Self, CharstringError> {
        Self::parse_with_offset(data).map_err(|_| CharstringError)
    }

    /// Return a byte offset in the decrypted private dictionary on failure.
    pub fn parse_with_offset(data: &'a [u8]) -> Result<Self, usize> {
        let charstrings_start = keyword(data, b"/CharStrings").ok_or(data.len())?;
        let prefix_end = keyword(data, b"/Subrs")
            .unwrap_or(charstrings_start)
            .min(charstrings_start);
        let prefix = data.get(..prefix_end).ok_or(data.len())?;
        let len_iv = if let Some(start) = keyword(prefix, b"/lenIV") {
            let mut reader = Reader {
                data: prefix,
                at: start,
                start,
            };
            if reader.word().map_err(|_| reader.at)? != b"/lenIV" {
                return Err(reader.start);
            }
            signed_number(reader.word().map_err(|_| reader.at)?).map_err(|_| reader.start)?
        } else {
            4
        };
        let mut subrs = Vec::new();
        if let Some(start) = keyword(data, b"/Subrs") {
            let mut reader = Reader {
                data,
                at: start,
                start,
            };
            if reader.word().map_err(|_| reader.at)? != b"/Subrs" {
                return Err(reader.start);
            }
            let count = number(reader.word().map_err(|_| reader.at)?).map_err(|_| reader.start)?;
            if count > data.len() / 4 || reader.word().map_err(|_| reader.at)? != b"array" {
                return Err(reader.start);
            }
            subrs = vec![None; count];
            for _ in 0..count {
                if reader.word().map_err(|_| reader.at)? != b"dup" {
                    return Err(reader.start);
                }
                let index =
                    number(reader.word().map_err(|_| reader.at)?).map_err(|_| reader.start)?;
                let length =
                    number(reader.word().map_err(|_| reader.at)?).map_err(|_| reader.start)?;
                if !matches!(
                    reader.word().map_err(|_| reader.at)?,
                    b"RD" | b"-|" | b"readstring"
                ) {
                    return Err(reader.start);
                }
                let bytes = reader.binary(length).map_err(|_| data.len())?;
                if !matches!(
                    reader.word().map_err(|_| reader.at)?,
                    b"NP" | b"|-" | b"put"
                ) {
                    return Err(reader.start);
                }
                *subrs.get_mut(index).ok_or(reader.start)? = Some(bytes);
            }
        }
        let mut reader = Reader {
            data,
            at: charstrings_start,
            start: charstrings_start,
        };
        if reader.word().map_err(|_| reader.at)? != b"/CharStrings" {
            return Err(reader.start);
        }
        let count = number(reader.word().map_err(|_| reader.at)?).map_err(|_| reader.start)?;
        if count == 0 || count > data.len() / 5 {
            return Err(reader.start);
        }
        let mut glyphs = Vec::with_capacity(count);
        let mut first = reader.word().map_err(|_| reader.at)?;
        while !first.starts_with(b"/") {
            first = reader.word().map_err(|_| reader.at)?;
        }
        for index in 0..count {
            let name = if index == 0 {
                first
            } else {
                reader.word().map_err(|_| reader.at)?
            };
            let name = name
                .strip_prefix(b"/")
                .filter(|name| !name.is_empty())
                .ok_or(reader.start)?;
            let length = number(reader.word().map_err(|_| reader.at)?).map_err(|_| reader.start)?;
            if !matches!(reader.word().map_err(|_| reader.at)?, b"RD" | b"-|") {
                return Err(reader.start);
            }
            let bytes = reader.binary(length).map_err(|_| data.len())?;
            if !matches!(reader.word().map_err(|_| reader.at)?, b"ND" | b"|-") {
                return Err(reader.start);
            }
            glyphs.push((name, bytes));
        }
        Ok(Self {
            glyphs,
            subrs,
            len_iv,
        })
    }

    /// Number of glyph charstrings in source order.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// The encrypted program for a named glyph.
    pub fn glyph(&self, name: &[u8]) -> Option<&'a [u8]> {
        self.glyphs
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, bytes)| *bytes)
    }

    /// The glyph name at its source-order index.
    pub fn glyph_name(&self, index: usize) -> Option<&'a [u8]> {
        self.glyphs.get(index).map(|(name, _)| *name)
    }

    /// The source-order index of a glyph name.
    pub fn glyph_index(&self, name: &[u8]) -> Option<usize> {
        self.glyphs
            .iter()
            .position(|(candidate, _)| *candidate == name)
    }

    /// The encrypted local subroutine at `index`.
    pub fn subroutine(&self, index: usize) -> Option<&'a [u8]> {
        self.subrs.get(index).copied().flatten()
    }

    /// The number of random bytes before each encrypted charstring.
    pub fn len_iv(&self) -> i32 {
        self.len_iv
    }

    /// The decrypted program for a named glyph.
    pub fn glyph_program(&self, name: &[u8]) -> Option<Vec<u8>> {
        decrypt_charstring(self.glyph(name)?, self.len_iv)
    }

    /// The decrypted local subroutine at `index`.
    pub fn subroutine_program(&self, index: usize) -> Option<Vec<u8>> {
        decrypt_charstring(self.subroutine(index)?, self.len_iv)
    }
}

fn keyword(data: &[u8], word: &[u8]) -> Option<usize> {
    data.windows(word.len()).position(|window| window == word)
}

fn number(word: &[u8]) -> Result<usize, CharstringError> {
    if word.is_empty() {
        return Err(CharstringError);
    }
    word.iter().try_fold(0usize, |value, byte| {
        let digit = byte
            .checked_sub(b'0')
            .filter(|digit| *digit < 10)
            .ok_or(CharstringError)?;
        value
            .checked_mul(10)
            .and_then(|n| n.checked_add(usize::from(digit)))
            .ok_or(CharstringError)
    })
}

fn signed_number(word: &[u8]) -> Result<i32, CharstringError> {
    let (negative, digits) = if let Some(rest) = word.strip_prefix(b"-") {
        (true, rest)
    } else {
        (false, word)
    };
    let value = i64::try_from(number(digits)?).map_err(|_| CharstringError)?;
    let value = if negative {
        value.checked_neg().ok_or(CharstringError)?
    } else {
        value
    };
    i32::try_from(value).map_err(|_| CharstringError)
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    start: usize,
}

impl<'a> Reader<'a> {
    fn word(&mut self) -> Result<&'a [u8], CharstringError> {
        loop {
            let next = self.data.get(self.at..).ok_or(CharstringError)?;
            let Some(&byte) = next.first() else {
                return Err(CharstringError);
            };
            if byte == b'%' {
                let length = next
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .ok_or(CharstringError)?;
                self.at = self.at.checked_add(length).ok_or(CharstringError)?;
            } else if byte.is_ascii_whitespace() {
                self.at = self.at.checked_add(1).ok_or(CharstringError)?;
            } else {
                break;
            }
        }
        self.start = self.at;
        let rest = self.data.get(self.at..).ok_or(CharstringError)?;
        let length = rest
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(rest.len());
        let end = self.at.checked_add(length).ok_or(CharstringError)?;
        let word = self.data.get(self.at..end).ok_or(CharstringError)?;
        self.at = end;
        Ok(word)
    }

    fn binary(&mut self, length: usize) -> Result<&'a [u8], CharstringError> {
        let whitespace = *self.data.get(self.at).ok_or(CharstringError)?;
        if !whitespace.is_ascii_whitespace() {
            return Err(CharstringError);
        }
        self.at = self.at.checked_add(1).ok_or(CharstringError)?;
        if whitespace == b'\r' && self.data.get(self.at) == Some(&b'\n') {
            self.at = self.at.checked_add(1).ok_or(CharstringError)?;
        }
        let end = self.at.checked_add(length).ok_or(CharstringError)?;
        let bytes = self.data.get(self.at..end).ok_or(CharstringError)?;
        self.at = end;
        Ok(bytes)
    }
}
