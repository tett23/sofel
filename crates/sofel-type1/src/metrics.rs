//! Type 1 charstring side bearings and advances.

use alloc::vec::Vec;
use core::fmt;

/// Invalid or incomplete charstring metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricsError;

impl fmt::Display for MetricsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 metrics")
    }
}

impl core::error::Error for MetricsError {}

/// Unscaled Type 1 side bearings and advance vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    /// Horizontal side bearing.
    pub left: i32,
    /// Vertical side bearing.
    pub top: i32,
    /// Horizontal advance.
    pub advance_x: i32,
    /// Vertical advance.
    pub advance_y: i32,
}

/// Read the first `hsbw` or `sbw` instruction of a decrypted charstring.
pub fn parse_metrics(program: &[u8]) -> Result<Metrics, MetricsError> {
    let mut stack = Vec::new();
    let mut at = 0usize;
    while let Some(&byte) = program.get(at) {
        at = at.checked_add(1).ok_or(MetricsError)?;
        match byte {
            13 => {
                let [left, advance_x] = stack.as_slice() else {
                    return Err(MetricsError);
                };
                return Ok(Metrics {
                    left: *left,
                    top: 0,
                    advance_x: *advance_x,
                    advance_y: 0,
                });
            }
            12 => {
                let escape = *program.get(at).ok_or(MetricsError)?;
                at = at.checked_add(1).ok_or(MetricsError)?;
                if escape == 7 {
                    let [left, top, advance_x, advance_y] = stack.as_slice() else {
                        return Err(MetricsError);
                    };
                    return Ok(Metrics {
                        left: *left,
                        top: *top,
                        advance_x: *advance_x,
                        advance_y: *advance_y,
                    });
                }
                if escape == 12 {
                    let denominator = stack.pop().ok_or(MetricsError)?;
                    let numerator = stack.pop().ok_or(MetricsError)?;
                    stack.push(numerator.checked_div(denominator).ok_or(MetricsError)?);
                } else {
                    stack.clear();
                }
            }
            32..=246 => stack.push(i32::from(byte).checked_sub(139).ok_or(MetricsError)?),
            247..=250 => {
                let next = i32::from(*program.get(at).ok_or(MetricsError)?);
                at = at.checked_add(1).ok_or(MetricsError)?;
                let value = i32::from(byte)
                    .checked_sub(247)
                    .and_then(|n| n.checked_mul(256))
                    .and_then(|n| n.checked_add(next))
                    .and_then(|n| n.checked_add(108))
                    .ok_or(MetricsError)?;
                stack.push(value);
            }
            251..=254 => {
                let next = i32::from(*program.get(at).ok_or(MetricsError)?);
                at = at.checked_add(1).ok_or(MetricsError)?;
                let value = i32::from(byte)
                    .checked_sub(251)
                    .and_then(|n| n.checked_mul(256))
                    .and_then(|n| n.checked_add(next))
                    .and_then(|n| n.checked_add(108))
                    .and_then(i32::checked_neg)
                    .ok_or(MetricsError)?;
                stack.push(value);
            }
            255 => {
                let end = at.checked_add(4).ok_or(MetricsError)?;
                let bytes = program.get(at..end).ok_or(MetricsError)?;
                stack.push(i32::from_be_bytes(
                    bytes.try_into().map_err(|_| MetricsError)?,
                ));
                at = end;
            }
            14 => return Err(MetricsError),
            _ => stack.clear(),
        }
        if stack.len() > 24 {
            return Err(MetricsError);
        }
    }
    Err(MetricsError)
}
