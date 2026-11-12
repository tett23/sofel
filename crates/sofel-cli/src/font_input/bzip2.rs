//! bzip2 block decoding, including the randomised blocks used by early encoders.
use super::{Diagnostic, diagnostic};

mod random;

type Result<T> = core::result::Result<T, Diagnostic>;

struct Bits<'a> {
    data: &'a [u8],
    byte: usize,
    bit: u8,
}

impl Bits<'_> {
    fn error(&self, message: &'static str) -> Diagnostic {
        diagnostic(self.byte, message)
    }

    fn read(&mut self, count: usize) -> Result<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            let byte = self
                .data
                .get(self.byte)
                .ok_or_else(|| self.error("truncated bzip2 stream"))?;
            value = (value << 1) | u32::from((byte >> (7u8.wrapping_sub(self.bit))) & 1);
            self.bit = self.bit.wrapping_add(1) % 8;
            if self.bit == 0 {
                self.byte = self.byte.saturating_add(1);
            }
        }
        Ok(value)
    }
}

struct Tree {
    counts: [u32; 21],
    symbols: Vec<usize>,
}

impl Tree {
    // Lengths are checked against 1..=20; there are at most 258 symbols.
    #[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
    fn read(bits: &mut Bits<'_>, alphabet_size: usize) -> Result<Self> {
        let mut length = i32::try_from(bits.read(5)?).unwrap_or_default();
        let mut lengths = Vec::with_capacity(alphabet_size);
        let mut counts = [0u32; 21];
        for _ in 0..alphabet_size {
            loop {
                if !(1..=20).contains(&length) {
                    return Err(bits.error("invalid bzip2 code length"));
                }
                if bits.read(1)? == 0 {
                    break;
                }
                length += if bits.read(1)? == 0 { 1 } else { -1 };
            }
            let len =
                usize::try_from(length).map_err(|_| bits.error("invalid bzip2 code length"))?;
            counts[len] += 1;
            lengths.push(len);
        }
        let mut free = 1u32;
        for &count in &counts[1..] {
            free = (free << 1)
                .checked_sub(count)
                .ok_or_else(|| bits.error("oversubscribed bzip2 code"))?;
        }
        let mut symbols: Vec<_> = (0..alphabet_size).collect();
        symbols.sort_by_key(|&symbol| lengths[symbol]);
        Ok(Self { counts, symbols })
    }

    // Canonical codes have at most 20 bits; base counts at most 258 symbols.
    #[allow(clippy::arithmetic_side_effects)]
    fn symbol(&self, bits: &mut Bits<'_>) -> Result<usize> {
        let (mut code, mut first, mut base) = (0, 0, 0usize);
        for &count in &self.counts[1..] {
            code = (code << 1) | bits.read(1)?;
            if code < first + count {
                let index = base + (code - first) as usize;
                return self
                    .symbols
                    .get(index)
                    .copied()
                    .ok_or_else(|| bits.error("invalid bzip2 symbol"));
            }
            first = (first + count) << 1;
            base += count as usize;
        }
        Err(bits.error("invalid bzip2 code"))
    }
}

// Selector ranks are checked before use; symbol ranks come from the constructed
// alphabet. A successful run fits the declared block (at most 900,000 bytes).
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
fn last_column(bits: &mut Bits<'_>, block_size: usize) -> Result<Vec<u8>> {
    let groups = bits.read(16)?;
    let mut alphabet = Vec::new();
    for group in 0u8..16 {
        if groups & (0x8000 >> group) != 0 {
            let mask = bits.read(16)?;
            alphabet.extend(
                (0u8..16)
                    .filter(|index| mask & (0x8000 >> index) != 0)
                    .map(|index| group * 16 + index),
            );
        }
    }
    let tree_count = bits.read(3)? as usize;
    let selector_count = bits.read(15)? as usize;
    if alphabet.is_empty() || !(2..=6).contains(&tree_count) || selector_count == 0 {
        return Err(bits.error("invalid bzip2 Huffman groups"));
    }
    let mut order: Vec<usize> = (0..tree_count).collect();
    let mut selectors = Vec::with_capacity(selector_count);
    for _ in 0..selector_count {
        let mut index = 0;
        while bits.read(1)? != 0 {
            index += 1;
            if index >= tree_count {
                return Err(bits.error("invalid bzip2 selector"));
            }
        }
        let tree = order.remove(index);
        order.insert(0, tree);
        selectors.push(tree);
    }
    let end = alphabet.len() + 1;
    let trees = (0..tree_count)
        .map(|_| Tree::read(bits, end + 1))
        .collect::<Result<Vec<_>>>()?;
    let mut column = Vec::new();
    let mut weight = 1usize;
    for selector in selectors {
        for _ in 0..50 {
            let symbol = trees[selector].symbol(bits)?;
            if symbol == end {
                return Ok(column);
            }
            if symbol < 2 {
                let run = (symbol + 1) * weight;
                if run > block_size - column.len() {
                    return Err(bits.error("bzip2 run exceeds block size"));
                }
                column.resize(column.len() + run, alphabet[0]);
                weight *= 2;
            } else {
                if column.len() == block_size {
                    return Err(bits.error("bzip2 block exceeds declared size"));
                }
                let byte = alphabet.remove(symbol - 1);
                alphabet.insert(0, byte);
                column.push(byte);
                weight = 1;
            }
        }
    }
    Err(bits.error("missing bzip2 end-of-block symbol"))
}

// Cumulative byte counts partition the column; next is a permutation of its
// row indices. Output growth is checked against the caller's bound.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
fn restore(
    column: &[u8],
    origin: usize,
    randomised: bool,
    output: &mut Vec<u8>,
    limit: usize,
    bits: &Bits<'_>,
) -> Result<u32> {
    if origin >= column.len() {
        return Err(bits.error("invalid bzip2 block origin"));
    }
    let mut starts = [0usize; 256];
    for &b in column {
        starts[usize::from(b)] += 1;
    }
    let mut total = 0;
    for count in &mut starts {
        let frequency = *count;
        *count = total;
        total += frequency;
    }
    let mut next = vec![0usize; column.len()];
    for (row, &byte) in column.iter().enumerate() {
        let start = &mut starts[usize::from(byte)];
        next[*start] = row;
        *start += 1;
    }
    let mut row = origin;
    let (mut previous, mut repeats) = (0u8, 0);
    let (mut rand_index, mut countdown) = (0usize, 0u16);
    let mut crc = u32::MAX;
    for _ in column {
        row = next[row];
        let mut byte = column[row];
        if randomised {
            if countdown == 0 {
                countdown = random::NUMBERS[rand_index];
                rand_index = (rand_index + 1) % random::NUMBERS.len();
            }
            countdown -= 1;
            byte ^= u8::from(countdown == 1);
        }
        let (value, count) = if repeats == 4 {
            repeats = 0;
            (previous, usize::from(byte))
        } else {
            repeats = if byte == previous { repeats + 1 } else { 1 };
            previous = byte;
            (byte, 1)
        };
        if count > limit - output.len() {
            return Err(bits.error("bzip2 output exceeds the font size limit"));
        }
        output.resize(output.len() + count, value);
        for _ in 0..count {
            crc = (crc << 8) ^ CRC_TABLE[((crc >> 24) as u8 ^ value) as usize];
        }
    }
    if repeats == 4 {
        return Err(bits.error("truncated bzip2 run"));
    }
    Ok(!crc)
}

// The constant loop visits exactly the 256 byte values.
#[allow(clippy::cast_possible_truncation, clippy::indexing_slicing)]
const CRC_TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = (i as u32) << 24;
        let mut n = 0;
        while n < 8 {
            crc = (crc << 1) ^ if crc >> 31 == 1 { 0x04c11db7 } else { 0 };
            n += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

pub(super) fn decode(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut bits = Bits {
        data,
        byte: 0,
        bit: 0,
    };
    let mut output = Vec::new();
    loop {
        if bits.read(24)? != 0x425a68 {
            return Err(bits.error("invalid bzip2 member header"));
        }
        let digit = bits.read(8)?;
        if !(49..=57).contains(&digit) {
            return Err(bits.error("invalid bzip2 block size"));
        }
        let block_size = (digit.wrapping_sub(48) as usize).saturating_mul(100_000);
        let mut combined = 0u32;
        loop {
            let marker = (u64::from(bits.read(24)?) << 24) | u64::from(bits.read(24)?);
            let checksum_offset = bits.byte;
            let expected = bits.read(32)?;
            match marker {
                0x1772_4538_5090 => {
                    if combined != expected {
                        return Err(diagnostic(
                            checksum_offset,
                            "bzip2 member checksum mismatch",
                        ));
                    }
                    break;
                }
                0x3141_5926_5359 => {
                    let randomised = bits.read(1)? != 0;
                    let origin = bits.read(24)? as usize;
                    let column = last_column(&mut bits, block_size)?;
                    let crc = restore(&column, origin, randomised, &mut output, limit, &bits)?;
                    if crc != expected {
                        return Err(diagnostic(checksum_offset, "bzip2 block checksum mismatch"));
                    }
                    combined = combined.rotate_left(1) ^ crc;
                }
                _ => return Err(bits.error("invalid bzip2 block marker")),
            }
        }
        let padding = usize::from(8u8.wrapping_sub(bits.bit) % 8);
        bits.read(padding)?;
        if bits.byte == data.len() {
            return Ok(output);
        }
    }
}
