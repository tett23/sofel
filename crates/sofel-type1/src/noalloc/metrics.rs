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

/// The BuildChar operand stack holds at most 24 numbers (Adobe Type 1
/// Font Format, chapter 6, "Charstring Number Encoding"; also its
/// appendix of implementation limits). A 25th operand is an error.
const STACK_LIMIT: usize = 24;

struct Stack {
    values: [i32; STACK_LIMIT],
    len: usize,
}

impl Stack {
    fn push(&mut self, value: i32) -> Result<(), MetricsError> {
        *self.values.get_mut(self.len).ok_or(MetricsError)? = value;
        self.len = self.len.checked_add(1).ok_or(MetricsError)?;
        Ok(())
    }

    fn pop(&mut self) -> Result<i32, MetricsError> {
        self.len = self.len.checked_sub(1).ok_or(MetricsError)?;
        self.values.get(self.len).copied().ok_or(MetricsError)
    }

    fn as_slice(&self) -> &[i32] {
        self.values.get(..self.len).unwrap_or(&[])
    }
}

/// Read the first `hsbw` or `sbw` instruction of a decrypted charstring.
///
/// The operand stack is a fixed array of the Type 1 limit (sofel ADR 0098).
pub fn parse_metrics(program: &[u8]) -> Result<Metrics, MetricsError> {
    let mut stack = Stack {
        values: [0; STACK_LIMIT],
        len: 0,
    };
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
                    let denominator = stack.pop()?;
                    let numerator = stack.pop()?;
                    stack.push(numerator.checked_div(denominator).ok_or(MetricsError)?)?;
                } else {
                    stack.len = 0;
                }
            }
            32..=246 => stack.push(i32::from(byte).checked_sub(139).ok_or(MetricsError)?)?,
            247..=250 => {
                let next = i32::from(*program.get(at).ok_or(MetricsError)?);
                at = at.checked_add(1).ok_or(MetricsError)?;
                let value = i32::from(byte)
                    .checked_sub(247)
                    .and_then(|n| n.checked_mul(256))
                    .and_then(|n| n.checked_add(next))
                    .and_then(|n| n.checked_add(108))
                    .ok_or(MetricsError)?;
                stack.push(value)?;
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
                stack.push(value)?;
            }
            255 => {
                let end = at.checked_add(4).ok_or(MetricsError)?;
                let bytes = program.get(at..end).ok_or(MetricsError)?;
                stack.push(i32::from_be_bytes(
                    bytes.try_into().map_err(|_| MetricsError)?,
                ))?;
                at = end;
            }
            14 => return Err(MetricsError),
            _ => stack.len = 0,
        }
    }
    Err(MetricsError)
}
