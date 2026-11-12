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
    InvalidReference,
    InvalidArgument,
    ExecutionLimit,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum HintTarget {
    Mono,
    Gray,
    HorizontalLcd,
    VerticalLcd,
}

#[derive(Clone, Copy)]
struct EngineInfo {
    version: u8,
    target: HintTarget,
    rotated: bool,
    stretched: bool,
    variation: bool,
    symmetric_smoothing: bool,
}

impl EngineInfo {
    fn query(self, selector: i64) -> i64 {
        let subpixel = self.version == 40 && self.target != HintTarget::Mono;
        let mut result = if selector & 1 != 0 {
            i64::from(self.version)
        } else {
            0
        };
        for (bit, enabled) in [
            (1, self.rotated),
            (2, self.stretched),
            (3, self.variation),
            (5, self.version == 35 && self.target != HintTarget::Mono),
            (6, subpixel),
            (8, subpixel && self.target == HintTarget::VerticalLcd),
            (10, subpixel),
            (11, subpixel && self.symmetric_smoothing),
            (12, subpixel && self.target == HintTarget::Gray),
        ] {
            if enabled && selector & (1 << bit) != 0 {
                result |= 1 << (bit + 7);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Default)]
struct HintPoint {
    unscaled: [i64; 2],
    original: [i64; 2],
    current: [i64; 2],
    touched: [bool; 2],
}

#[derive(Clone)]
struct Interpreter {
    loop_count: u16,
    points: [alloc::vec::Vec<HintPoint>; 2],
    reference: [u16; 3],
    delta_base: u16,
    delta_shift: u8,
    zones: [u8; 3],
    control_value_cutin: i64,
    single_width_cutin: i64,
    single_width: i64,
    scan_control: bool,
    scan_type: u16,
    ppem: [u16; 2],
    point_size: i64,
    engine: EngineInfo,
    projection: [i64; 2],
    dual_projection: [i64; 2],
    freedom: [i64; 2],
    functions: alloc::collections::BTreeMap<u16, alloc::rc::Rc<[u8]>>,
    rounding: Rounding,
    storage: alloc::vec::Vec<i64>,
    cvt: alloc::vec::Vec<i64>,
    scale: i64,
}

impl Interpreter {
    fn new(max_storage: u16, varied_cvt: &[i32], scale: i64) -> Self {
        Self {
            engine: EngineInfo {
                version: 40,
                target: HintTarget::Gray,
                rotated: false,
                stretched: false,
                variation: false,
                symmetric_smoothing: false,
            },
            points: Default::default(),
            reference: [0; 3],
            loop_count: 1,
            delta_base: 9,
            delta_shift: 3,
            zones: [1; 3],
            control_value_cutin: 68,
            single_width_cutin: 0,
            single_width: 0,
            scan_control: false,
            scan_type: 0,
            ppem: [0; 2],
            point_size: 0,
            projection: [16384, 0],
            dual_projection: [16384, 0],
            freedom: [16384, 0],
            functions: alloc::collections::BTreeMap::new(),
            rounding: Rounding::Grid,
            scale,
            cvt: varied_cvt
                .iter()
                .map(|v| sofel_font::read::cff::ft::mul_fix(i64::from(v / 64), scale))
                .collect(),
            storage: alloc::vec![0; usize::from(max_storage)],
        }
    }

    fn interpolate(&mut self, stack: &mut alloc::vec::Vec<i64>) {
        let count = usize::from(core::mem::replace(&mut self.loop_count, 1));
        let Some(first) = stack.len().checked_sub(count) else {
            return;
        };
        let [z0, z1, z2] = self.zones.map(usize::from);
        let base = self.points[z0].get(usize::from(self.reference[1])).copied();
        let end = self.points[z1].get(usize::from(self.reference[2])).copied();
        let axis = usize::from(self.projection[0] == 0);
        let original_axis = usize::from(self.dual_projection[0] == 0);
        let twilight = self.zones.contains(&0);
        let original = |p: HintPoint| {
            if twilight {
                p.original[original_axis]
            } else {
                p.unscaled[original_axis]
            }
        };
        if let Some(base) = base {
            let old_range = end.map_or(0, |p| original(p).wrapping_sub(original(base)));
            let new_range = end.map_or(0, |p| p.current[axis].wrapping_sub(base.current[axis]));
            for &index in stack[first..].iter().rev() {
                if let Some(p) = self.points[z2].get_mut(index as u32 as usize) {
                    let offset = original(*p).wrapping_sub(original(base));
                    let distance = if old_range == 0 {
                        offset
                    } else {
                        sofel_font::read::cff::ft::mul_div(offset, new_range, old_range)
                    };
                    if self.freedom[axis] != 0 {
                        p.current[axis] = base.current[axis].wrapping_add(distance);
                        p.touched[axis] = true;
                    }
                }
            }
        }
        stack.truncate(first);
    }

    fn projected_ppem(&self) -> i64 {
        i64::from(self.ppem[usize::from(self.projection[0] == 0)])
    }

    fn execute(
        &mut self,
        code: &[u8],
        stack: &mut alloc::vec::Vec<i64>,
        max_stack: usize,
    ) -> Result<(), Error> {
        self.execute_frame(code, stack, max_stack, 0, &mut 1_000_000)
    }

    fn execute_frame(
        &mut self,
        mut code: &[u8],
        stack: &mut alloc::vec::Vec<i64>,
        max_stack: usize,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<(), Error> {
        while !code.is_empty() {
            *remaining = remaining.checked_sub(1).ok_or(Error::ExecutionLimit)?;
            let instruction = next(&mut code)?;
            match instruction.opcode {
                op @ 0x00..=0x05 => {
                    let axis = if op & 1 == 0 { [0, 16384] } else { [16384, 0] };
                    if op < 4 {
                        self.projection = axis;
                        self.dual_projection = axis;
                    }
                    if op & 2 == 0 {
                        self.freedom = axis;
                    }
                }
                op @ (0x0c | 0x0d) => {
                    if max_stack.saturating_sub(stack.len()) < 2 {
                        return Err(Error::StackOverflow);
                    }
                    stack.extend(if op == 0x0c {
                        self.projection
                    } else {
                        self.freedom
                    });
                }
                0x0e => self.freedom = self.projection,
                op @ (0x4b | 0x4c) => {
                    let value = if op == 0x4c && self.engine.version == 40 {
                        self.point_size
                    } else {
                        self.projected_ppem()
                    };
                    push(stack, value, max_stack)?;
                }
                op @ 0x10..=0x12 => {
                    self.reference[usize::from(op - 0x10)] =
                        stack.pop().ok_or(Error::StackUnderflow)? as u16;
                }
                op @ 0x13..=0x16 => {
                    let zone = stack.pop().ok_or(Error::StackUnderflow)? as i32;
                    if matches!(zone, 0 | 1) {
                        if op == 0x16 {
                            self.zones.fill(zone as u8);
                        } else {
                            self.zones[usize::from(op - 0x13)] = zone as u8;
                        }
                    }
                }
                op @ 0x1d..=0x1f => {
                    let value = stack.pop().ok_or(Error::StackUnderflow)?;
                    match op {
                        0x1d => self.control_value_cutin = value,
                        0x1e => self.single_width_cutin = value,
                        _ => {
                            self.single_width =
                                sofel_font::read::cff::ft::mul_fix(value, self.scale)
                        }
                    }
                }
                0x5e => self.delta_base = stack.pop().ok_or(Error::StackUnderflow)? as u16,
                0x5f => {
                    let shift = stack.pop().ok_or(Error::StackUnderflow)?;
                    if !(0..=6).contains(&shift) {
                        return Err(Error::InvalidArgument);
                    }
                    self.delta_shift = shift as u8;
                }
                op @ 0x73..=0x75 => {
                    let count = stack.pop().ok_or(Error::StackUnderflow)?;
                    let pairs = usize::try_from(count)
                        .unwrap_or(usize::MAX)
                        .min(stack.len() / 2);
                    let first = stack.len() - 2 * pairs;
                    let band = self.projected_ppem()
                        - i64::from(self.delta_base)
                        - 16 * i64::from(op - 0x73);
                    for pair in stack[first..].rchunks_exact(2) {
                        let argument = pair[0];
                        if (argument & 0xf0) >> 4 == band {
                            if let Some(value) = usize::try_from(pair[1])
                                .ok()
                                .and_then(|i| self.cvt.get_mut(i))
                            {
                                let step = (argument & 15) - 8;
                                *value = value.wrapping_add(
                                    (step + i64::from(step >= 0)) << (6 - self.delta_shift),
                                );
                            }
                        }
                    }
                    stack.truncate(first);
                }
                op @ (0x2e | 0x2f) => {
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let zone = usize::from(self.zones[0]);
                    if let Some(point) = self.points[zone].get_mut(usize::from(index)) {
                        let axis = usize::from(self.projection[0] == 0);
                        if self.freedom[axis] != 0 {
                            if op == 0x2f {
                                point.current[axis] = self.rounding.apply(point.current[axis]);
                            }
                            point.touched[axis] = true;
                        }
                        self.reference[..2].fill(index);
                    }
                }
                op @ (0x3e | 0x3f) => {
                    let cvt = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    self.reference[..2].fill(index);
                    let zone = usize::from(self.zones[0]);
                    let Some(point) = self.points[zone].get_mut(usize::from(index)) else {
                        continue;
                    };
                    let Some(&value) = usize::try_from(cvt).ok().and_then(|i| self.cvt.get(i))
                    else {
                        continue;
                    };
                    let freedom = usize::from(self.freedom[0] == 0);
                    let projection = usize::from(self.projection[0] == 0);
                    if zone == 0 {
                        point.original = [0; 2];
                        point.original[freedom] = value;
                        point.current = point.original;
                    }
                    let current = point.current[projection];
                    let distance = if op == 0x3e {
                        value
                    } else {
                        self.rounding.apply(
                            if i128::from(value.abs_diff(current))
                                > i128::from(self.control_value_cutin)
                            {
                                current
                            } else {
                                value
                            },
                        )
                    };
                    if freedom == projection {
                        point.current[freedom] = distance;
                        point.touched[freedom] = true;
                    }
                }
                0x17 => {
                    let count = stack.pop().ok_or(Error::StackUnderflow)?;
                    if count < 0 {
                        return Err(Error::InvalidArgument);
                    }
                    self.loop_count = count.min(65535) as u16;
                }
                0x39 => self.interpolate(stack),
                op @ (0x46 | 0x47) => {
                    let index = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    let zone = &self.points[usize::from(self.zones[2])];
                    *index = usize::try_from(*index)
                        .ok()
                        .and_then(|i| zone.get(i))
                        .map_or(0, |point| {
                            if op == 0x47 {
                                point.original[usize::from(self.dual_projection[0] == 0)]
                            } else {
                                point.current[usize::from(self.projection[0] == 0)]
                            }
                        });
                }
                0x48 => {
                    let coordinate = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let zone = usize::from(self.zones[2]);
                    if let Some(point) = self.points[zone].get_mut(usize::from(index)) {
                        let axis = usize::from(self.projection[0] == 0);
                        if self.freedom[axis] != 0 {
                            point.current[axis] = coordinate;
                            point.touched[axis] = true;
                        }
                        if zone == 0 {
                            point.original = point.current;
                        }
                    }
                }
                0x85 => {
                    let flags = stack.pop().ok_or(Error::StackUnderflow)?;
                    let limit = flags & 255;
                    let ppem = i64::from(self.ppem[0].max(self.ppem[1]));
                    self.scan_control = match limit {
                        0 => false,
                        255 => true,
                        _ if (flags & 0x800 != 0 && ppem > limit)
                            || (flags & 0x1000 != 0 && self.engine.rotated)
                            || (flags & 0x2000 != 0 && self.engine.stretched) =>
                        {
                            false
                        }
                        _ if (flags & 0x100 != 0 && ppem <= limit)
                            || (flags & 0x200 != 0 && self.engine.rotated)
                            || (flags & 0x400 != 0 && self.engine.stretched) =>
                        {
                            true
                        }
                        _ => self.scan_control,
                    };
                }
                0x8d => {
                    let mode = stack.pop().ok_or(Error::StackUnderflow)?;
                    if mode >= 0 {
                        self.scan_type = mode as u16;
                    }
                }
                0x88 => {
                    let selector = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *selector = self.engine.query(*selector);
                }
                0x2c => {
                    let id = u16::try_from(stack.pop().ok_or(Error::StackUnderflow)?)
                        .map_err(|_| Error::InvalidReference)?;
                    let start = code;
                    loop {
                        let size = start.len() - code.len();
                        match next(&mut code)?.opcode {
                            0x2d => {
                                self.functions
                                    .insert(id, alloc::rc::Rc::from(&start[..size]));
                                break;
                            }
                            0x2c | 0x89 => return Err(Error::InvalidOpcode),
                            _ => {}
                        }
                    }
                }
                op @ (0x2a | 0x2b) => {
                    let id = u16::try_from(stack.pop().ok_or(Error::StackUnderflow)?)
                        .map_err(|_| Error::InvalidReference)?;
                    let count = if op == 0x2a {
                        stack.pop().ok_or(Error::StackUnderflow)?
                    } else {
                        1
                    };
                    let body = self
                        .functions
                        .get(&id)
                        .cloned()
                        .ok_or(Error::InvalidReference)?;
                    if depth >= 32 {
                        return Err(Error::StackOverflow);
                    }
                    for _ in 0..count {
                        // ENDF consumes an instruction even for an empty body.
                        *remaining = remaining.checked_sub(1).ok_or(Error::ExecutionLimit)?;
                        self.execute_frame(&body, stack, max_stack, depth + 1, remaining)?;
                    }
                }
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
                op @ (0x42 | 0x44 | 0x70) => {
                    let value = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)?;
                    let table = if op == 0x42 {
                        &mut self.storage
                    } else {
                        &mut self.cvt
                    };
                    if let Some(slot) = usize::try_from(index).ok().and_then(|i| table.get_mut(i)) {
                        *slot = if op == 0x70 {
                            sofel_font::read::cff::ft::mul_fix(value, self.scale)
                        } else {
                            value
                        };
                    }
                }
                op @ (0x43 | 0x45) => {
                    let table = if op == 0x43 { &self.storage } else { &self.cvt };
                    let index = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *index = usize::try_from(*index)
                        .ok()
                        .and_then(|i| table.get(i))
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
    Interpreter::new(0, &[], 0x10000).execute(code, stack, max_stack)
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
    let mut prep = Interpreter::new(2, &[], 0x10000);
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

#[test]
fn adr0064_i1_control_values_scale_font_units_and_preserve_pixel_units() {
    let mut vm = Interpreter::new(0, &[21 * 64 + 63, -21 * 64 - 63], 0x8000);
    let mut stack = alloc::vec::Vec::new();
    vm.execute(&[0xb0, 0, 0x45, 0xb0, 1, 0x45], &mut stack, 8)
        .unwrap();
    assert_eq!(stack, [11, -11]);
    vm.execute(
        &[
            0xb1, 1, 9, 0x70, 0xb0, 1, 0x45, 0xb1, 0, 9, 0x44, 0xb0, 0, 0x45,
        ],
        &mut stack,
        8,
    )
    .unwrap();
    assert_eq!(stack, [11, -11, 5, 9]);
}

#[test]
fn adr0064_i1_font_functions_share_the_operand_stack_and_bound_recursion() {
    let mut vm = Interpreter::new(0, &[], 0x10000);
    let mut stack = alloc::vec::Vec::new();
    // Function 7 adds 45. Its literal byte is also the ENDF opcode.
    vm.execute(&[0xb0, 7, 0x2c, 0xb0, 0x2d, 0x60, 0x2d], &mut stack, 8)
        .unwrap();
    vm.execute(&[0xb2, 1, 3, 7, 0x2a, 0xb0, 7, 0x2b], &mut stack, 8)
        .unwrap();
    assert_eq!(stack, [181]);
    vm.execute(&[0xb0, 8, 0x2c, 0xb0, 8, 0x2b, 0x2d], &mut stack, 8)
        .unwrap();
    assert_eq!(
        vm.execute(&[0xb0, 8, 0x2b], &mut stack, 8),
        Err(Error::StackOverflow)
    );
}

#[test]
fn adr0064_i1_axis_vectors_can_be_set_independently() {
    let mut stack = alloc::vec::Vec::new();
    run(
        &[0x00, 0x03, 0x0c, 0x0d, 0x0e, 0x0d, 0x04, 0x0d],
        &mut stack,
        8,
    )
    .unwrap();
    assert_eq!(stack, [16384, 0, 0, 16384, 16384, 0, 0, 16384]);
}

#[test]
fn adr0064_i1_getinfo_reports_only_requested_interpreter_features() {
    let mut vm = Interpreter::new(0, &[], 0x10000);
    vm.engine = EngineInfo {
        version: 40,
        target: HintTarget::VerticalLcd,
        rotated: true,
        stretched: false,
        variation: true,
        symmetric_smoothing: true,
    };
    for (selector, expected) in [
        (1, 40),
        (2, 256),
        (4, 0),
        (8, 1024),
        (32, 0),
        (64, 8192),
        (256, 32768),
        (1024, 131072),
        (2048, 262144),
        (4096, 0),
    ] {
        let mut stack = alloc::vec![selector];
        vm.execute(&[0x88], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
    vm.engine.version = 35;
    assert_eq!(vm.engine.query(32 | 64 | 4096), 4096);
    vm.engine.target = HintTarget::Mono;
    assert_eq!(vm.engine.query(32 | 64 | 4096), 0);
}

#[test]
fn adr0064_i1_projected_ppem_and_point_size_use_driver_metrics() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.ppem = [12, 18];
    vm.point_size = 9 * 64;
    let mut stack = alloc::vec::Vec::new();
    vm.execute(&[0x4b, 0x00, 0x4b, 0x4c], &mut stack, 4)
        .unwrap();
    vm.engine.version = 35;
    vm.execute(&[0x4c], &mut stack, 4).unwrap();
    assert_eq!(stack, [12, 18, 576, 18]);
}

#[test]
fn adr0064_i1_scan_control_prioritizes_disable_and_remembers_unmatched_settings() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.ppem = [12, 18];
    vm.engine.rotated = true;
    for (selector, expected) in [
        (0x111, false),
        (0x112, true),
        (1, true),
        (0x1201, false),
        (0x20ff, true),
        (0x200, false),
    ] {
        vm.execute(&[0x85], &mut alloc::vec![selector], 1).unwrap();
        assert_eq!(vm.scan_control, expected);
    }
    for value in [0x10005, -1] {
        vm.execute(&[0x8d], &mut alloc::vec![value], 1).unwrap();
        assert_eq!(vm.scan_type, 5);
    }
}

#[test]
fn adr0064_i1_zone_selection_and_cutins_preserve_instruction_units() {
    let mut vm = Interpreter::new(0, &[], 32768);
    let mut stack = alloc::vec::Vec::new();
    vm.execute(
        &[0xb0, 0, 0x16, 0xb0, 1, 0x14, 0xb0, 2, 0x16],
        &mut stack,
        1,
    )
    .unwrap();
    assert_eq!(vm.zones, [0, 1, 0]);
    vm.execute(
        &[0xb0, 17, 0x1d, 0xb0, 19, 0x1e, 0xb0, 21, 0x1f],
        &mut stack,
        1,
    )
    .unwrap();
    assert_eq!(
        (
            vm.control_value_cutin,
            vm.single_width_cutin,
            vm.single_width
        ),
        (17, 19, 11)
    );
}

#[test]
fn adr0064_i1_delta_control_values_obey_ppem_bands_and_signed_steps() {
    for (op, base) in [(0x73, 10), (0x74, 26), (0x75, 42)] {
        for (ppem, expected) in [(base - 1, 0), (base, 4), (base + 15, -4), (base + 16, 0)] {
            let mut vm = Interpreter::new(0, &[0], 65536);
            vm.ppem = [ppem; 2];
            let mut stack = alloc::vec::Vec::new();
            vm.execute(
                &[0xb0, 10, 0x5e, 0xb0, 4, 0x5f, 0xb4, 8, 0, 0xf7, 0, 2, op],
                &mut stack,
                5,
            )
            .unwrap();
            assert!(stack.is_empty());
            assert_eq!(vm.cvt, [expected]);
            assert!(vm.execute(&[0xb0, 7, 0x5f], &mut stack, 5).is_err());
        }
    }
}

#[test]
fn adr0064_i1_miap_sets_original_twilight_coordinates_and_reference_points() {
    let mut vm = Interpreter::new(0, &[70 * 64], 65536);
    vm.points[0].resize(2, HintPoint::default());
    vm.execute(
        &[0xb0, 0, 0x16, 0xb1, 1, 0, 0x3f],
        &mut alloc::vec::Vec::new(),
        2,
    )
    .unwrap();
    assert_eq!(vm.points[0][1].original, [70, 0]);
    assert_eq!(vm.points[0][1].current, [64, 0]);
    assert_eq!(vm.points[0][1].touched, [true, false]);
    assert_eq!(vm.reference, [1, 1, 0]);
}

#[test]
fn adr0064_i1_reference_point_registers_take_unsigned_short_indices() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.execute(&[0x10, 0x11, 0x12], &mut alloc::vec![-1, 7, 9], 3)
        .unwrap();
    assert_eq!(vm.reference, [9, 7, 65535]);
}

#[test]
fn adr0064_i1_ip_interpolates_all_selected_twilight_points() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.zones = [0; 3];
    vm.reference = [0, 0, 1];
    for (original, current) in [(0, 10), (100, 210), (25, 25), (75, 75)] {
        vm.points[0].push(HintPoint {
            original: [original, 0],
            current: [current, 0],
            ..Default::default()
        });
    }
    vm.execute(&[0xb2, 2, 3, 2, 0x17, 0x39], &mut alloc::vec::Vec::new(), 3)
        .unwrap();
    assert_eq!(vm.points[0][2].current, [60, 0]);
    assert_eq!(vm.points[0][3].current, [160, 0]);
    assert_eq!(vm.loop_count, 1);
}

#[test]
fn adr0064_i1_gc_reads_original_and_current_scfs_updates_twilight() {
    for zone in [0, 1] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.zones[2] = zone;
        vm.points[usize::from(zone)].push(HintPoint {
            original: [13, 7],
            current: [25, 8],
            ..Default::default()
        });
        let mut stack = alloc::vec::Vec::new();
        vm.execute(
            &[
                0xb0, 0, 0x47, 0xb0, 0, 0x46, 0xb1, 0, 11, 0x48, 0xb0, 0, 0x47, 0xb0, 0, 0x46,
            ],
            &mut stack,
            4,
        )
        .unwrap();
        assert_eq!(stack, [13, 25, if zone == 0 { 11 } else { 13 }, 11]);
    }
}

#[test]
fn adr0064_i1_mdap_rounds_only_when_requested_and_sets_references() {
    for (opcode, expected) in [(0x2e, 37), (0x2f, 64)] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.points[1].resize(
            2,
            HintPoint {
                current: [37, 9],
                ..Default::default()
            },
        );
        vm.execute(&[0xb0, 1, opcode], &mut alloc::vec::Vec::new(), 1)
            .unwrap();
        assert_eq!(vm.points[1][1].current, [expected, 9]);
        assert_eq!(vm.points[1][1].touched, [true, false]);
        assert_eq!(vm.reference, [1, 1, 0]);
    }
}
