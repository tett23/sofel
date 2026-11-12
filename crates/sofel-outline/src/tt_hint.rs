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

fn run(mut code: &[u8], stack: &mut alloc::vec::Vec<i64>, max_stack: usize) -> Result<(), Error> {
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
