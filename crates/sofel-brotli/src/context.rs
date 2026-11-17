//! Literal contexts and move-to-front context maps (RFC 7932, sections 7 and 8).
use super::{Bits, prefix::Prefix};
use alloc::{vec, vec::Vec};

pub(super) fn map(bits: &mut Bits<'_>, size: usize, trees: usize) -> Option<Vec<u8>> {
    if trees == 1 {
        return Some(vec![0; size]);
    }
    let runs = if bits.take(1)? == 0 {
        0
    } else {
        bits.take(4)?.checked_add(1)?
    };
    let codes = Prefix::read(bits, trees.checked_add(runs)?)?;
    let mut map = Vec::with_capacity(size);
    while map.len() < size {
        let s = codes.symbol(bits)?;
        if s > 0 && s <= runs {
            let n = (1usize << s).checked_add(bits.take(u32::try_from(s).ok()?)?)?;
            let end = map.len().checked_add(n)?;
            if end > size {
                return None;
            }
            map.resize(end, 0);
        } else {
            map.push(u8::try_from(s.saturating_sub(runs)).ok()?);
        }
    }
    if bits.take(1)? != 0 {
        let mut order: [u8; 256] = core::array::from_fn(|n| u8::try_from(n).unwrap_or_default());
        for entry in &mut map {
            let head = order.get_mut(..=usize::from(*entry))?;
            *entry = *head.last()?;
            head.rotate_right(1);
        }
    }
    Some(map)
}

fn signed(b: u8) -> u8 {
    match b {
        0 => 0,
        1..=15 => 1,
        16..=63 => 2,
        64..=127 => 3,
        128..=191 => 4,
        192..=239 => 5,
        240..=254 => 6,
        255 => 7,
    }
}
fn utf8(b: u8) -> u8 {
    match b {
        9 | 10 | 13 => 4,
        0..=31 | 127 => 0,
        32 => 8,
        34 | 39 => 16,
        37 => 20,
        40 | 60 | 91 | 123 => 24,
        41 | 62 | 93 | 125 => 28,
        44 | 58 | 59 => 32,
        46 => 36,
        61 => 40,
        b'0'..=b'9' => 44,
        b'A' | b'E' | b'I' | b'O' | b'U' => 48,
        b'A'..=b'Z' => 52,
        b'a' | b'e' | b'i' | b'o' | b'u' => 56,
        b'a'..=b'z' => 60,
        128..=191 => b & 1,
        192..=255 => 2 | (b & 1),
        _ => 12,
    }
}
pub(super) fn literal(mode: usize, last: u8, previous: u8) -> usize {
    usize::from(match mode {
        0 => last & 63,
        1 => last >> 2,
        2 => {
            utf8(last)
                | match previous {
                    0..=32 | 127..=207 => 0,
                    b'A'..=b'Z' | b'0'..=b'9' | 208..=255 => 2,
                    b'a'..=b'z' => 3,
                    _ => 1,
                }
        }
        _ => signed(last) << 3 | signed(previous),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0065_i2_context_map_zero_run_bound() {
        // RLEMAX=1, simple prefix tree with its only symbol 1, repeat two zeros.
        let encoded = [0x21, 0x02];
        let read = |size| {
            map(
                &mut Bits {
                    data: &encoded,
                    bit: 0,
                },
                size,
                2,
            )
        };
        assert_eq!(read(2), Some(vec![0, 0]));
        assert_eq!(read(1), None);
    }
}
