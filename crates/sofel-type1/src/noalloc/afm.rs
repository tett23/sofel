use core::fmt;

use crate::noalloc::Problem;
use crate::noalloc::text::offset_in;

/// Invalid or incomplete Adobe Font Metrics data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AfmError;

impl fmt::Display for AfmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid AFM metrics")
    }
}

impl core::error::Error for AfmError {}

/// One kerning pair, borrowing the glyph names from the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pair<'a> {
    pub(crate) left: &'a [u8],
    pub(crate) right: &'a [u8],
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// Read an AFM file with checked pair counts and a required end marker,
/// passing each kerning pair to `pair`. Fails with the offset of the line
/// in error, or the end of the data when a marker is missing.
pub(crate) fn scan<'a>(data: &'a [u8], pair: &mut dyn FnMut(Pair<'a>)) -> Result<(), usize> {
    let mut lines = data.split(|byte| *byte == b'\n' || *byte == b'\r');
    let first = lines.next().ok_or(0usize)?;
    if !first.starts_with(b"StartFontMetrics ") {
        return Err(0);
    }
    let mut remaining = None;
    for line in lines {
        let at = offset_in(data, line);
        let mut words = line
            .split(|byte| *byte == b' ' || *byte == b'\t')
            .filter(|word| !word.is_empty());
        let Some(key) = words.next() else { continue };
        match key {
            b"StartKernPairs" | b"StartKernPairs0" | b"StartKernPairs1" => {
                if remaining.is_some() {
                    return Err(at);
                }
                let count = decimal(words.next().ok_or(at)?).map_err(|_| at)?;
                if count > data.len() / 10 {
                    return Err(at);
                }
                remaining = Some(count);
            }
            b"KP" | b"KPX" | b"KPY" => {
                let left = words.next().ok_or(at)?;
                let right = words.next().ok_or(at)?;
                let first = signed_decimal(words.next().ok_or(at)?).map_err(|_| at)?;
                let (x, y) = if key == b"KPY" {
                    (0, first)
                } else if key == b"KP" {
                    (
                        first,
                        signed_decimal(words.next().ok_or(at)?).map_err(|_| at)?,
                    )
                } else {
                    (first, 0)
                };
                if words.next().is_some() || left.is_empty() || right.is_empty() {
                    return Err(at);
                }
                let count = remaining.as_mut().ok_or(at)?;
                *count = count.checked_sub(1).ok_or(at)?;
                pair(Pair { left, right, x, y });
            }
            b"EndKernPairs" => {
                if remaining.take() != Some(0) {
                    return Err(at);
                }
            }
            b"EndFontMetrics" => {
                if remaining.is_some() {
                    return Err(at);
                }
                return Ok(());
            }
            _ => {}
        }
    }
    Err(data.len())
}

/// Validate AFM kerning data: the header, the pair lines and their counts,
/// and the end marker.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Problem)) {
    if let Err(offset) = scan(data, &mut |_| {}) {
        report(Problem {
            offset,
            reason: "invalid AFM metrics",
        });
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
