//! MATH fixture from sofel-read's table test, reused for ADR 0062 queries.

#![allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]

use super::build::*;

/// Absolute positions of the parts of [`sample`].
const CONSTANTS: usize = 10;
const DEVICE: usize = 224;
const GLYPH_INFO: usize = 232;
const ITALICS: usize = 240;
const ITALICS_COV: usize = 252;
const ACCENT: usize = 260;
const ACCENT_COV: usize = 268;
const EXTENDED: usize = 274;
const KERN_INFO: usize = 284;
const KERN_COV: usize = 296;
const KERN: usize = 302;
const VARIANTS: usize = 316;
const VERT_COV: usize = 330;
const HORIZ_COV: usize = 336;
const CONSTRUCTION0: usize = 342;
const ASSEMBLY: usize = 354;
const CONSTRUCTION1: usize = 380;
const END: usize = 388;

fn off(to: usize, from: usize) -> u16 {
    (to - from) as u16
}

/// A MATH table with every subtable: italics corrections for glyphs 3
/// and 5, a top accent for 5, extended shapes 7 and 8, a top right kern
/// for 3, a vertical construction for 10 (variants 12 and 13, parts 14
/// and 15) and a horizontal one for 11 (variant 16).
pub fn sample() -> Vec<u8> {
    let mut b = Buf::new()
        .u32(0x0001_0000)
        .u16(off(CONSTANTS, 0))
        .u16(off(GLYPH_INFO, 0))
        .u16(off(VARIANTS, 0));
    // MathConstants.
    b = b.i16(80).i16(60).u16(1500).u16(2000);
    for i in 0..51u16 {
        let device = if i == 0 { off(DEVICE, CONSTANTS) } else { 0 };
        b = b.i16(i.cast_signed() * 10).u16(device);
    }
    b = b.i16(55);
    assert_eq!(b.len(), DEVICE);
    b = b.u16(12).u16(12).u16(1).u16(0x4000);
    assert_eq!(b.len(), GLYPH_INFO);
    b = b
        .u16(off(ITALICS, GLYPH_INFO))
        .u16(off(ACCENT, GLYPH_INFO))
        .u16(off(EXTENDED, GLYPH_INFO))
        .u16(off(KERN_INFO, GLYPH_INFO));
    b = b
        .u16(off(ITALICS_COV, ITALICS))
        .u16(2)
        .i16(30)
        .u16(0)
        .i16(-40)
        .u16(0);
    b = b.u16(1).u16(2).u16(3).u16(5);
    assert_eq!(b.len(), ACCENT);
    b = b.u16(off(ACCENT_COV, ACCENT)).u16(1).i16(250).u16(0);
    b = b.u16(1).u16(1).u16(5);
    assert_eq!(b.len(), EXTENDED);
    b = b.u16(2).u16(1).u16(7).u16(8).u16(0);
    assert_eq!(b.len(), KERN_INFO);
    b = b
        .u16(off(KERN_COV, KERN_INFO))
        .u16(1)
        .u16(off(KERN, KERN_INFO))
        .u16(0)
        .u16(0)
        .u16(0);
    b = b.u16(1).u16(1).u16(3);
    assert_eq!(b.len(), KERN);
    b = b.u16(1).i16(100).u16(0).i16(-10).u16(0).i16(-20).u16(0);
    assert_eq!(b.len(), VARIANTS);
    b = b
        .u16(50)
        .u16(off(VERT_COV, VARIANTS))
        .u16(off(HORIZ_COV, VARIANTS))
        .u16(1)
        .u16(1)
        .u16(off(CONSTRUCTION0, VARIANTS))
        .u16(off(CONSTRUCTION1, VARIANTS));
    b = b.u16(1).u16(1).u16(10);
    b = b.u16(1).u16(1).u16(11);
    assert_eq!(b.len(), CONSTRUCTION0);
    b = b
        .u16(off(ASSEMBLY, CONSTRUCTION0))
        .u16(2)
        .u16(12)
        .u16(1000)
        .u16(13)
        .u16(1500);
    assert_eq!(b.len(), ASSEMBLY);
    b = b.i16(5).u16(0).u16(2);
    b = b.u16(14).u16(0).u16(100).u16(500).u16(0);
    b = b.u16(15).u16(100).u16(0).u16(300).u16(1);
    assert_eq!(b.len(), CONSTRUCTION1);
    b = b.u16(0).u16(1).u16(16).u16(800);
    assert_eq!(b.len(), END);
    b.done()
}

/// GDEF with two ligature carets for glyph 3, the second with a Device.
pub fn gdef_with_carets() -> Vec<u8> {
    Buf::new()
        .u32(0x0001_0000)
        .u16(0)
        .u16(0)
        .u16(12)
        .u16(0)
        .u16(6)
        .u16(1)
        .u16(12)
        .u16(1)
        .u16(1)
        .u16(3)
        .u16(2)
        .u16(6)
        .u16(10)
        .u16(1)
        .i16(100)
        .u16(3)
        .i16(200)
        .u16(6)
        .u16(12)
        .u16(13)
        .u16(1)
        .u16(0x7000)
        .done()
}

/// One-axis MATH font whose leading constant uses a GDEF VariationIndex.
pub fn variable_math_font() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .done();
    let mut math = sample();
    math[DEVICE..DEVICE + 6].copy_from_slice(&[0, 0, 0, 0, 0x80, 0]);
    let mut gdef = Buf::new().u32(0x0001_0003);
    for _ in 0..5 {
        gdef = gdef.u16(0);
    }
    let mut gdef = gdef.u32(18).done();
    gdef.extend_from_slice(&store);
    let mut tables = sample_tables();
    tables.push((b"MATH", math));
    tables.push((b"GDEF", gdef));
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}
