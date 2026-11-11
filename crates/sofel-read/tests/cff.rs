//! The CFF and CFF2 views, the DICT parsers, the charstring decoder and
//! the validators (ADR 0021).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod support;

use sofel_read::cff::charstring::{
    self, CharStringErrorKind, CharStringToken, CharStringTokens, Context, NullSink, PathSink, Work,
};
use sofel_read::cff::dict::{
    self, DictErrorKind, DictKind, DictToken, DictTokens, FontDict, NoBlend, Operand, PrivateDict,
    TopDict, op,
};
use sofel_read::cff::{
    Cff1, Cff2, Charset, Encoding, FdSelect, FtBlend, Header, Index, Version, subr_bias,
};
use sofel_read::item_variation::ItemVariationStore;
use sofel_read::{Diagnostic, Kind, Severity};
use support::build::*;

// ---------------------------------------------------------------- INDEX

#[test]
fn index_empty_and_counts() {
    let empty = cff_index(&[], 1, false);
    let i = Index::new(&empty, Version::Cff).unwrap();
    assert_eq!(i.len(), 0);
    assert_eq!(i.end(), Some(2));
    assert_eq!(i.get(0), None);
    let empty2 = cff_index(&[], 1, true);
    let i = Index::new(&empty2, Version::Cff2).unwrap();
    assert_eq!(i.end(), Some(4));
    for off_size in 1..=4u8 {
        for wide in [false, true] {
            let data = cff_index(
                &[b"ab".to_vec(), b"".to_vec(), b"cde".to_vec()],
                off_size,
                wide,
            );
            let v = if wide { Version::Cff2 } else { Version::Cff };
            let i = Index::new(&data, v).unwrap();
            assert_eq!(i.len(), 3);
            assert_eq!(i.off_size(), off_size);
            assert_eq!(i.get(0), Some(&b"ab"[..]));
            assert_eq!(i.get(1), Some(&b""[..]));
            assert_eq!(i.get(2), Some(&b"cde"[..]));
            assert_eq!(i.get(3), None);
            assert_eq!(i.end(), Some(data.len()));
        }
    }
}

#[test]
fn index_bad_offsets() {
    // Offset size 0 and 5.
    assert!(Index::new(&[0, 1, 0, 1, 1], Version::Cff).is_err());
    assert!(Index::new(&[0, 1, 5, 0, 0, 0, 0, 1], Version::Cff).is_err());
    // Offsets cut.
    assert!(Index::new(&[0, 2, 1, 1, 2], Version::Cff).is_err());
    // Not ascending: element 0 is [1, 3), element 1 is (3 > 2).
    let data = [0, 2, 1, 1, 3, 2, b'a', b'b'];
    let i = Index::new(&data, Version::Cff).unwrap();
    assert_eq!(i.get(0), Some(&b"ab"[..]));
    assert_eq!(i.get(1), None);
    // FreeType's cff_index_get_pointers raises the third offset to the
    // second.
    assert!(!i.ft_offsets_sorted());
    assert_eq!(i.ft_pointer(1, false), Some(&b""[..]));
    // Offset 0.
    let data = [0, 1, 1, 0, 1];
    let i = Index::new(&data, Version::Cff).unwrap();
    assert_eq!(i.get(0), None);
    assert_eq!(i.end(), Some(5));
    // Data outside.
    let data = [0, 1, 1, 1, 9, b'a'];
    let i = Index::new(&data, Version::Cff).unwrap();
    assert_eq!(i.get(0), None);
    // FreeType's cff_index_access_element cuts the object at the end.
    assert_eq!(i.ft_element(0), Some(&b"a"[..]));
    assert_eq!(i.ft_element(1), None);
}

// ---------------------------------------------------------------- DICT

fn tokens(data: &[u8]) -> Vec<DictToken<'_>> {
    DictTokens::new(data).map(|t| t.unwrap().1).collect()
}

#[test]
fn dict_integer_boundaries() {
    let cases: &[(&[u8], i64)] = &[
        (&[32], -107),
        (&[139], 0),
        (&[246], 107),
        (&[247, 0], 108),
        (&[250, 255], 1131),
        (&[251, 0], -108),
        (&[254, 255], -1131),
        (&[28, 0x80, 0x00], -32768),
        (&[28, 0x7F, 0xFF], 32767),
        (&[29, 0x7F, 0xFF, 0xFF, 0xFF], 0x7FFF_FFFF),
        // FreeType on 64-bit targets does not sign-extend.
        (&[29, 0xFF, 0xFF, 0xFF, 0xFF], 0xFFFF_FFFF),
        // FreeType reads 27 as a one-byte integer.
        (&[27], 27 - 139),
        // Cut by the end of the data: 0.
        (&[28, 1], 0),
        (&[247], 0),
    ];
    for (bytes, v) in cases {
        assert_eq!(tokens(bytes), vec![DictToken::Integer(*v)], "{bytes:?}");
    }
    assert_eq!(tokens(&[12, 7]), vec![DictToken::Operator(op::FONT_MATRIX)]);
    assert_eq!(tokens(&[31]), vec![DictToken::Operator(31)]);
    assert_eq!(tokens(&[255]), vec![DictToken::Operator(255)]);
    let e = DictTokens::new(&[139, 12]).last().unwrap().unwrap_err();
    assert_eq!(e.kind, DictErrorKind::TruncatedOperator);
    assert_eq!(e.offset, 1);
}

fn real(s: &str) -> i64 {
    let b = d_real(s);
    Operand::Real(&b[1..]).fixed()
}

#[test]
fn dict_reals() {
    // FreeType's cff_parse_real by hand.
    assert_eq!(real("1.5"), 0x18000);
    assert_eq!(real("-2.25"), -0x24000);
    // FreeType drops the digits after leading fraction zeros push the
    // integer length below 0: 0.001 is 0 (cff_parse_fixed), and only
    // cff_parse_fixed_dynamic keeps it.
    assert_eq!(real("0.001"), 0);
    assert_eq!(real("0.5"), 0x8000);
    // FT_DivFix(1, 10).
    assert_eq!(real("0.1"), 6554);
    assert_eq!(real("1E3"), 1000 << 16);
    assert_eq!(real("2.5E-1"), 0x4000);
    // More than 5 integer digits: 0x7FFFFFFF.
    assert_eq!(real("1E5"), 0x7FFF_FFFF);
    assert_eq!(real("-123456"), -0x7FFF_FFFF);
    // Too small: 0.
    assert_eq!(real("1E-6"), 0);
    // An exponent beyond 1000 overflows.
    assert_eq!(real("1E1001"), 0x7FFF_FFFF);
    assert_eq!(real("1E-1001"), 0);
    // Leading zeros are skipped.
    assert_eq!(real("0000.5"), 0x8000);
    // cff_parse_num truncates.
    let b = d_real("-2.75");
    assert_eq!(Operand::Real(&b[1..]).num(), -3);
    // An unterminated real at the end of the data is returned and ends
    // the tokens.
    assert_eq!(tokens(&[30, 0x15]), vec![DictToken::Real(&[0x15])]);
    // Cut data gives 0.
    assert_eq!(Operand::Real(&[0x15]).fixed(), 0);
}

#[test]
fn dict_fixed_conversions() {
    // Integers: shifted to 16.16, clamped to +-0x7FFFFFFF.
    assert_eq!(Operand::Integer(3).fixed(), 3 << 16);
    assert_eq!(Operand::Integer(40000).fixed(), 0x7FFF_FFFF);
    assert_eq!(Operand::Integer(-40000).fixed(), -0x7FFF_FFFF);
    // Scaled by 1000 (BlueScale).
    assert_eq!(Operand::Integer(1).fixed_scaled(3), 1000 << 16);
    let b = d_real("0.039625");
    assert_eq!(Operand::Real(&b[1..]).fixed_scaled(3), 2_596_864);
    // Blended values (byte 255): parse_num rounds.
    assert_eq!(Operand::Blended(0x18000).num(), 2);
    assert_eq!(Operand::Blended(0x17FFF).num(), 1);
    assert_eq!(Operand::Blended(-0x18000).num(), -1);
    assert_eq!(Operand::Blended(0x18000).fixed(), 0x18000);
    // cff_parse_fixed_dynamic.
    let b = d_real("0.001");
    assert_eq!(Operand::Real(&b[1..]).fixed_dynamic(), (65536, -3));
    assert_eq!(Operand::Integer(1).fixed_dynamic(), (65536, 0));
    // 100000 has 6 digits: divided by 10^1 (FT_DivFix(100000, 10)).
    assert_eq!(Operand::Integer(100_000).fixed_dynamic(), (10000 << 16, 1));
    // 40000 has 5 digits but is above 0x7FFF: divided by 10^1.
    assert_eq!(Operand::Integer(40000).fixed_dynamic(), (4000 << 16, 1));
}

#[test]
fn top_dict_defaults() {
    let t = TopDict::parse(&[], Version::Cff).unwrap();
    assert_eq!(t, TopDict::defaults(Version::Cff));
    assert_eq!(t.underline_position, -100);
    assert_eq!(t.underline_thickness, 50);
    assert_eq!(t.charstring_type, 2);
    assert_eq!(t.cid_count, 8720);
    assert_eq!(t.version, dict::NO_SID);
    assert_eq!(t.maxstack, 48);
    assert!(!t.font_matrix.present);
    assert!(!t.is_cid());
    let t2 = TopDict::parse(&[], Version::Cff2).unwrap();
    assert_eq!(t2.maxstack, 513);
}

#[test]
fn top_dict_values() {
    let mut data = dict(&[
        (&[391], op::VERSION),
        (&[1], op::IS_FIXED_PITCH),
        (&[-50], op::UNDERLINE_POSITION),
        (&[100, 200, 300, 400], op::FONT_BBOX),
        (&[392, 393, 4], op::ROS),
        (&[17], op::CHAR_STRINGS),
        (&[18], op::CHAR_STRINGS),
        (&[10, 20], op::PRIVATE),
    ]);
    for s in ["0.001", "0", "0", "0.001", "0", "0"] {
        data.extend(d_real(s));
    }
    data.extend(d_op(op::FONT_MATRIX));
    let t = TopDict::parse(&data, Version::Cff).unwrap();
    assert_eq!(t.version, 391);
    assert_eq!(t.is_fixed_pitch, 1);
    assert_eq!(t.underline_position, -50);
    assert_eq!(t.font_bbox, [100 << 16, 200 << 16, 300 << 16, 400 << 16]);
    assert_eq!(
        (t.cid_registry, t.cid_ordering, t.cid_supplement),
        (392, 393, 4)
    );
    assert!(t.is_cid());
    // The last one wins.
    assert_eq!(t.char_strings, 18);
    assert_eq!((t.private_size, t.private_offset), (10, 20));
    assert!(t.font_matrix.present);
    assert_eq!((t.font_matrix.xx, t.font_matrix.yy), (65536, 65536));
    assert_eq!(t.font_matrix.units_per_em, 1000);
    // A real supplement is truncated.
    let mut ros = dict(&[]);
    ros.extend(d_int(392));
    ros.extend(d_int(393));
    ros.extend(d_real("2.9"));
    ros.extend(d_op(op::ROS));
    assert_eq!(
        TopDict::parse(&ros, Version::Cff).unwrap().cid_supplement,
        2
    );
}

#[test]
fn top_dict_errors() {
    // An operator without operands.
    let e = TopDict::parse(&dict(&[(&[], op::VERSION)]), Version::Cff).unwrap_err();
    assert_eq!(
        e.kind,
        DictErrorKind::StackUnderflow {
            operator: op::VERSION
        }
    );
    // FontBBox with three.
    let e = TopDict::parse(&dict(&[(&[1, 2, 3], op::FONT_BBOX)]), Version::Cff).unwrap_err();
    assert_eq!(
        e.kind,
        DictErrorKind::StackUnderflow {
            operator: op::FONT_BBOX
        }
    );
    // 95 operands fit a Top DICT; 96 do not (FreeType's stack of 96
    // holds the operator too).
    let ok: Vec<i32> = vec![0; 95];
    assert!(TopDict::parse(&dict(&[(&ok, op::XUID)]), Version::Cff).is_ok());
    let over: Vec<i32> = vec![0; 96];
    let e = TopDict::parse(&dict(&[(&over, op::XUID)]), Version::Cff).unwrap_err();
    assert_eq!(e.kind, DictErrorKind::StackOverflow);
    // A negative Private size.
    let e = TopDict::parse(&dict(&[(&[-1, 0], op::PRIVATE)]), Version::Cff).unwrap_err();
    assert_eq!(
        e.kind,
        DictErrorKind::InvalidValue {
            operator: op::PRIVATE
        }
    );
    // MultipleMaster with 17 designs.
    let e = TopDict::parse(
        &dict(&[(&[17, 0, 0, 0, 0], op::MULTIPLE_MASTER)]),
        Version::Cff,
    )
    .unwrap_err();
    assert_eq!(
        e.kind,
        DictErrorKind::InvalidValue {
            operator: op::MULTIPLE_MASTER
        }
    );
    // Operators of the other version are ignored.
    let t = TopDict::parse(
        &dict(&[(&[5], op::VERSION), (&[7], op::VSTORE)]),
        Version::Cff2,
    )
    .unwrap();
    assert_eq!(t.version, dict::NO_SID);
    assert_eq!(t.vstore, 7);
    // maxstack is always 513.
    for v in [1, 513, 600] {
        let t = TopDict::parse(&dict(&[(&[v], op::MAXSTACK)]), Version::Cff2).unwrap();
        assert_eq!(t.maxstack, 513);
    }
}

#[test]
fn font_matrix_scaling() {
    // Integers above 0x7FFF get a positive power of ten, which FreeType
    // finds unlikely: the default matrix with units_per_em 1.
    let data = dict(&[(&[100_000, 0, 0, 100_000, 0, 0], op::FONT_MATRIX)]);
    let t = TopDict::parse(&data, Version::Cff).unwrap();
    assert_eq!(t.font_matrix.xx, 65536);
    assert_eq!(t.font_matrix.units_per_em, 1);
    // Mixed powers: 0.0005 (5 * 10^-4) and 0.001 (1 * 10^-3) are scaled
    // to the larger power, 10^-3: 0.5 (rounded) and 1.
    let mut data = Vec::new();
    for s in ["0.0005", "0", "0", "0.001", "0", "0"] {
        data.extend(d_real(s));
    }
    data.extend(d_op(op::FONT_MATRIX));
    let t = TopDict::parse(&data, Version::Cff).unwrap();
    assert_eq!(
        (
            t.font_matrix.xx,
            t.font_matrix.yy,
            t.font_matrix.units_per_em
        ),
        (32768, 65536, 1000)
    );
    // A degenerate matrix becomes the default with units_per_em 1.
    let data = dict(&[(&[0, 0, 0, 0, 0, 0], op::FONT_MATRIX)]);
    let t = TopDict::parse(&data, Version::Cff).unwrap();
    assert!(t.font_matrix.present);
    assert_eq!(
        (
            t.font_matrix.xx,
            t.font_matrix.yy,
            t.font_matrix.units_per_em
        ),
        (65536, 65536, 1)
    );
    // Fewer than six values fail.
    let data = dict(&[(&[1, 0, 0, 1, 0], op::FONT_MATRIX)]);
    assert!(TopDict::parse(&data, Version::Cff).is_err());
}

#[test]
fn private_dict_defaults_and_values() {
    let p = PrivateDict::parse(&[], Version::Cff, &NoBlend).unwrap();
    assert_eq!(p.blue_shift, 7);
    assert_eq!(p.blue_fuzz, 1);
    assert_eq!(p.len_iv, -1);
    assert_eq!(p.expansion_factor, 3932);
    assert_eq!(p.blue_scale, 2_596_864);
    // initialRandomSeed 0 becomes 987654321.
    assert_eq!(p.initial_random_seed, 987_654_321);
    let p = PrivateDict::parse(
        &dict(&[
            (&[-5], op::INITIAL_RANDOM_SEED),
            (&[1, 2, 3], op::BLUE_VALUES),
            (&[2000], op::BLUE_SHIFT),
            (&[10, 5], op::STEM_SNAP_H),
            (&[100], op::DEFAULT_WIDTH_X),
            (&[200], op::NOMINAL_WIDTH_X),
            (&[50], op::SUBRS),
        ]),
        Version::Cff,
        &NoBlend,
    )
    .unwrap();
    assert_eq!(p.initial_random_seed, 5);
    // An odd count of BlueValues is made even; the values accumulate.
    assert_eq!(p.blue_values.as_slice(), &[1 << 16, 3 << 16]);
    assert_eq!(p.blue_shift, 7);
    assert_eq!(p.stem_snap_h.as_slice(), &[10, 15]);
    assert_eq!(
        (p.default_width_x, p.nominal_width_x, p.subrs),
        (100, 200, 50)
    );
    // The default for a font without a Private DICT is all zero.
    assert_eq!(PrivateDict::default().initial_random_seed, 0);
    // 96 operands fit a CFF Private DICT.
    let ok: Vec<i32> = vec![0; 96];
    assert!(PrivateDict::parse(&dict(&[(&ok, op::BLUE_VALUES)]), Version::Cff, &NoBlend).is_ok());
    let over: Vec<i32> = vec![0; 97];
    assert!(
        PrivateDict::parse(&dict(&[(&over, op::BLUE_VALUES)]), Version::Cff, &NoBlend).is_err()
    );
}

fn store_one_region(peaks: &[(i16, i16, i16)]) -> Vec<u8> {
    item_variation_store(peaks.len() as u16, &[peaks.to_vec()], &[vec![0]])
}

#[test]
fn private_dict_blend() {
    // 10 + 4 * BV[1] with one region.
    let data = dict(&[(&[10, 4, 1], op::BLEND), (&[], op::STD_HW)]);
    let ivs = store_one_region(&[(0, 0x4000, 0x4000)]);
    let store = ItemVariationStore::new(&ivs).unwrap();
    // No fvar: BV = (1, 0).
    let p = PrivateDict::parse(&data, Version::Cff2, &FtBlend::new(Some(store), &[])).unwrap();
    assert_eq!(p.std_hw, 10);
    // An axis whose peak is 0 counts 1: 10 + 4.
    let ivs0 = store_one_region(&[(0, 0, 0)]);
    let store0 = ItemVariationStore::new(&ivs0).unwrap();
    let p = PrivateDict::parse(&data, Version::Cff2, &FtBlend::new(Some(store0), &[0])).unwrap();
    assert_eq!(p.std_hw, 14);
    // Without a store, blend fails.
    let e = PrivateDict::parse(&data, Version::Cff2, &NoBlend).unwrap_err();
    assert_eq!(e.kind, DictErrorKind::Blend);
    // Too few operands.
    let short = dict(&[(&[4, 1], op::BLEND)]);
    let e = PrivateDict::parse(&short, Version::Cff2, &FtBlend::new(Some(store), &[])).unwrap_err();
    assert_eq!(
        e.kind,
        DictErrorKind::StackUnderflow {
            operator: op::BLEND
        }
    );
    // vsindex after blend.
    let after = dict(&[
        (&[10, 4, 1], op::BLEND),
        (&[], op::STD_HW),
        (&[0], op::VSINDEX),
    ]);
    let e = PrivateDict::parse(&after, Version::Cff2, &FtBlend::new(Some(store), &[])).unwrap_err();
    assert_eq!(
        e.kind,
        DictErrorKind::InvalidValue {
            operator: op::VSINDEX
        }
    );
    // vsindex outside the store.
    let bad = dict(&[
        (&[1], op::VSINDEX),
        (&[10, 4, 1], op::BLEND),
        (&[], op::STD_HW),
    ]);
    assert!(PrivateDict::parse(&bad, Version::Cff2, &FtBlend::new(Some(store), &[])).is_err());
}

#[test]
fn blend_vector_at_default_instance() {
    let ivs = item_variation_store(
        1,
        &[
            vec![(0, 0x4000, 0x4000)],
            vec![(0, 0, 0)],
            vec![(-0x4000, 0x2000, 0x4000)],
            vec![(-0x4000, -0x2000, 0)],
        ],
        &[vec![0, 1, 2, 3]],
    );
    let store = ItemVariationStore::new(&ivs).unwrap();
    let b = FtBlend::new(Some(store), &[0]);
    use dict::BlendWeights;
    assert_eq!(b.regions(0), Some(4));
    // Peak above the coordinate, start at it: 0.
    assert_eq!(b.weight(0, 0), 0);
    // Every peak 0: 1.
    assert_eq!(b.weight(0, 1), 0x10000);
    // A negative start with a positive end is invalid: peak 0, 1.
    assert_eq!(b.weight(0, 2), 0x10000);
    // A coordinate at the end: 0.
    assert_eq!(b.weight(0, 3), 0);
    // No fvar: all 0.
    let b = FtBlend::new(Some(store), &[]);
    assert_eq!(b.weight(0, 1), 0);
    // Coordinates that do not match the axes.
    let b = FtBlend::new(Some(store), &[0, 0]);
    assert_eq!(b.regions(0), None);
    // A region index outside.
    let ivs = item_variation_store(1, &[vec![(0, 0, 0)]], &[vec![1]]);
    let store = ItemVariationStore::new(&ivs).unwrap();
    assert_eq!(FtBlend::new(Some(store), &[]).regions(0), None);
}

#[test]
fn font_dict_cff2() {
    let data = dict(&[(&[3, 7], op::PRIVATE), (&[1], op::VERSION)]);
    let f = FontDict::parse(&data, Version::Cff2).unwrap();
    assert_eq!((f.private_size, f.private_offset), (3, 7));
    let f = FontDict::parse(&data, Version::Cff).unwrap();
    assert_eq!((f.private_size, f.private_offset), (3, 7));
    assert_eq!(dict::field(DictKind::Font2, op::VERSION), None);
}

// ---------------------------------------------------------------- charset, encoding, FDSelect

#[test]
fn charsets() {
    let iso = Charset::new(&[], 0, 229).unwrap();
    assert_eq!(iso.sid(228), Some(228));
    assert!(Charset::new(&[], 0, 230).is_err());
    assert!(Charset::new(&[], 1, 167).is_err());
    assert_eq!(Charset::new(&[], 1, 166).unwrap().sid(2), Some(229));
    assert!(Charset::new(&[], 2, 88).is_err());
    assert_eq!(Charset::new(&[], 2, 87).unwrap().sid(2), Some(231));
    // Format 0.
    let t = [0u8, 0, 0, 0, 0, 5, 1, 0];
    let c = Charset::new(&t, 3, 3).unwrap();
    assert_eq!(c.sid(0), Some(0));
    assert_eq!(c.sid(1), Some(5));
    assert_eq!(c.sid(2), Some(256));
    assert_eq!(c.sid(3), None);
    assert!(Charset::new(&t, 3, 4).is_err());
    // Format 1: SIDs 10, 11, 12, then 20.
    let t = [0u8, 0, 0, 1, 0, 10, 2, 0, 20, 0];
    let c = Charset::new(&t, 3, 5).unwrap();
    assert_eq!(c.sids().collect::<Vec<_>>(), vec![0, 10, 11, 12, 20]);
    assert_eq!(c.sid(4), Some(20));
    assert_eq!(c.glyph(12), Some(3));
    assert!(Charset::new(&t, 3, 6).is_err());
    // A range that covers more glyphs than there are is fine.
    assert_eq!(Charset::new(&t, 3, 3).unwrap().sid(2), Some(11));
    // Format 2 with nLeft past 0xFFFF: cut.
    let t = [0u8, 0, 0, 2, 0xFF, 0xFE, 0x00, 0x05, 0, 1, 0, 1];
    let c = Charset::new(&t, 3, 5).unwrap();
    assert_eq!(c.sids().collect::<Vec<_>>(), vec![0, 0xFFFE, 0xFFFF, 1, 2]);
    // Unknown format.
    assert!(Charset::new(&[0, 0, 0, 3], 3, 2).is_err());
}

#[test]
fn encodings() {
    // Charset: glyph 1 = SID 66 ('A' in the standard strings is SID 34).
    let cs_bytes = [0u8, 0, 0, 0, 0, 34, 0, 35];
    let charset = Charset::new(&cs_bytes, 3, 3).unwrap();
    let std = Encoding::new(&[], 0, 3).unwrap();
    // Standard encoding: code 65 ('A') is SID 34, glyph 1.
    assert_eq!(std.glyph(65, &charset), 1);
    assert_eq!(std.glyph(66, &charset), 2);
    assert_eq!(std.glyph(67, &charset), 0);
    let exp = Encoding::new(&[], 1, 3).unwrap();
    assert_eq!(exp.glyph(65, &charset), 0);
    // Format 0: codes 70 and 71; a later code wins over an earlier one.
    let t = [0u8, 0, 2, 70, 70];
    let e = Encoding::new(&t, 1, 3);
    assert!(e.is_ok());
    let e = Encoding::new(&t, 1, 3).unwrap();
    let _ = e;
    let t = [0u8, 0, 0, 2, 70, 70];
    let e = Encoding::new(&t, 2, 3).unwrap();
    assert_eq!(e.format(), Some(0));
    assert_eq!(e.glyph(70, &charset), 2);
    // Format 1 with a supplement mapping code 90 to SID 35.
    let t = [0u8, 0, 0x81, 1, 80, 1, 1, 90, 0, 35];
    let e = Encoding::new(&t, 2, 3).unwrap();
    assert_eq!(e.glyph(80, &charset), 1);
    assert_eq!(e.glyph(81, &charset), 2);
    assert_eq!(e.glyph(90, &charset), 2);
    assert_eq!(e.supplements().collect::<Vec<_>>(), vec![(90, 35)]);
    // Unknown format and cut data.
    assert!(Encoding::new(&[0, 0, 2, 0], 2, 3).is_err());
    assert!(Encoding::new(&[0, 0, 0, 3, 1], 2, 3).is_err());
}

#[test]
fn fd_selects() {
    let t = [0u8, 0, 1, 2];
    let s = FdSelect::new(&t, 0, 3).unwrap();
    assert_eq!((s.fd(0), s.fd(1), s.fd(2), s.fd(9)), (0, 1, 2, 0));
    assert!(FdSelect::new(&t, 0, 4).is_err());
    // Format 3: [0, 2) -> 1, [2, 5) -> 0, sentinel 5.
    let t = [3u8, 0, 2, 0, 0, 1, 0, 2, 0, 0, 5];
    let s = FdSelect::new(&t, 0, 5).unwrap();
    assert_eq!(
        (s.fd(0), s.fd(1), s.fd(2), s.fd(4), s.fd(5)),
        (1, 1, 0, 0, 0)
    );
    assert_eq!(s.range(2), Some((5, 0)));
    // No ranges.
    assert!(FdSelect::new(&[3, 0, 0, 0, 0], 0, 1).is_err());
    // Format 4.
    let t = [4u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 3, 0, 0, 0, 2];
    let s = FdSelect::new(&t, 0, 2).unwrap();
    assert_eq!((s.format(), s.fd(1)), (4, 3));
    assert!(FdSelect::new(&[5], 0, 1).is_err());
}

#[test]
fn item_variation_store_view() {
    let ivs = item_variation_store(2, &[vec![(0, 1, 2), (3, 4, 5)]], &[vec![0, 0]]);
    let s = ItemVariationStore::new(&ivs).unwrap();
    assert_eq!(
        (s.axis_count(), s.region_count(), s.data_count()),
        (2, 1, 1)
    );
    assert_eq!(s.region(0).unwrap().axis(1), Some((3, 4, 5)));
    assert_eq!(s.region(1), None);
    let d = s.item_data(0).unwrap();
    assert_eq!((d.region_index_count(), d.region_index(1)), (2, Some(0)));
    assert!(ItemVariationStore::new(&[0, 2, 0, 0, 0, 8, 0, 0]).is_err());
    assert!(ItemVariationStore::new(&ivs[..10]).is_err());
}

// ---------------------------------------------------------------- tables

#[test]
fn cff1_view() {
    let mut spec = Cff1Spec::new(vec![charstring(&[Cs::Op(14)])]);
    spec.strings = vec![b"Glyph1".to_vec()];
    let t = cff1(&spec);
    let c = Cff1::new(&t).unwrap();
    assert_eq!(
        c.header(),
        Header {
            major: 1,
            minor: 0,
            header_size: 4,
            off_size: 4,
            top_dict_length: 0,
        }
    );
    assert_eq!(c.names().get(0), Some(&b"Test"[..]));
    assert_eq!(c.string(0), Some(&b".notdef"[..]));
    assert_eq!(c.string(390), Some(&b"Semibold"[..]));
    assert_eq!(c.string(391), Some(&b"Glyph1"[..]));
    assert_eq!(c.string(392), None);
    let mut bad = t.clone();
    bad[0] = 2;
    assert!(Cff1::new(&bad).is_err());
    bad[0] = 1;
    bad[2] = 3;
    assert!(Cff1::new(&bad).is_err());
}

#[test]
fn cff2_view() {
    let t = cff2(&Cff2Spec {
        charstrings: vec![Vec::new()],
        fds: vec![(Vec::new(), Vec::new())],
        ..Default::default()
    });
    let c = Cff2::new(&t).unwrap();
    assert_eq!(c.header().major, 2);
    let top = TopDict::parse(c.top_dict(), Version::Cff2).unwrap();
    assert_ne!(top.char_strings, 0);
    assert!(c.global_subrs().is_empty());
}

#[test]
fn subroutine_bias() {
    assert_eq!(subr_bias(0), 107);
    assert_eq!(subr_bias(1239), 107);
    assert_eq!(subr_bias(1240), 1131);
    assert_eq!(subr_bias(33899), 1131);
    assert_eq!(subr_bias(33900), 32768);
}

// ---------------------------------------------------------------- charstrings

#[test]
fn char_string_tokens() {
    let data = charstring(&[
        Cs::I(-107),
        Cs::I(1131),
        Cs::I(-1131),
        Cs::I(-32768),
        Cs::F(0x18000),
        Cs::Op(0x0C23),
        Cs::Op(19),
    ]);
    let mut t = CharStringTokens::new(&data, Version::Cff);
    let mut out = Vec::new();
    while let Some(tok) = t.next_token() {
        out.push(tok.unwrap());
    }
    assert_eq!(
        out,
        vec![
            CharStringToken::Integer(-107),
            CharStringToken::Integer(1131),
            CharStringToken::Integer(-1131),
            CharStringToken::Integer(-32768),
            CharStringToken::Fixed(0x18000),
            CharStringToken::Operator(0x0C23),
            CharStringToken::Operator(19),
        ]
    );
    let mut t = CharStringTokens::new(&[19, 0xFF], Version::Cff);
    assert_eq!(t.next_token(), Some(Ok(CharStringToken::Operator(19))));
    assert!(t.skip(2).is_none());
    assert_eq!(t.skip(1), Some(&[0xFF][..]));
    assert!(t.at_end());
    assert_eq!(
        CharStringTokens::new(&[28, 1], Version::Cff).next_token(),
        Some(Err(0))
    );
}

/// What the decoder sent, in font units (device space `>> 10`).
#[derive(Default, Debug)]
struct Rec(Vec<String>);

impl PathSink for Rec {
    fn move_to(&mut self) {
        self.0.push("M".into());
    }
    fn line_to(&mut self, f: (i32, i32), t: (i32, i32)) {
        self.0.push(format!(
            "L {} {} {} {}",
            f.0 >> 10,
            f.1 >> 10,
            t.0 >> 10,
            t.1 >> 10
        ));
    }
    fn cubic_to(&mut self, f: (i32, i32), a: (i32, i32), b: (i32, i32), t: (i32, i32)) {
        let _ = f;
        self.0.push(format!(
            "C {} {} {} {} {} {}",
            a.0 >> 10,
            a.1 >> 10,
            b.0 >> 10,
            b.1 >> 10,
            t.0 >> 10,
            t.1 >> 10
        ));
    }
}

struct Font {
    version: Version,
    char_strings: Vec<u8>,
    charset: Option<Vec<u8>>,
    gsubrs: Vec<u8>,
    lsubrs: Vec<u8>,
    store: Option<Vec<u8>>,
    coords: Vec<i32>,
    seed: u32,
}

impl Font {
    fn new(version: Version) -> Font {
        let wide = version == Version::Cff2;
        Font {
            version,
            char_strings: cff_index(&[], 1, wide),
            charset: None,
            gsubrs: cff_index(&[], 1, wide),
            lsubrs: cff_index(&[], 1, wide),
            store: None,
            coords: Vec::new(),
            seed: 987_654_321,
        }
    }

    fn decode(
        &self,
        cs: &[u8],
    ) -> (
        Result<charstring::Decoded, charstring::CharStringError>,
        Vec<String>,
    ) {
        let store = self
            .store
            .as_deref()
            .map(|s| ItemVariationStore::new(s).unwrap());
        let blend = FtBlend::new(store, &self.coords);
        let char_strings = Index::new(&self.char_strings, self.version).unwrap();
        let charset = self
            .charset
            .as_deref()
            .map(|c| Charset::new(c, 3, char_strings.len()).unwrap());
        let g = Index::new(&self.gsubrs, self.version).unwrap();
        let l = Index::new(&self.lsubrs, self.version).unwrap();
        let ctx = Context {
            version: self.version,
            char_strings,
            charset,
            global_subrs: g,
            global_subrs_sorted: g.ft_offsets_sorted(),
            local_subrs: l,
            local_subrs_sorted: l.ft_offsets_sorted(),
            charstring_type: 2,
            default_width_x: 100,
            nominal_width_x: 200,
            max_stack: 513,
            has_variations: store.is_some_and(|s| s.data_count() > 0),
            vsindex: 0,
            blend: &blend,
            random_seed: self.seed,
            scale: 0x400,
        };
        let mut work = Work::default();
        let mut rec = Rec::default();
        let r = charstring::decode(cs, &ctx, &mut work, &mut rec);
        (r, rec.0)
    }
}

fn cff(items: &[Cs]) -> Vec<String> {
    let (r, out) = Font::new(Version::Cff).decode(&charstring(items));
    r.unwrap();
    out
}

fn cff_err(items: &[Cs]) -> CharStringErrorKind {
    Font::new(Version::Cff)
        .decode(&charstring(items))
        .0
        .unwrap_err()
        .kind
}

use Cs::{I, Op};

#[test]
fn moves_and_lines() {
    assert_eq!(
        cff(&[
            I(10),
            I(20),
            Op(21),
            I(5),
            Op(6),
            I(7),
            I(8),
            Op(5),
            I(3),
            Op(7),
            Op(14)
        ]),
        vec![
            "M",
            "L 10 20 15 20",
            "L 15 20 22 28",
            "L 22 28 22 31",
            "L 22 31 10 20",
        ]
    );
    // hlineto alternates: x, y, x.
    assert_eq!(
        cff(&[I(0), Op(22), I(1), I(2), I(3), Op(6), Op(14)]),
        vec!["M", "L 0 0 1 0", "L 1 0 1 2", "L 1 2 4 2", "L 4 2 0 0",]
    );
    // vlineto starts with y; vmoveto moves y.
    assert_eq!(
        cff(&[I(5), Op(4), I(1), I(2), Op(7), Op(14)]),
        vec!["M", "L 0 5 0 6", "L 0 6 2 6", "L 2 6 0 5",]
    );
}

#[test]
fn curves() {
    assert_eq!(
        cff(&[
            I(0),
            Op(22),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            I(6),
            Op(8),
            Op(14)
        ]),
        vec!["M", "C 1 2 4 6 9 12", "L 9 12 0 0",]
    );
    // hhcurveto with an odd argument count: dy1 first.
    assert_eq!(
        cff(&[I(0), Op(22), I(7), I(1), I(2), I(3), I(4), Op(27), Op(14)]),
        vec!["M", "C 1 7 3 10 7 10", "L 7 10 0 0",]
    );
    // vvcurveto with an odd count: dx1 first.
    assert_eq!(
        cff(&[I(0), Op(22), I(7), I(1), I(2), I(3), I(4), Op(26), Op(14)]),
        vec!["M", "C 7 1 9 4 9 8", "L 9 8 0 0",]
    );
    // hvcurveto with a final df.
    assert_eq!(
        cff(&[I(0), Op(22), I(1), I(2), I(3), I(4), I(5), Op(31), Op(14)]),
        vec!["M", "C 1 0 3 3 8 7", "L 8 7 0 0",]
    );
    // vhcurveto: two curves, alternating.
    assert_eq!(
        cff(&[
            I(0),
            Op(22),
            I(1),
            I(2),
            I(3),
            I(4),
            I(5),
            I(6),
            I(7),
            I(8),
            Op(30),
            Op(14)
        ]),
        vec!["M", "C 0 1 2 4 6 4", "C 11 4 17 11 17 19", "L 17 19 0 0",]
    );
    // rcurveline: a curve, then a line.
    assert_eq!(
        cff(&[
            I(0),
            Op(22),
            I(1),
            I(1),
            I(1),
            I(1),
            I(1),
            I(1),
            I(2),
            I(0),
            Op(24),
            Op(14)
        ]),
        vec!["M", "C 1 1 2 2 3 3", "L 3 3 5 3", "L 5 3 0 0",]
    );
    // rlinecurve: a line, then a curve.
    assert_eq!(
        cff(&[
            I(0),
            Op(22),
            I(2),
            I(0),
            I(1),
            I(1),
            I(1),
            I(1),
            I(1),
            I(1),
            Op(25),
            Op(14)
        ]),
        vec!["M", "L 0 0 2 0", "C 3 1 4 2 5 3", "L 5 3 0 0",]
    );
}

#[test]
fn flexes() {
    // flex: two curves; fd is ignored.
    let flex = cff(&[
        I(0),
        Op(22),
        I(1),
        I(0),
        I(1),
        I(1),
        I(1),
        I(0),
        I(1),
        I(0),
        I(1),
        I(-1),
        I(1),
        I(0),
        I(50),
        Op(0x0C23),
        Op(14),
    ]);
    assert_eq!(&flex[1..3], &["C 1 0 2 1 3 1", "C 4 1 5 0 6 0"]);
    // hflex: dy2 only; ends at the start's y.
    let hflex = cff(&[
        I(0),
        Op(22),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        Op(0x0C22),
        Op(14),
    ]);
    assert_eq!(&hflex[1..3], &["C 1 0 2 1 3 1", "C 4 1 5 0 6 0"]);
    // hflex1.
    let hflex1 = cff(&[
        I(0),
        Op(22),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(-2),
        I(1),
        Op(0x0C24),
        Op(14),
    ]);
    assert_eq!(&hflex1[1..3], &["C 1 1 2 2 3 2", "C 4 2 5 0 6 0"]);
    // flex1: the last point moves along the larger of |dx| and |dy|.
    let flex1 = cff(&[
        I(0),
        Op(22),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(1),
        I(-1),
        I(1),
        I(-1),
        I(4),
        Op(0x0C25),
        Op(14),
    ]);
    assert_eq!(&flex1[1..3], &["C 1 1 2 2 3 3", "C 4 2 5 1 9 0"]);
    let flex1y = cff(&[
        I(0),
        Op(22),
        I(0),
        I(1),
        I(0),
        I(1),
        I(0),
        I(1),
        I(0),
        I(1),
        I(1),
        I(1),
        I(4),
        Op(0x0C25),
        Op(14),
    ]);
    assert_eq!(&flex1y[2], "C 0 4 1 5 0 9");
}

#[test]
fn widths() {
    let f = Font::new(Version::Cff);
    let w = |items: &[Cs]| f.decode(&charstring(items)).0.unwrap().width >> 16;
    assert_eq!(w(&[Op(14)]), 100);
    assert_eq!(w(&[I(5), Op(14)]), 205);
    assert_eq!(w(&[I(5), I(0), I(0), Op(21), Op(14)]), 205);
    assert_eq!(w(&[I(0), I(0), Op(21), Op(14)]), 100);
    assert_eq!(w(&[I(5), I(0), Op(22), Op(14)]), 205);
    assert_eq!(w(&[I(5), I(0), Op(4), Op(14)]), 205);
    assert_eq!(w(&[I(5), I(0), I(10), Op(1), Op(14)]), 205);
    assert_eq!(w(&[I(0), I(10), Op(1), Op(14)]), 100);
    // CFF2 has no width: always defaultWidthX.
    let f2 = Font::new(Version::Cff2);
    let r = f2
        .decode(&charstring(&[I(5), I(0), I(0), Op(21)]))
        .0
        .unwrap();
    assert_eq!(r.width >> 16, 100);
}

#[test]
fn cff2_width_operand_is_not_taken() {
    let f2 = Font::new(Version::Cff2);
    let (r, out) = f2.decode(&charstring(&[I(1), I(2), I(3), Op(21), I(5), Op(6)]));
    r.unwrap();
    // rmoveto took the last two operands.
    assert_eq!(out, vec!["M", "L 2 3 7 3", "L 7 3 2 3"]);
}

#[test]
fn stems_and_masks() {
    // Two hstems and an implicit vstemhm of one stem: 3 hints, one mask
    // byte.
    let out = cff(&[
        I(0),
        I(10),
        I(20),
        I(10),
        Op(18),
        I(0),
        I(5),
        Op(19),
        Cs::Raw(vec![0xE0]),
        I(0),
        Op(22),
        I(4),
        Op(6),
        Op(14),
    ]);
    assert_eq!(out, vec!["M", "L 0 0 4 0", "L 4 0 0 0"]);
    // Nine hints: two mask bytes.
    let mut nine = Vec::new();
    for _ in 0..9 {
        nine.push(I(0));
        nine.push(I(1));
    }
    nine.push(Op(1));
    nine.push(Op(19));
    nine.push(Cs::Raw(vec![0xFF, 0x80]));
    nine.push(Op(14));
    cff(&nine);
    // A mask cut by the end.
    let mut cut = nine.clone();
    cut.truncate(nine.len() - 2);
    cut.push(Cs::Raw(vec![0xFF]));
    assert_eq!(cff_err(&cut), CharStringErrorKind::Truncated);
    // 97 stems.
    let mut many = Vec::new();
    for _ in 0..24 {
        for _ in 0..4 {
            many.push(I(1));
            many.push(I(1));
        }
        many.push(Op(1));
    }
    many.push(I(1));
    many.push(I(1));
    many.push(Op(1));
    many.push(Op(19));
    assert_eq!(cff_err(&many), CharStringErrorKind::TooManyHints);
    // 97 stems without a mask fail at the first move.
    many.pop();
    many.push(I(0));
    many.push(Op(22));
    assert_eq!(cff_err(&many), CharStringErrorKind::TooManyHints);
}

#[test]
fn zero_length_lines() {
    // A zero-length line is dropped; so is the closing line when the
    // path already ends at its start.
    assert_eq!(
        cff(&[I(0), Op(22), I(0), Op(6), I(5), Op(6), I(-5), Op(6), Op(14)]),
        vec!["M", "L 0 0 5 0", "L 5 0 0 0"]
    );
    // After a new hint mask, a zero-length line in character space is
    // kept (and dropped in device space).
    assert_eq!(
        cff(&[
            I(0),
            I(1),
            Op(18),
            I(0),
            Op(22),
            Op(19),
            Cs::Raw(vec![0x80]),
            I(0),
            Op(6),
            I(5),
            Op(6),
            Op(14)
        ]),
        vec!["M", "L 0 0 5 0", "L 5 0 0 0"]
    );
    // A move without lines makes no contour.
    assert_eq!(
        cff(&[I(0), Op(22), I(5), Op(22), I(1), Op(6), Op(14)]),
        vec!["M", "L 5 0 6 0", "L 6 0 5 0"]
    );
    // A line shorter than a device-space unit vanishes: 16/65536.
    assert_eq!(cff(&[I(0), Op(22), Cs::F(16), Op(6), Op(14)]), vec!["M"]);
}

#[test]
fn subroutines() {
    let mut f = Font::new(Version::Cff);
    // Local subr 0 (biased -107): a line; global subr 1 (-106) ends.
    f.lsubrs = cff_index(&[charstring(&[I(5), Op(6), Op(11)])], 1, false);
    f.gsubrs = cff_index(&[Vec::new(), charstring(&[Op(14)])], 1, false);
    let (r, out) = f.decode(&charstring(&[
        I(0),
        Op(22),
        I(-107),
        Op(10),
        I(-106),
        Op(29),
        I(9),
        Op(6),
    ]));
    r.unwrap();
    assert_eq!(out, vec!["M", "L 0 0 5 0", "L 5 0 0 0"]);
    // Out of range.
    assert_eq!(
        f.decode(&charstring(&[I(-105), Op(29)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::SubroutineOutOfRange
    );
    assert_eq!(
        f.decode(&charstring(&[I(0), Op(10)])).0.unwrap_err().kind,
        CharStringErrorKind::SubroutineOutOfRange
    );
    // A 16.16 subroutine number.
    assert_eq!(
        f.decode(&charstring(&[Cs::F(0), Op(10)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::TypeMismatch
    );
    // Nesting: a subroutine calling itself recurses until depth 16.
    f.gsubrs = cff_index(&[charstring(&[I(-107), Op(29)])], 1, false);
    assert_eq!(
        f.decode(&charstring(&[I(-107), Op(29)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::SubroutineTooDeep
    );
    // 16 nested calls are fine: a chain of 16 subroutines.
    let chain: Vec<Vec<u8>> = (0..16)
        .map(|i| {
            if i < 15 {
                charstring(&[I(-107 + i + 1), Op(29), Op(11)])
            } else {
                charstring(&[Op(11)])
            }
        })
        .collect();
    f.gsubrs = cff_index(&chain, 1, false);
    f.decode(&charstring(&[I(-107), Op(29), Op(14)])).0.unwrap();
    let chain17: Vec<Vec<u8>> = (0..17)
        .map(|i| {
            if i < 16 {
                charstring(&[I(-107 + i + 1), Op(29), Op(11)])
            } else {
                charstring(&[Op(11)])
            }
        })
        .collect();
    f.gsubrs = cff_index(&chain17, 1, false);
    assert_eq!(
        f.decode(&charstring(&[I(-107), Op(29), Op(14)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::SubroutineTooDeep
    );
    // The bias depends on the count: 1240 subroutines use 1131.
    let many: Vec<Vec<u8>> = (0..1240).map(|_| charstring(&[Op(11)])).collect();
    f.gsubrs = cff_index(&many, 2, false);
    f.decode(&charstring(&[I(-1131), Op(29), Op(14)]))
        .0
        .unwrap();
    assert!(f.decode(&charstring(&[I(1240 - 1131), Op(29)])).0.is_err());
    // return at the top.
    assert_eq!(
        cff_err(&[Op(11)]),
        CharStringErrorKind::ReturnOutsideSubroutine
    );
}

#[test]
fn stack_limits() {
    let mut items: Vec<Cs> = (0..48).map(|_| I(0)).collect();
    items.push(Op(14));
    // 48 operands then endchar is a seac attempt; drop instead.
    let _ = items;
    // 48 operands of hstem (24 stems) fit.
    let mut ok: Vec<Cs> = (0..48).map(|_| I(0)).collect();
    ok.push(Op(1));
    ok.push(Op(14));
    Font::new(Version::Cff).decode(&charstring(&ok)).0.unwrap();
    let mut over: Vec<Cs> = (0..49).map(|_| I(0)).collect();
    over.push(Op(14));
    assert_eq!(cff_err(&over), CharStringErrorKind::StackOverflow);
    let f2 = Font::new(Version::Cff2);
    let ok2: Vec<Cs> = (0..513).map(|_| I(0)).collect();
    f2.decode(&charstring(&ok2)).0.unwrap();
    let over2: Vec<Cs> = (0..514).map(|_| I(0)).collect();
    assert_eq!(
        f2.decode(&charstring(&over2)).0.unwrap_err().kind,
        CharStringErrorKind::StackOverflow
    );
    // Underflow.
    assert_eq!(cff_err(&[Op(21)]), CharStringErrorKind::StackUnderflow);
    // rlineto with an odd count reads past the stack.
    assert_eq!(
        cff_err(&[I(0), Op(22), I(1), Op(5)]),
        CharStringErrorKind::StackUnderflow
    );
}

#[test]
fn seac() {
    // Glyph 1 = 'A' (SID 34, code 65), glyph 2 = 'acute' (SID 125, code 194).
    let mut f = Font::new(Version::Cff);
    let base = charstring(&[I(0), Op(22), I(10), Op(6), Op(14)]);
    let accent = charstring(&[I(0), Op(22), I(0), I(3), Op(5), Op(14)]);
    f.char_strings = cff_index(&[Vec::new(), base, accent], 1, false);
    f.charset = Some(vec![0, 0, 0, 0, 0, 34, 0, 125]);
    let (r, out) = f.decode(&charstring(&[I(100), I(50), I(65), I(194), Op(14)]));
    r.unwrap();
    // The accent first, moved by (100, 50), then the base.
    assert_eq!(
        out,
        vec![
            "M",
            "L 100 50 100 53",
            "L 100 53 100 50",
            "M",
            "L 0 0 10 0",
            "L 10 0 0 0"
        ]
    );
    // A code without a glyph.
    assert_eq!(
        f.decode(&charstring(&[I(0), I(0), I(65), I(66), Op(14)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::SeacComponentMissing
    );
    // Nested seac.
    f.char_strings = cff_index(
        &[
            Vec::new(),
            charstring(&[I(0), I(0), I(65), I(194), Op(14)]),
            Vec::new(),
        ],
        1,
        false,
    );
    assert_eq!(
        f.decode(&charstring(&[I(0), I(0), I(65), I(194), Op(14)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::NestedSeac
    );
}

fn top_after(items: &[Cs]) -> Vec<String> {
    // Draw a vertical line of the value left on the stack.
    let mut all = vec![I(0), Op(22)];
    all.extend_from_slice(items);
    all.push(Op(7));
    all.push(Op(14));
    cff(&all)
}

#[test]
fn arithmetic() {
    assert_eq!(top_after(&[I(3), I(4), Op(0x0C0A)])[1], "L 0 0 0 7");
    assert_eq!(top_after(&[I(3), I(4), Op(0x0C0B)])[1], "L 0 0 0 -1");
    assert_eq!(top_after(&[I(6), I(4), Op(0x0C18)])[1], "L 0 0 0 24");
    assert_eq!(top_after(&[I(9), I(2), Op(0x0C0C)])[1], "L 0 0 0 4");
    assert_eq!(top_after(&[I(-9), Op(0x0C09)])[1], "L 0 0 0 9");
    assert_eq!(top_after(&[I(9), Op(0x0C0E)])[1], "L 0 0 0 -9");
    assert_eq!(top_after(&[I(16), Op(0x0C1A)])[1], "L 0 0 0 4");
    assert_eq!(top_after(&[I(1), I(1), Op(0x0C0F)])[1], "L 0 0 0 1");
    // and: 0, a zero-length line, so no contour at all.
    assert_eq!(top_after(&[I(1), I(0), Op(0x0C03)]).len(), 0);
    assert_eq!(top_after(&[I(1), I(0), Op(0x0C04)])[1], "L 0 0 0 1");
    assert_eq!(top_after(&[I(0), Op(0x0C05)])[1], "L 0 0 0 1");
    assert_eq!(top_after(&[I(5), I(7), Op(0x0C12)])[1], "L 0 0 0 5");
    assert_eq!(
        top_after(&[I(5), I(7), Op(0x0C1C), Op(0x0C12)])[1],
        "L 0 0 0 7"
    );
    assert_eq!(top_after(&[I(5), Op(0x0C1B), Op(0x0C0A)])[1], "L 0 0 0 10");
    assert_eq!(
        top_after(&[I(8), I(3), Op(0x0C14), I(3), Op(0x0C15)])[1],
        "L 0 0 0 8"
    );
    assert_eq!(
        top_after(&[I(1), I(2), I(3), I(4), Op(0x0C16)])[1],
        "L 0 0 0 1"
    );
    assert_eq!(
        top_after(&[I(1), I(2), I(4), I(3), Op(0x0C16)])[1],
        "L 0 0 0 2"
    );
    // index: 0 copies the top, 1 the one below.
    assert_eq!(
        top_after(&[I(9), I(0), Op(0x0C1D), Op(0x0C0A)])[1],
        "L 0 0 0 18"
    );
    assert_eq!(
        top_after(&[
            I(7),
            I(9),
            I(1),
            Op(0x0C1D),
            Op(0x0C0A),
            Op(0x0C1C),
            Op(0x0C12)
        ])[1],
        "L 0 0 0 16"
    );
    // roll: (1 2 3) 3 1 roll -> (3 1 2).
    assert_eq!(
        top_after(&[
            I(1),
            I(2),
            I(3),
            I(3),
            I(1),
            Op(0x0C1E),
            Op(0x0C12),
            Op(0x0C12)
        ])[1],
        "L 0 0 0 3"
    );
    // random: the same value at every decode.
    let f = Font::new(Version::Cff);
    let cs = charstring(&[
        I(0),
        Op(22),
        Op(0x0C17),
        Op(0x0C17),
        Op(0x0C0A),
        I(1000),
        Op(0x0C18),
        Op(7),
        Op(14),
    ]);
    assert_eq!(f.decode(&cs).1, f.decode(&cs).1);
    // In CFF2 the arithmetic operators are unknown: the stack is cleared.
    let f2 = Font::new(Version::Cff2);
    let (r, out) = f2.decode(&charstring(&[
        I(0),
        Op(22),
        I(3),
        I(4),
        Op(0x0C0A),
        I(2),
        Op(7),
    ]));
    r.unwrap();
    assert_eq!(out, vec!["M", "L 0 0 0 2", "L 0 2 0 0"]);
}

#[test]
fn cff2_operators() {
    let mut f = Font::new(Version::Cff2);
    // return and endchar are ignored (reserved): the stack is cleared.
    let (r, out) = f.decode(&charstring(&[I(0), Op(22), I(9), Op(14), I(2), Op(7)]));
    r.unwrap();
    assert_eq!(out, vec!["M", "L 0 0 0 2", "L 0 2 0 0"]);
    // blend without a store.
    assert_eq!(
        f.decode(&charstring(&[I(1), I(2), I(1), Op(16)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::BlendWithoutStore
    );
    f.store = Some(item_variation_store(
        1,
        &[vec![(0, 0, 0)], vec![(0, 0x4000, 0x4000)]],
        &[vec![0, 1]],
    ));
    // 10 + 4 * 0 + 6 * 0 with no fvar.
    let (r, out) = f.decode(&charstring(&[
        I(0),
        Op(22),
        I(10),
        I(4),
        I(6),
        I(1),
        Op(16),
        Op(6),
    ]));
    r.unwrap();
    assert_eq!(out[1], "L 0 0 10 0");
    // With fvar at 0: the peak-0 region weighs 1: 10 + 4.
    f.coords = vec![0];
    let (r, out) = f.decode(&charstring(&[
        I(0),
        Op(22),
        I(10),
        I(4),
        I(6),
        I(1),
        Op(16),
        Op(6),
    ]));
    r.unwrap();
    assert_eq!(out[1], "L 0 0 14 0");
    // Too few operands for the blend.
    assert_eq!(
        f.decode(&charstring(&[I(4), I(6), I(1), Op(16)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::StackUnderflow
    );
    // vsindex outside, and after blend.
    assert_eq!(
        f.decode(&charstring(&[I(1), Op(15), I(1), I(2), I(3), I(1), Op(16)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::VsindexOutOfRange
    );
    assert_eq!(
        f.decode(&charstring(&[I(1), I(2), I(3), I(1), Op(16), I(0), Op(15)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::VsindexAfterBlend
    );
    // fvar with another axis count.
    f.coords = vec![0, 0];
    assert_eq!(
        f.decode(&charstring(&[I(1), I(2), I(3), I(1), Op(16)]))
            .0
            .unwrap_err()
            .kind,
        CharStringErrorKind::VsindexOutOfRange
    );
}

#[test]
fn unknown_operators_and_endchar() {
    let f = Font::new(Version::Cff);
    let mut work = Work::default();
    let ch = Index::new(&f.char_strings, Version::Cff).unwrap();
    let g = Index::new(&f.gsubrs, Version::Cff).unwrap();
    let ctx = Context {
        version: Version::Cff,
        char_strings: ch,
        charset: None,
        global_subrs: g,
        global_subrs_sorted: true,
        local_subrs: g,
        local_subrs_sorted: true,
        charstring_type: 2,
        default_width_x: 0,
        nominal_width_x: 0,
        max_stack: 48,
        has_variations: false,
        vsindex: 0,
        blend: &NoBlend,
        random_seed: 1,
        scale: 0x400,
    };
    charstring::decode(&[2, 14], &ctx, &mut work, &mut NullSink).unwrap();
    assert_eq!(work.unknown_operator(), Some((0, 2)));
    assert!(!work.ended_without_endchar());
    charstring::decode(&[139], &ctx, &mut work, &mut NullSink).unwrap();
    assert!(work.ended_without_endchar());
    assert_eq!(work.unknown_operator(), None);
}

// ---------------------------------------------------------------- validators

fn diags(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    f(&mut |d| out.push(d));
    out
}

fn kinds(d: &[Diagnostic]) -> Vec<&'static str> {
    d.iter().map(|d| d.kind.id()).collect()
}

fn simple_cff() -> Cff1Spec {
    Cff1Spec::new(vec![
        charstring(&[Op(14)]),
        charstring(&[I(0), Op(22), I(5), Op(6), Op(14)]),
    ])
}

#[test]
fn valid_tables_have_no_diagnostics() {
    let t = cff1(&simple_cff());
    assert_eq!(diags(|r| sofel_read::cff::validate(&t, Some(2), r)), vec![]);
    let t2 = cff2(&Cff2Spec {
        charstrings: vec![Vec::new(), charstring(&[I(0), Op(22), I(5), Op(6)])],
        fds: vec![(Vec::new(), Vec::new())],
        ..Default::default()
    });
    assert_eq!(
        diags(|r| sofel_read::cff::validate2(&t2, Some(2), None, r)),
        vec![]
    );
}

#[test]
fn validator_header_and_index() {
    let t = cff1(&simple_cff());
    let mut bad = t.clone();
    bad[0] = 3;
    assert_eq!(
        kinds(&diags(|r| sofel_read::cff::validate(&bad, None, r))),
        vec!["unknown_version"]
    );
    let mut bad = t.clone();
    bad[2] = 2;
    let d = diags(|r| sofel_read::cff::validate(&bad, None, r));
    assert_eq!(kinds(&d), vec!["header_size_too_small"]);
    assert_eq!(d[0].offset, 2);
    assert_eq!(d[0].severity, Severity::Error);
    let mut bad = t.clone();
    bad[3] = 0;
    assert_eq!(
        kinds(&diags(|r| sofel_read::cff::validate(&bad, None, r))),
        vec!["invalid_off_size"]
    );
    // The Name INDEX's offset size 0.
    let mut bad = t.clone();
    bad[6] = 0;
    assert_eq!(
        kinds(&diags(|r| sofel_read::cff::validate(&bad, None, r))),
        vec!["invalid_off_size"]
    );
    // The Name INDEX's first offset 2.
    let mut bad = t.clone();
    bad[7] = 2;
    assert_eq!(
        kinds(&diags(|r| sofel_read::cff::validate(&bad, None, r))),
        vec!["first_offset_not_one"]
    );
    // Two names.
    let mut spec = simple_cff();
    spec.names.push(b"B".to_vec());
    let d = diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r));
    assert!(kinds(&d).contains(&"multiple_names"));
    assert!(kinds(&d).contains(&"too_few_top_dicts"));
}

#[test]
fn validator_dicts_and_glyphs() {
    let mut spec = simple_cff();
    spec.top = dict(&[
        (&[1], op::VERSION),
        (&[2], op::VERSION),
        (&[3], 0x0C27),
        (&[0], op::VSTORE),
        (&[1], op::CHARSTRING_TYPE),
    ]);
    let d = diags(|r| sofel_read::cff::validate(&cff1(&spec), Some(3), r));
    let k = kinds(&d);
    assert!(k.contains(&"duplicate_operator"), "{k:?}");
    assert!(k.contains(&"unknown_operator"), "{k:?}");
    assert!(k.contains(&"operator_not_allowed"), "{k:?}");
    assert!(k.contains(&"unsupported_charstring_type"), "{k:?}");
    assert!(k.contains(&"glyph_count_mismatch"), "{k:?}");
    let w = d
        .iter()
        .find(|d| d.kind.id() == "duplicate_operator")
        .unwrap();
    assert_eq!(w.severity, Severity::Warning);
    // A broken real.
    let mut spec = simple_cff();
    spec.top = vec![30, 0xD1, 0xFF, 0];
    assert!(
        kinds(&diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r)))
            .contains(&"invalid_real")
    );
    // Missing operands.
    let mut spec = simple_cff();
    spec.top = dict(&[(&[], op::NOTICE)]);
    assert!(
        kinds(&diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r)))
            .contains(&"dict_missing_operands")
    );
    // No glyphs.
    let mut spec = simple_cff();
    spec.charstrings.clear();
    assert!(
        kinds(&diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r))).contains(&"no_glyphs")
    );
    // A charset of an unknown format; a SID beyond the strings.
    let mut spec = simple_cff();
    spec.charset = Some(vec![7]);
    assert!(
        kinds(&diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r)))
            .contains(&"unknown_format")
    );
    let mut spec = simple_cff();
    spec.charset = Some(vec![0, 1, 200]);
    assert!(
        kinds(&diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r)))
            .contains(&"sid_out_of_range")
    );
    // An encoding of an unknown format.
    let mut spec = simple_cff();
    spec.encoding = Some(vec![5, 0]);
    assert!(
        kinds(&diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r)))
            .contains(&"unknown_format")
    );
}

#[test]
fn validator_charstrings() {
    let mut spec = simple_cff();
    spec.charstrings.push(charstring(&[Op(21)]));
    spec.charstrings.push(charstring(&[I(0), Op(10)]));
    spec.charstrings.push(charstring(&[I(0)]));
    spec.charstrings.push(charstring(&[Op(2), Op(14)]));
    let d = diags(|r| sofel_read::cff::validate(&cff1(&spec), None, r));
    let k = kinds(&d);
    assert_eq!(
        k,
        vec![
            "char_string_stack_underflow",
            "subroutine_out_of_range",
            "missing_endchar",
            "unknown_char_string_operator",
        ]
    );
    let mut details = Vec::new();
    d[0].kind.details(&mut |k, v| details.push((k, v)));
    assert_eq!(details[0], ("glyph", sofel_read::DetailValue::Uint(2)));
}

fn cid_spec(fd_select: Vec<u8>, fds: usize) -> Cff1Spec {
    let mut spec = simple_cff();
    spec.private = None;
    spec.cid = Some(CidSpec {
        ros: (391, 392, 0),
        fds: (0..fds)
            .map(|_| (Vec::new(), Vec::new(), Vec::new()))
            .collect(),
        fd_select,
    });
    spec.strings = vec![b"Adobe".to_vec(), b"Identity".to_vec()];
    spec
}

#[test]
fn validator_cid() {
    let ok = cff1(&cid_spec(vec![0, 0, 1], 2));
    assert_eq!(
        diags(|r| sofel_read::cff::validate(&ok, Some(2), r)),
        vec![]
    );
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(&cff1(&cid_spec(vec![0, 0, 2], 2)), None, r)
    }));
    assert_eq!(k, vec!["fd_out_of_range"]);
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(&cff1(&cid_spec(vec![3, 0, 1, 0, 1, 0, 0, 2], 2)), None, r)
    }));
    assert_eq!(k, vec!["first_range_not_zero"]);
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(&cff1(&cid_spec(vec![3, 0, 1, 0, 0, 0, 0, 3], 2)), None, r)
    }));
    assert_eq!(k, vec!["sentinel_mismatch"]);
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(
            &cff1(&cid_spec(vec![3, 0, 2, 0, 0, 0, 0, 0, 1, 0, 2], 2)),
            None,
            r,
        )
    }));
    assert_eq!(k, vec!["ranges_not_sorted"]);
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(
            &cff1(&cid_spec(
                vec![4, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
                1,
            )),
            None,
            r,
        )
    }));
    assert_eq!(k, vec!["fd_select_format4"]);
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(&cff1(&cid_spec(vec![9], 1)), None, r)
    }));
    assert_eq!(k, vec!["unknown_format"]);
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate(&cff1(&cid_spec(vec![0, 0, 0], 257)), None, r)
    }));
    assert_eq!(k, vec!["too_many_font_dicts"]);
}

#[test]
fn validator_cff2() {
    let base = Cff2Spec {
        charstrings: vec![Vec::new(), charstring(&[I(0), Op(22), I(5), Op(6)])],
        fds: vec![(Vec::new(), Vec::new())],
        ..Default::default()
    };
    let mut bad = cff2(&base);
    bad[2] = 4;
    assert_eq!(
        kinds(&diags(|r| sofel_read::cff::validate2(&bad, None, None, r))),
        vec!["header_size_too_small"]
    );
    let mut bad = cff2(&base);
    bad[3] = 0xFF;
    assert_eq!(
        kinds(&diags(|r| sofel_read::cff::validate2(&bad, None, None, r))),
        vec!["offset_out_of_bounds"]
    );
    // Variation Store: a region index outside, and an axis count unlike
    // fvar's.
    let mut spec = base.clone();
    spec.vstore = Some(item_variation_store(1, &[vec![(0, 0, 0)]], &[vec![3]]));
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate2(&cff2(&spec), None, Some(2), r)
    }));
    assert_eq!(k, vec!["axis_count_mismatch", "region_index_out_of_range"]);
    // blend without a store.
    let mut spec = base.clone();
    spec.charstrings
        .push(charstring(&[I(1), I(2), I(1), Op(16)]));
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate2(&cff2(&spec), None, None, r)
    }));
    assert_eq!(k, vec!["blend_without_store"]);
    // The FDSelect of a font with two Font DICTs.
    let mut spec = base.clone();
    spec.fds.push((Vec::new(), Vec::new()));
    spec.fd_select = vec![0, 0, 5];
    let k = kinds(&diags(|r| {
        sofel_read::cff::validate2(&cff2(&spec), None, None, r)
    }));
    assert_eq!(k, vec!["fd_out_of_range"]);
}

#[test]
fn inv1_fdselect_format4_severity() {
    let make = |sentinel: u32| {
        let mut fd_select = vec![4];
        fd_select.extend(2u32.to_be_bytes());
        fd_select.extend(0u32.to_be_bytes());
        fd_select.extend(0u16.to_be_bytes());
        fd_select.extend(1u32.to_be_bytes());
        fd_select.extend(1u16.to_be_bytes());
        fd_select.extend(sentinel.to_be_bytes());
        cff2(&Cff2Spec {
            charstrings: vec![
                Vec::new(),
                charstring(&[I(0), Op(22)]),
                charstring(&[I(0), Op(22)]),
            ],
            fds: vec![(Vec::new(), Vec::new()); 2],
            fd_select,
            ..Default::default()
        })
    };
    let valid = diags(|r| sofel_read::cff::validate2(&make(3), None, None, r));
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].kind, Kind::FdSelectFormat4);
    assert_eq!(valid[0].severity, Severity::Warning);

    let malformed = diags(|r| sofel_read::cff::validate2(&make(4), None, None, r));
    assert!(
        malformed
            .iter()
            .any(|d| d.kind == Kind::FdSelectFormat4 && d.severity == Severity::Warning)
    );
    assert!(malformed.iter().any(|d| d.severity == Severity::Error));
}

#[test]
fn validate_sfnt_runs_cff_validators() {
    let mut spec = simple_cff();
    spec.charstrings.push(charstring(&[Op(21)]));
    let font = otto_font(3, vec![(b"CFF ", cff1(&spec))]);
    let s = sofel_read::sfnt::Sfnt::new(&font).unwrap();
    let d = diags(|r| sofel_read::validate::sfnt(&s, r));
    assert!(
        d.iter().any(|d| d.kind
            == Kind::CharStringStackUnderflow {
                glyph: 2,
                position: 0
            }),
        "{d:?}"
    );
}
