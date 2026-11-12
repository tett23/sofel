//! TrueType bytecode execution (sofel ADR 0064).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0064_i1_branches_skip_nested_blocks_and_push_payloads() {
        let mut stack = alloc::vec::Vec::new();
        // Both halves contain opcode-looking payload bytes. The nested
        // ELSE belongs to its own IF and cannot end the outer false arm.
        let program = [
            0xb0, 0, 0x58, 0x40, 3, 0x1b, 0x58, 0x59, 0x58, 0x1b, 0x59, 0x1b, 0xb0, 1, 0x58, 0xb8,
            0xff, 0xfe, 0x1b, 0xb0, 0x59, 0x59, 0x59,
        ];
        run(&program, &mut stack, 4).unwrap();
        assert_eq!(stack, [-2]);
    }

    #[test]
    fn adr0064_i1_truncated_push_and_unmatched_conditional_fail() {
        for code in [
            &[0x40][..],
            &[0x41, 1, 0],
            &[0xb7, 0],
            &[0xb0, 0, 0x58, 0xb0, 0x59],
        ] {
            assert_eq!(
                run(code, &mut alloc::vec::Vec::new(), 8),
                Err(Error::CodeOverflow)
            );
        }
        assert_eq!(
            run(&[0x58], &mut alloc::vec::Vec::new(), 8),
            Err(Error::StackUnderflow)
        );
        assert_eq!(
            run(&[0xb1, 1, 2], &mut alloc::vec::Vec::new(), 1),
            Err(Error::StackOverflow)
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    CodeOverflow,
    StackUnderflow,
    StackOverflow,
    InvalidOpcode,
    DivideByZero,
}

struct Instruction<'a> {
    opcode: u8,
    immediate: &'a [u8],
    words: bool,
}

fn next<'a>(code: &mut &'a [u8]) -> Result<Instruction<'a>, Error> {
    let (&opcode, rest) = code.split_first().ok_or(Error::CodeOverflow)?;
    let words = opcode == 0x41 || (0xb8..=0xbf).contains(&opcode);
    let (count, rest) = match opcode {
        0x40 | 0x41 => {
            let (&n, rest) = rest.split_first().ok_or(Error::CodeOverflow)?;
            (usize::from(n), rest)
        }
        0xb0..=0xbf => (usize::from(opcode & 7) + 1, rest),
        _ => (0, rest),
    };
    let bytes = if words { count * 2 } else { count };
    let (immediate, remaining) = rest.split_at_checked(bytes).ok_or(Error::CodeOverflow)?;
    *code = remaining;
    Ok(Instruction {
        opcode,
        immediate,
        words,
    })
}

fn skip_block(code: &mut &[u8], stop_at_else: bool) -> Result<(), Error> {
    let mut depth = 0usize;
    loop {
        match next(code)?.opcode {
            0x58 => depth += 1,
            0x1b if depth == 0 && stop_at_else => return Ok(()),
            0x59 if depth == 0 => return Ok(()),
            0x59 => depth -= 1,
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum Rounding {
    Grid,
    HalfGrid,
    DoubleGrid,
    Super {
        period: i64,
        phase: i64,
        threshold: i64,
        diagonal: bool,
    },
    Off,
    Up,
    Down,
}

impl Rounding {
    fn super_round(selector: i64, diagonal: bool) -> Self {
        let grid = if diagonal { 0x2d41 } else { 0x4000 };
        let period = match selector & 0xc0 {
            0 => grid / 2,
            0x80 => grid * 2,
            _ => grid,
        };
        let phase = period * ((selector >> 4) & 3) / 4;
        let threshold = match selector & 15 {
            0 => period - 1,
            n => (n - 4) * period / 8,
        };
        Self::Super {
            period: period >> 8,
            phase: phase >> 8,
            threshold: threshold >> 8,
            diagonal,
        }
    }

    fn apply(&self, distance: i64) -> i64 {
        let magnitude = distance.wrapping_abs();
        let rounded = match self {
            Self::Grid => magnitude.wrapping_add(32) & !63,
            Self::HalfGrid => (magnitude & !63).wrapping_add(32),
            Self::DoubleGrid => magnitude.wrapping_add(16) & !31,
            Self::Super {
                period,
                phase,
                threshold,
                diagonal,
            } => {
                let shifted = magnitude.wrapping_add(threshold - phase);
                let base = if *diagonal {
                    shifted / period * period
                } else {
                    shifted & -period
                };
                let snapped = base.wrapping_add(*phase);
                if snapped < 0 { *phase } else { snapped }
            }
            Self::Off => return distance,
            Self::Up => magnitude.wrapping_add(63) & !63,
            Self::Down => magnitude & !63,
        };
        if distance < 0 {
            -rounded.max(0)
        } else {
            rounded.max(0)
        }
    }
}

#[derive(Clone)]
struct Interpreter {
    rounding: Rounding,
    storage: alloc::vec::Vec<i64>,
}

impl Interpreter {
    fn new(max_storage: u16) -> Self {
        Self {
            rounding: Rounding::Grid,
            storage: alloc::vec![0; usize::from(max_storage)],
        }
    }

    fn execute(
        &mut self,
        mut code: &[u8],
        stack: &mut alloc::vec::Vec<i64>,
        max_stack: usize,
    ) -> Result<(), Error> {
        while !code.is_empty() {
            let instruction = next(&mut code)?;
            match instruction.opcode {
                0x40 | 0x41 | 0xb0..=0xbf => {
                    let data = instruction.immediate;
                    let count = if instruction.words {
                        data.len() / 2
                    } else {
                        data.len()
                    };
                    if count > max_stack.saturating_sub(stack.len()) {
                        return Err(Error::StackOverflow);
                    }
                    if instruction.words {
                        stack.extend(
                            data.as_chunks::<2>()
                                .0
                                .iter()
                                .map(|v| i64::from(i16::from_be_bytes(*v))),
                        );
                    } else {
                        stack.extend(data.iter().copied().map(i64::from));
                    }
                }
                op @ (0x76 | 0x77) => {
                    let selector = stack.pop().ok_or(Error::StackUnderflow)?;
                    self.rounding = Rounding::super_round(selector, op == 0x77);
                }
                0x18 => self.rounding = Rounding::Grid,
                0x19 => self.rounding = Rounding::HalfGrid,
                0x3d => self.rounding = Rounding::DoubleGrid,
                0x7a => self.rounding = Rounding::Off,
                0x7c => self.rounding = Rounding::Up,
                0x7d => self.rounding = Rounding::Down,
                op @ (0x56..=0x57 | 0x68..=0x6f) => {
                    let value = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *value = match op {
                        0x56 => i64::from(self.rounding.apply(*value) & 64 != 0),
                        0x57 => i64::from(self.rounding.apply(*value) & 64 == 0),
                        0x68..=0x6b => self.rounding.apply(*value),
                        _ => *value,
                    };
                }
                0x42 => {
                    let value = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)?;
                    if let Some(slot) = usize::try_from(index)
                        .ok()
                        .and_then(|i| self.storage.get_mut(i))
                    {
                        *slot = value;
                    }
                }
                0x43 => {
                    let index = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *index = usize::try_from(*index)
                        .ok()
                        .and_then(|i| self.storage.get(i))
                        .copied()
                        .unwrap_or(0);
                }
                0x20 => {
                    let value = *stack.last().ok_or(Error::StackUnderflow)?;
                    push(stack, value, max_stack)?;
                }
                0x21 => {
                    stack.pop().ok_or(Error::StackUnderflow)?;
                }
                0x22 => stack.clear(),
                0x23 | 0x8a => {
                    let count = if instruction.opcode == 0x23 { 2 } else { 3 };
                    let start = stack
                        .len()
                        .checked_sub(count)
                        .ok_or(Error::StackUnderflow)?;
                    stack[start..].rotate_left(1);
                }
                0x24 => push(stack, stack.len() as i64, max_stack)?,
                0x25 | 0x26 => {
                    let index = stack.pop().ok_or(Error::StackUnderflow)?;
                    let start = usize::try_from(index)
                        .ok()
                        .filter(|&n| n != 0)
                        .and_then(|n| stack.len().checked_sub(n));
                    if instruction.opcode == 0x25 {
                        let value = start.and_then(|i| stack.get(i)).copied().unwrap_or(0);
                        stack.push(value);
                    } else if let Some(i) = start {
                        stack[i..].rotate_left(1);
                    }
                }
                op @ (0x50..=0x55 | 0x5a..=0x5b | 0x60..=0x63 | 0x8b..=0x8c) => {
                    let [.., left, right] = stack.as_mut_slice() else {
                        return Err(Error::StackUnderflow);
                    };
                    *left = binary(op, *left, *right)?;
                    stack.pop();
                }
                op @ (0x5c | 0x64..=0x67) => {
                    let value = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *value = match op {
                        0x5c => i64::from(*value == 0),
                        0x64 => value.wrapping_abs(),
                        0x65 => value.wrapping_neg(),
                        0x66 => *value & !63,
                        _ => value.wrapping_add(63) & !63,
                    };
                }
                0x58 => {
                    if stack.pop().ok_or(Error::StackUnderflow)? == 0 {
                        skip_block(&mut code, true)?;
                    }
                }
                0x1b => skip_block(&mut code, false)?,
                0x59 => {}
                _ => return Err(Error::InvalidOpcode),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn run(code: &[u8], stack: &mut alloc::vec::Vec<i64>, max_stack: usize) -> Result<(), Error> {
    Interpreter::new(0).execute(code, stack, max_stack)
}

#[test]
fn adr0064_i1_index_and_roll_select_from_the_top_of_the_stack() {
    let mut values = alloc::vec::Vec::new();
    run(
        &[0xb2, 1, 2, 3, 0x8a, 0xb0, 3, 0x25, 0xb0, 3, 0x26, 0x24],
        &mut values,
        8,
    )
    .unwrap();
    assert_eq!(values, [2, 1, 2, 3, 4]);
    run(&[0x22, 0xb0, 0, 0x25, 0xb0, 9, 0x26], &mut values, 8).unwrap();
    assert_eq!(values, [0]);
}

fn push(stack: &mut alloc::vec::Vec<i64>, value: i64, limit: usize) -> Result<(), Error> {
    if stack.len() >= limit {
        return Err(Error::StackOverflow);
    }
    stack.push(value);
    Ok(())
}

#[test]
fn adr0064_i1_arithmetic_rounds_multiply_and_truncates_division() {
    for (op, a, b, expected) in [
        (0x63, -1, 32, -1),
        (0x63, 1, 31, 0),
        (0x62, -5, 3, -106),
        (0x62, 5, -3, -106),
        (0x50, -7, 2, 1),
        (0x5b, 0, -5, 1),
        (0x8b, -20, -4, -4),
    ] {
        let mut stack = alloc::vec![a, b];
        run(&[op], &mut stack, 2).unwrap();
        assert_eq!(stack, [expected]);
    }
    assert_eq!(
        run(&[0x62], &mut alloc::vec![1, 0], 2),
        Err(Error::DivideByZero)
    );
    for (op, input, expected) in [(0x66, -65, -128), (0x67, -65, -64), (0x5c, -1, 0)] {
        let mut stack = alloc::vec![input];
        run(&[op], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
}

fn binary(op: u8, a: i64, b: i64) -> Result<i64, Error> {
    Ok(match op {
        0x50 => i64::from(a < b),
        0x51 => i64::from(a <= b),
        0x52 => i64::from(a > b),
        0x53 => i64::from(a >= b),
        0x54 => i64::from(a == b),
        0x55 => i64::from(a != b),
        0x5a => i64::from(a != 0 && b != 0),
        0x5b => i64::from(a != 0 || b != 0),
        0x60 => a.wrapping_add(b),
        0x61 => a.wrapping_sub(b),
        0x62 | 0x63 => {
            // FreeType computes unsigned magnitudes, then restores the sign.
            let (x, y) = (a.unsigned_abs(), b.unsigned_abs());
            let magnitude = if op == 0x62 {
                x.wrapping_mul(64)
                    .checked_div(y)
                    .ok_or(Error::DivideByZero)?
            } else {
                x.wrapping_mul(y).wrapping_add(32) >> 6
            }
            .cast_signed();
            if (a < 0) != (b < 0) {
                magnitude.wrapping_neg()
            } else {
                magnitude
            }
        }
        0x8b => a.max(b),
        0x8c => a.min(b),
        _ => return Err(Error::InvalidOpcode),
    })
}

#[test]
fn adr0064_i1_rounding_state_controls_negative_distances_and_parity() {
    for (mode, expected) in [
        (0x18, -64),
        (0x19, -96),
        (0x3d, -64),
        (0x7a, -65),
        (0x7c, -128),
        (0x7d, -64),
    ] {
        let mut stack = alloc::vec![-65];
        run(&[mode, 0x68], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
    for (op, expected) in [(0x56, 1), (0x57, 0), (0x6c, -65)] {
        let mut stack = alloc::vec![-65];
        run(&[op], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
}

#[test]
fn adr0064_i1_super_rounding_keeps_phase_and_diagonal_period() {
    for (op, selector, input, expected) in [
        (0x76, 0x54, 0, 16),
        (0x76, 0x54, -81, -80),
        (0x77, 0x44, 89, 45),
        (0x77, 0x44, -90, -90),
    ] {
        let mut stack = alloc::vec![input, selector];
        run(&[op, 0x68], &mut stack, 2).unwrap();
        assert_eq!(stack, [expected]);
    }
}

#[test]
fn adr0064_i1_glyph_storage_writes_are_private_and_bounded() {
    let mut prep = Interpreter::new(2);
    let mut stack = alloc::vec![1, -42];
    prep.execute(&[0x42], &mut stack, 4).unwrap();
    let mut glyph = prep.clone();
    glyph
        .execute(
            &[0xb1, 1, 99, 0x42, 0xb1, 2, 7, 0x42, 0xb0, 2, 0x43],
            &mut stack,
            4,
        )
        .unwrap();
    assert_eq!(stack, [0]);
    prep.execute(&[0xb0, 1, 0x43], &mut stack, 4).unwrap();
    assert_eq!(stack, [0, -42]);
}
