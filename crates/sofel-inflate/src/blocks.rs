use super::*;
use alloc::vec;

// Counts and prefixes below are bounded by DEFLATE's 15-bit code length
// and alphabets of at most 288 symbols.
struct Bits<'a> {
    rest: &'a [u8],
    pending: u32,
    available: u8,
}
impl<'a> Bits<'a> {
    fn take(&mut self, n: u8) -> Result<u32> {
        while self.available < n {
            let (&byte, rest) = self.rest.split_first().ok_or(Error::Invalid)?;
            self.rest = rest;
            self.pending |= u32::from(byte) << self.available;
            self.available = self.available.saturating_add(8);
        }
        let result = self.pending & (1u32 << n).saturating_sub(1);
        self.pending >>= n;
        self.available = self.available.saturating_sub(n);
        Ok(result)
    }
    fn aligned(&mut self, n: usize) -> Result<&'a [u8]> {
        self.available = 0;
        self.pending = 0;
        let (data, rest) = self.rest.split_at_checked(n).ok_or(Error::Invalid)?;
        self.rest = rest;
        Ok(data)
    }
}

struct Tree {
    counts: [u16; 16],
    symbols: Vec<u16>,
}
impl Tree {
    fn new(lengths: &[u8], require_complete: bool) -> Result<Self> {
        let mut counts = [0u16; 16];
        let mut sorted = Vec::new();
        for (symbol, &length) in lengths.iter().enumerate().filter(|(_, n)| **n != 0) {
            let count = counts.get_mut(usize::from(length)).ok_or(Error::Invalid)?;
            *count = count.saturating_add(1);
            sorted.push((length, u16::try_from(symbol).map_err(|_| Error::Invalid)?));
        }
        let mut slots = 1u32;
        for &count in counts.iter().skip(1) {
            slots = (slots << 1)
                .checked_sub(u32::from(count))
                .ok_or(Error::Invalid)?;
        }
        if slots != 0
            && (require_complete
                || (!sorted.is_empty() && sorted.as_slice().first().is_none_or(|s| s.0 != 1))
                || sorted.len() > 1)
        {
            return Err(Error::Invalid);
        }
        sorted.sort_unstable();
        Ok(Self {
            counts,
            symbols: sorted.into_iter().map(|s| s.1).collect(),
        })
    }
    fn symbol(&self, bits: &mut Bits<'_>) -> Result<u16> {
        let mut prefix = 0u32;
        let mut first = 0u32;
        let mut index = 0u32;
        for &count in self.counts.iter().skip(1) {
            prefix = (prefix << 1) | bits.take(1)?;
            let delta = prefix.checked_sub(first).ok_or(Error::Invalid)?;
            if delta < u32::from(count) {
                return self
                    .symbols
                    .get(index.saturating_add(delta) as usize)
                    .copied()
                    .ok_or(Error::Invalid);
            }
            first = first.saturating_add(u32::from(count)) << 1;
            index = index.saturating_add(u32::from(count));
        }
        Err(Error::Invalid)
    }
}
fn dynamic(bits: &mut Bits<'_>) -> Result<(Tree, Tree)> {
    let literals = (257u32.saturating_add(bits.take(5)?)) as usize;
    let distances = (1u32.saturating_add(bits.take(5)?)) as usize;
    let codes = (4u32.saturating_add(bits.take(4)?)) as usize;
    if literals > 286 {
        return Err(Error::Invalid);
    }
    let mut lengths = [0u8; 19];
    for i in [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ]
    .into_iter()
    .take(codes)
    {
        *lengths.get_mut(i).ok_or(Error::Invalid)? =
            u8::try_from(bits.take(3)?).map_err(|_| Error::Invalid)?;
    }
    let order = Tree::new(&lengths, true)?;
    let total = literals.saturating_add(distances);
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let code = order.symbol(bits)?;
        let (value, n) = match code {
            0..=15 => (u8::try_from(code).map_err(|_| Error::Invalid)?, 1),
            16 => (
                *lengths.last().ok_or(Error::Invalid)?,
                3u32.saturating_add(bits.take(2)?),
            ),
            17 => (0, 3u32.saturating_add(bits.take(3)?)),
            18 => (0, 11u32.saturating_add(bits.take(7)?)),
            _ => return Err(Error::Invalid),
        };
        let end = lengths.len().saturating_add(n as usize);
        if end > total {
            return Err(Error::Invalid);
        }
        lengths.resize(end, value);
    }
    if lengths.get(256) == Some(&0) {
        return Err(Error::Invalid);
    }
    let (a, b) = lengths.split_at_checked(literals).ok_or(Error::Invalid)?;
    Ok((Tree::new(a, false)?, Tree::new(b, false)?))
}

fn compressed(
    bits: &mut Bits<'_>,
    trees: &(Tree, Tree),
    out: &mut Vec<u8>,
    limit: usize,
    start: usize,
    window: usize,
) -> Result<()> {
    const LENGTHS: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    loop {
        let code = trees.0.symbol(bits)?;
        match code {
            0..=255 => {
                if out.len() >= limit {
                    return Err(Error::Limit);
                }
                out.push(u8::try_from(code).map_err(|_| Error::Invalid)?);
            }
            256 => return Ok(()),
            257..=285 => {
                let index = code.saturating_sub(257);
                let base = *LENGTHS.get(usize::from(index)).ok_or(Error::Invalid)?;
                let extra = if index == 28 {
                    0
                } else {
                    index.saturating_sub(4) / 4
                };
                let length = usize::from(base).saturating_add(
                    bits.take(u8::try_from(extra).map_err(|_| Error::Invalid)?)? as usize,
                );
                let code = trees.1.symbol(bits)?;
                if code >= 30 {
                    return Err(Error::Invalid);
                }
                let extra = (code / 2).saturating_sub(1);
                let base = if code < 4 {
                    u32::from(code) + 1
                } else {
                    ((2 + u32::from(code % 2)) << extra).saturating_add(1)
                };
                let distance = base
                    .saturating_add(bits.take(u8::try_from(extra).map_err(|_| Error::Invalid)?)?)
                    as usize;
                if distance > window || distance > out.len().saturating_sub(start) {
                    return Err(Error::Invalid);
                }
                if length > limit.saturating_sub(out.len()) {
                    return Err(Error::Limit);
                }
                for _ in 0..length {
                    let byte = *out
                        .get(out.len().saturating_sub(distance))
                        .ok_or(Error::Invalid)?;
                    out.push(byte);
                }
            }
            _ => return Err(Error::Invalid),
        }
    }
}
pub(super) fn decode(
    input: &[u8],
    out: &mut Vec<u8>,
    limit: usize,
    window: usize,
) -> Result<usize> {
    let start = out.len();
    let mut bits = Bits {
        rest: input,
        pending: 0,
        available: 0,
    };
    let mut fixed = None;
    loop {
        let last = bits.take(1)? != 0;
        match bits.take(2)? {
            0 => {
                let header = bits.aligned(4)?.first_chunk::<4>().ok_or(Error::Invalid)?;
                let &[a, b, c, d] = header;
                let len = u16::from_le_bytes([a, b]);
                if len != !u16::from_le_bytes([c, d]) {
                    return Err(Error::Invalid);
                }
                if usize::from(len) > limit.saturating_sub(out.len()) {
                    return Err(Error::Limit);
                }
                out.extend(bits.aligned(usize::from(len))?);
            }
            1 => {
                if fixed.is_none() {
                    let mut lengths = vec![8; 144];
                    lengths.resize(256, 9);
                    lengths.resize(280, 7);
                    lengths.resize(288, 8);
                    fixed = Some((Tree::new(&lengths, true)?, Tree::new(&[5; 32], true)?));
                }
                compressed(
                    &mut bits,
                    fixed.as_ref().ok_or(Error::Invalid)?,
                    out,
                    limit,
                    start,
                    window,
                )?;
            }
            2 => {
                let trees = dynamic(&mut bits)?;
                compressed(&mut bits, &trees, out, limit, start, window)?;
            }
            _ => return Err(Error::Invalid),
        }
        if last {
            return Ok(input.len().saturating_sub(bits.rest.len()));
        }
    }
}
