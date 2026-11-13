//! AFM pair metrics used by Type 1 faces (sofel ADR 0065).

use alloc::vec::Vec;
use core::fmt;

/// Invalid or incomplete Adobe Font Metrics data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AfmError;

impl fmt::Display for AfmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid AFM metrics")
    }
}

impl core::error::Error for AfmError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pair<'a> {
    left: &'a [u8],
    right: &'a [u8],
    x: i32,
    y: i32,
}

/// Parsed AFM pair metrics, borrowing glyph names from the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Afm<'a> {
    pairs: Vec<Pair<'a>>,
}

impl<'a> Afm<'a> {
    /// Parse an AFM file with checked pair counts and a required end marker.
    pub fn parse(data: &'a [u8]) -> Result<Self, AfmError> {
        let mut lines = data.split(|byte| *byte == b'\n' || *byte == b'\r');
        let first = lines.next().ok_or(AfmError)?;
        if !first.starts_with(b"StartFontMetrics ") {
            return Err(AfmError);
        }

        let mut pairs = Vec::new();
        let mut remaining = None;
        let mut complete = false;
        for line in lines {
            let mut words = line
                .split(|byte| *byte == b' ' || *byte == b'\t')
                .filter(|word| !word.is_empty());
            let Some(key) = words.next() else { continue };
            match key {
                b"StartKernPairs" | b"StartKernPairs0" | b"StartKernPairs1" => {
                    if remaining.is_some() {
                        return Err(AfmError);
                    }
                    let count = decimal(words.next().ok_or(AfmError)?)?;
                    if count > data.len() / 10 {
                        return Err(AfmError);
                    }
                    remaining = Some(count);
                }
                b"KP" | b"KPX" | b"KPY" => {
                    let left = words.next().ok_or(AfmError)?;
                    let right = words.next().ok_or(AfmError)?;
                    let first = signed_decimal(words.next().ok_or(AfmError)?)?;
                    let (x, y) = if key == b"KPY" {
                        (0, first)
                    } else if key == b"KP" {
                        (first, signed_decimal(words.next().ok_or(AfmError)?)?)
                    } else {
                        (first, 0)
                    };
                    if words.next().is_some() || left.is_empty() || right.is_empty() {
                        return Err(AfmError);
                    }
                    let count = remaining.as_mut().ok_or(AfmError)?;
                    *count = count.checked_sub(1).ok_or(AfmError)?;
                    pairs.push(Pair { left, right, x, y });
                }
                b"EndKernPairs" => {
                    if remaining.take() != Some(0) {
                        return Err(AfmError);
                    }
                }
                b"EndFontMetrics" => {
                    if remaining.is_some() {
                        return Err(AfmError);
                    }
                    complete = true;
                    break;
                }
                _ => {}
            }
        }
        if !complete {
            return Err(AfmError);
        }
        pairs.sort_by(|a, b| (a.left, a.right).cmp(&(b.left, b.right)));
        Ok(Self { pairs })
    }

    /// Return the horizontal and vertical adjustment for two glyph names.
    pub fn kerning(&self, left: &[u8], right: &[u8]) -> (i32, i32) {
        self.pairs
            .binary_search_by(|pair| (pair.left, pair.right).cmp(&(left, right)))
            .ok()
            .and_then(|index| self.pairs.get(index))
            .map_or((0, 0), |pair| (pair.x, pair.y))
    }
}

fn decimal(data: &[u8]) -> Result<usize, AfmError> {
    if data.is_empty() {
        return Err(AfmError);
    }
    data.iter().try_fold(0usize, |value, byte| {
        if !byte.is_ascii_digit() {
            return Err(AfmError);
        }
        value
            .checked_mul(10)
            .and_then(|v| v.checked_add(usize::from(byte.checked_sub(b'0')?)))
            .ok_or(AfmError)
    })
}

fn signed_decimal(data: &[u8]) -> Result<i32, AfmError> {
    let (negative, digits) = if let Some(rest) = data.strip_prefix(b"-") {
        (true, rest)
    } else {
        (false, data)
    };
    let value = decimal(digits)?;
    let signed = i64::try_from(value).map_err(|_| AfmError)?;
    let signed = if negative {
        signed.checked_neg().ok_or(AfmError)?
    } else {
        signed
    };
    i32::try_from(signed).map_err(|_| AfmError)
}
