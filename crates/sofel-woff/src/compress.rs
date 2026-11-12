//! RFC 1950/1951 fixed Huffman blocks with distance-one matches, and
//! RFC 7932 uncompressed meta-blocks. Compression ratio is not a target
//! of the first encoder (ADR 0070).

use alloc::{vec, vec::Vec};

#[derive(Default)]
struct Bits {
    bytes: Vec<u8>,
    pending: u8,
    used: u8,
}
impl Bits {
    fn write(&mut self, mut value: u32, count: u32) {
        for _ in 0..count {
            self.pending |= u8::from(value & 1 != 0) << self.used;
            value >>= 1;
            self.used = self.used.saturating_add(1);
            if self.used == 8 {
                self.align();
            }
        }
    }
    fn align(&mut self) {
        if self.used != 0 {
            self.bytes.push(self.pending);
            self.pending = 0;
            self.used = 0;
        }
    }
    // The fixed-code ranges below are bounded by RFC 1951.
    #[allow(clippy::arithmetic_side_effects)]
    fn symbol(&mut self, symbol: u16) {
        let (code, bits) = match symbol {
            0..=143 => (0x30 + symbol, 8),
            144..=255 => (0x190 + (symbol - 144), 9),
            256..=279 => (symbol - 256, 7),
            _ => (0xc0 + (symbol - 280), 8),
        };
        self.write(u32::from(code.reverse_bits()) >> (16 - bits), bits);
    }
    fn repeat(&mut self, len: u16) {
        // RFC 1951 section 3.2.5, literal/length codes 257..285.
        const BASE: [u16; 29] = [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99,
            115, 131, 163, 195, 227, 258,
        ];
        let (index, &base) = BASE
            .iter()
            .enumerate()
            .rev()
            .find(|(_, n)| **n <= len)
            .unwrap_or((0, &3));
        let index = u16::try_from(index).unwrap_or_default();
        self.symbol(257u16.saturating_add(index));
        let extra = if index == 28 {
            0
        } else {
            index.saturating_sub(4) / 4
        };
        self.write(u32::from(len.saturating_sub(base)), u32::from(extra));
        self.write(0, 5); // distance code 0
    }
}

pub(super) fn zlib(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits {
        bytes: vec![0x78, 0x01],
        ..Bits::default()
    };
    bits.write(3, 3); // Final fixed-Huffman block.
    let mut bytes = data.iter().copied().peekable();
    while let Some(literal) = bytes.next() {
        bits.symbol(u16::from(literal));
        let mut count = 0u16;
        while bytes.next_if_eq(&literal).is_some() {
            count = count.saturating_add(1);
            if count == 258 {
                bits.repeat(count);
                count = 0;
            }
        }
        if count >= 3 {
            bits.repeat(count);
        } else {
            for _ in 0..count {
                bits.symbol(u16::from(literal));
            }
        }
    }
    bits.symbol(256);
    bits.align();
    let (a, b) = data.iter().fold((1u32, 0u32), |(a, b), v| {
        let a = a.wrapping_add(u32::from(*v)) % 65521;
        (a, b.wrapping_add(a) % 65521)
    });
    bits.bytes.extend(((b << 16) | a).to_be_bytes());
    bits.bytes
}

pub(super) fn brotli(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.write(0, 1); // WBITS = 16
    for block in data.chunks(65_536) {
        bits.write(0, 1); // ISLAST
        bits.write(0, 2); // MNIBBLES = 4
        bits.write(
            u32::try_from(block.len().saturating_sub(1)).unwrap_or_default(),
            16,
        );
        bits.write(1, 1); // ISUNCOMPRESSED
        bits.align();
        bits.bytes.extend(block);
    }
    bits.write(3, 2); // ISLAST and ISLASTEMPTY
    bits.align();
    bits.bytes
}
