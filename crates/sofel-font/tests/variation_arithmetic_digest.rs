//! The portable little-endian arithmetic record required by ADR 0030.
use sofel_font::var::{Arith, Ft, HbF32, HbF64};

fn append(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
}

#[test]
fn adr0030_i01_arithmetic_digest() {
    let mut hash = 0xcbf29ce484222325;
    for coord in [0, 1, 2048, 4096, 8191, 8192, 12288, 16383, 16384] {
        let hb = HbF32::fold_region_axis(1.0, coord, 0, 8192, 16384);
        let ft = Ft::fold_region_axis(
            Ft::one(),
            sofel_font::read::Fixed(coord.saturating_mul(4)),
            0,
            8192,
            16384,
        );
        let mut single = HbF32::zero();
        let mut double = HbF64::zero();
        let mut fixed = Ft::zero();
        for delta in [100_000_000, 1, -100_000_000] {
            single = HbF32::mul_add(single, hb, delta);
            double = HbF64::mul_add(double, hb, delta);
            fixed = Ft::mul_add(fixed, ft, delta);
        }
        append(&mut hash, &HbF32::finish(single).to_le_bytes());
        append(&mut hash, &HbF64::finish(double).to_le_bytes());
        append(&mut hash, &Ft::finish(fixed).to_le_bytes());
    }
    for value in [
        0.0_f32,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0xffc0_1234),
    ] {
        let single = HbF32::finish(value);
        let double = HbF64::finish(f64::from(value));
        let (bits32, bits64) = if value.is_nan() {
            assert!(single.is_nan() && double.is_nan());
            (0x7fc0_0000, 0x7ff8_0000_0000_0000)
        } else {
            assert_eq!(single.to_bits(), value.to_bits());
            assert_eq!(double.to_bits(), f64::from(value).to_bits());
            (single.to_bits(), double.to_bits())
        };
        append(&mut hash, &bits32.to_le_bytes());
        append(&mut hash, &bits64.to_le_bytes());
    }
    assert_eq!(hash, 0x9108_3a9b_61fc_0942);
}
