//! Bounded Brotli bitstream decoder.
#![no_std]
extern crate alloc;
mod block;
mod context;
mod dictionary;
mod prefix;
use alloc::vec::Vec;

struct Bits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl Bits<'_> {
    fn take(&mut self, count: u32) -> Option<usize> {
        let mut value = 0;
        for shift in 0..count {
            let byte = self.data.get(self.bit / 8)?;
            value |= usize::from((byte >> (self.bit % 8)) & 1) << shift;
            self.bit = self.bit.checked_add(1)?;
        }
        Some(value)
    }

    fn align(&mut self) -> Option<()> {
        let padding = self.bit.wrapping_neg() & 7;
        (self.take(u32::try_from(padding).ok()?)? == 0).then_some(())
    }
}

/// Decode a stream whose uncompressed length is known.
pub fn decompress(data: &[u8], expected: usize) -> Option<Vec<u8>> {
    let mut bits = Bits { data, bit: 0 };
    let window_bits = if bits.take(1)? == 0 {
        16
    } else {
        match bits.take(3)? {
            0 => match bits.take(3)? {
                0 => 17,
                1 => return None,
                n => n.checked_add(8)?,
            },
            n => n.checked_add(17)?,
        }
    };
    let window = (1usize << window_bits).checked_sub(16)?;
    let mut recent = [4, 11, 15, 16];
    let mut out = Vec::new();
    loop {
        let last = bits.take(1)? != 0;
        if last && bits.take(1)? != 0 {
            bits.align()?;
            return (out.len() == expected && bits.bit / 8 == data.len()).then_some(out);
        }
        let nibbles = u32::try_from(bits.take(2)?).ok()?.checked_add(4)?;
        if nibbles == 7 {
            return None;
        }
        let encoded = bits.take(nibbles << 2)?;
        if nibbles > 4 && encoded >> (nibbles.saturating_sub(1) << 2) == 0 {
            return None;
        }
        let length = encoded.checked_add(1)?;
        let target = out.len().checked_add(length)?;
        if target > expected {
            return None;
        }
        if last || bits.take(1)? == 0 {
            block::decode(&mut bits, &mut out, target, &mut recent, window)?;
            if last {
                bits.align()?;
                return (out.len() == expected && bits.bit / 8 == data.len()).then_some(out);
            }
            continue;
        }
        bits.align()?;
        let start = bits.bit / 8;
        let end = start.checked_add(length)?;
        out.extend_from_slice(data.get(start..end)?);
        bits.bit = end.checked_mul(8)?;
    }
}

#[cfg(test)]
mod compressed_block_tests {
    #[test]
    fn adr0065_i1_brotli_compressed_repeated_literal() {
        // Brotli 1.2.1, quality 4, lgwin 16.
        let compressed = [226, 15, 0, 128, 68, 80, 36, 22, 72, 6];
        assert_eq!(
            super::decompress(&compressed, 128).as_deref(),
            Some([b'A'; 128].as_slice())
        );
        assert!(super::decompress(&compressed, 127).is_none());
    }
}
