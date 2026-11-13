//! Literal insertions and backwards copies within a compressed meta-block.
use super::{Bits, prefix::Prefix};
use alloc::vec::Vec;

const INSERT: [u32; 24] = [
    0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 10, 12, 14, 24,
];
const COPY: [u32; 24] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 10, 24,
];
const GROUPS: [(usize, usize); 11] = [
    (0, 0),
    (0, 8),
    (0, 0),
    (0, 8),
    (8, 0),
    (8, 8),
    (0, 16),
    (16, 0),
    (8, 16),
    (16, 8),
    (16, 16),
];

fn length(bits: &mut Bits<'_>, widths: &[u32], index: usize, start: usize) -> Option<usize> {
    let base = widths
        .get(..index)?
        .iter()
        .try_fold(start, |v, &n| v.checked_add(1usize << n))?;
    base.checked_add(bits.take(*widths.get(index)?)?)
}

const BLOCK_LENGTH: [u32; 26] = [
    2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 7, 8, 9, 10, 11, 12, 13, 24,
];

fn type_count(bits: &mut Bits<'_>) -> Option<usize> {
    if bits.take(1)? == 0 {
        return Some(1);
    }
    let width = u32::try_from(bits.take(3)?).ok()?;
    (1usize << width)
        .checked_add(1)?
        .checked_add(bits.take(width)?)
}

struct BlockTypes {
    count: usize,
    current: usize,
    previous: usize,
    remaining: usize,
    codes: Option<(Prefix, Prefix)>,
}

impl BlockTypes {
    fn read(bits: &mut Bits<'_>) -> Option<Self> {
        let count = type_count(bits)?;
        let mut types = Self {
            count,
            current: 0,
            previous: 1,
            remaining: 0,
            codes: None,
        };
        if count > 1 {
            let switches = Prefix::read(bits, count.checked_add(2)?)?;
            let lengths = Prefix::read(bits, 26)?;
            let index = lengths.symbol(bits)?;
            types.remaining = length(bits, &BLOCK_LENGTH, index, 1)?;
            types.codes = Some((switches, lengths));
        }
        Some(types)
    }

    fn next(&mut self, bits: &mut Bits<'_>) -> Option<usize> {
        let Some((switches, lengths)) = &self.codes else {
            return Some(0);
        };
        if self.remaining == 0 {
            let next = match switches.symbol(bits)? {
                0 => self.previous,
                1 => self.current.checked_add(1)?.checked_rem(self.count)?,
                code => code.checked_sub(2)?,
            };
            self.previous = self.current;
            self.current = next;
            let index = lengths.symbol(bits)?;
            self.remaining = length(bits, &BLOCK_LENGTH, index, 1)?;
        }
        self.remaining = self.remaining.checked_sub(1)?;
        Some(self.current)
    }
}

pub(super) fn decode(
    bits: &mut Bits<'_>,
    out: &mut Vec<u8>,
    end: usize,
    recent: &mut [usize; 4],
    window: usize,
) -> Option<()> {
    let mut literal_types = BlockTypes::read(bits)?;
    let mut command_types = BlockTypes::read(bits)?;
    let mut distance_types = BlockTypes::read(bits)?;
    let postfix = bits.take(2)?;
    let direct = bits.take(4)? << postfix;
    let modes = (0..literal_types.count)
        .map(|_| bits.take(2))
        .collect::<Option<Vec<_>>>()?;
    let literal_count = type_count(bits)?;
    let literal_map =
        super::context::map(bits, literal_types.count.checked_mul(64)?, literal_count)?;
    let distance_count = type_count(bits)?;
    let distance_map =
        super::context::map(bits, distance_types.count.checked_mul(4)?, distance_count)?;
    let literals = (0..literal_count)
        .map(|_| Prefix::read(bits, 256))
        .collect::<Option<Vec<_>>>()?;
    let commands = (0..command_types.count)
        .map(|_| Prefix::read(bits, 704))
        .collect::<Option<Vec<_>>>()?;
    let direct_end = direct.checked_add(16)?;
    let distance_alphabet = direct_end.checked_add(48usize << postfix)?;
    let distances = (0..distance_count)
        .map(|_| Prefix::read(bits, distance_alphabet))
        .collect::<Option<Vec<_>>>()?;
    while out.len() < end {
        let cmd = commands.get(command_types.next(bits)?)?.symbol(bits)?;
        let &(igroup, cgroup) = GROUPS.get(cmd >> 6)?;
        let insert = length(bits, &INSERT, igroup.checked_add(cmd >> 3 & 7)?, 0)?;
        let copy = length(bits, &COPY, cgroup.checked_add(cmd & 7)?, 2)?;
        if out.len().checked_add(insert)? > end {
            return None;
        }
        for _ in 0..insert {
            let ty = literal_types.next(bits)?;
            let mut history = out.iter().rev().copied();
            let id = super::context::literal(
                *modes.get(ty)?,
                history.next().unwrap_or(0),
                history.next().unwrap_or(0),
            );
            let tree = *literal_map.get((ty << 6) | id)?;
            out.push(u8::try_from(literals.get(usize::from(tree))?.symbol(bits)?).ok()?);
        }
        if out.len() == end {
            break;
        }
        let code = if cmd < 128 {
            0
        } else {
            let ty = distance_types.next(bits)?;
            let id = copy.saturating_sub(2).min(3);
            let tree = *distance_map.get((ty << 2) | id)?;
            distances.get(usize::from(tree))?.symbol(bits)?
        };
        let distance = match code {
            0..=3 => *recent.get(code)?,
            4..=15 => {
                let k = code.checked_sub(4)?;
                let base = *recent.get(k / 6)?;
                let delta = ((k % 6) / 2).checked_add(1)?;
                if k % 2 == 0 {
                    base.checked_sub(delta)?
                } else {
                    base.checked_add(delta)?
                }
            }
            _ if code < direct_end => code.checked_sub(15)?,
            _ => {
                let v = code.checked_sub(direct_end)?;
                let group = v >> postfix;
                let n = u32::try_from((group >> 1).checked_add(1)?).ok()?;
                let start = 2usize
                    .checked_add(group & 1)?
                    .checked_shl(n)?
                    .checked_sub(4)?;
                ((start.checked_add(bits.take(n)?)?) << postfix)
                    .checked_add(v & (1usize << postfix).checked_sub(1)?)?
                    .checked_add(direct.checked_add(1)?)?
            }
        };
        let max_back = window.min(out.len());
        if distance > max_back {
            let id = distance.checked_sub(max_back)?.checked_sub(1)?;
            super::dictionary::append(out, copy, id, end)?;
            continue;
        }
        if distance == 0 || out.len().checked_add(copy)? > end {
            return None;
        }
        if code != 0 {
            recent.rotate_right(1);
            recent[0] = distance;
        }
        for _ in 0..copy {
            out.push(*out.get(out.len().checked_sub(distance)?)?);
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0065_i1_block_switch_reuses_previous_type() {
        let mut state = BlockTypes {
            count: 3,
            current: 2,
            previous: 1,
            remaining: 1,
            codes: Some((Prefix::Single(0), Prefix::Single(0))),
        };
        let mut bits = Bits { data: &[0], bit: 0 };
        assert_eq!(state.next(&mut bits), Some(2));
        assert_eq!(state.next(&mut bits), Some(1));
        assert_eq!(state.next(&mut bits), Some(2));
        assert_eq!(bits.bit, 4);
    }
}
