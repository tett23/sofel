//! The Type 1 `/Encoding` declaration.

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

/// The kind of encoding a declaration gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Declared {
    /// `StandardEncoding`.
    Standard,
    /// An explicit array, passed entry by entry.
    Explicit,
}

/// Read a standard or explicit encoding of at most 256 entries, passing
/// each explicit `(code, name)` to `entry`; a `.notdef` name is `None`.
pub(crate) fn scan<'a>(
    cleartext: &'a [u8],
    entry: &mut dyn FnMut(usize, Option<&'a [u8]>),
) -> Result<Declared, EncodingError> {
    let mut search = cleartext;
    let declaration = loop {
        let marker = search
            .windows(9)
            .position(|bytes| bytes == b"/Encoding")
            .ok_or(EncodingError)?;
        let after = marker.checked_add(9).ok_or(EncodingError)?;
        let candidate = search.get(after..).ok_or(EncodingError)?.trim_ascii_start();
        if candidate.starts_with(b"StandardEncoding")
            || candidate.starts_with(b"[")
            || candidate.first().is_some_and(u8::is_ascii_digit)
        {
            break candidate;
        }
        search = search.get(after..).ok_or(EncodingError)?;
    };
    if declaration.starts_with(b"StandardEncoding") {
        return Ok(Declared::Standard);
    }
    if let Some(body) = declaration.strip_prefix(b"[") {
        let end = body
            .iter()
            .position(|byte| *byte == b']')
            .ok_or(EncodingError)?;
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
            if code >= 256 {
                return Err(EncodingError);
            }
            entry(code, (name != b".notdef").then_some(name));
        }
        return Ok(Declared::Explicit);
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
    let mut rest = body.get(..end).ok_or(EncodingError)?;
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
        entry(code, (glyph_name != b".notdef").then_some(glyph_name));
    }
    Ok(Declared::Explicit)
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
