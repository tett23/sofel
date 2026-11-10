//! CFF and CFF2 read as HarfBuzz reads them (ADR 0021): the sanitizer,
//! the INDEX, charset, encoding, FDSelect and Item Variation Store
//! checks, and the DICT interpreters.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;

use sofel_read::cff::Version;
use sofel_read::cff::hb::{
    DictKind, FdSelectCounts, HbCharset, HbDict, HbFdSelect, HbIndex, Sanitizer, UNDEF_SID,
    interpret_dict, sanitize_cff2_var_store, sanitize_charset, sanitize_encoding,
    var_store_region_count,
};
use support::build::*;

#[test]
fn sanitizer_ranges_and_budget() {
    let mut sc = Sanitizer::new(10);
    assert!(sc.check_point(0) && sc.check_point(10));
    assert!(!sc.check_point(-1) && !sc.check_point(11));
    assert!(sc.check_range(2, 8));
    assert!(!sc.check_range(2, 9));
    assert!(!sc.check_array(0, u32::MAX, 2), "overflowing product");
    assert_eq!(sc.struct_at(4, 0), None, "offset 0 is Null");
    assert_eq!(sc.struct_at(4, -4), Some(0));
    assert_eq!(sc.struct_at(4, -5), None);
    // The budget is 64 per byte, at least 16384: 16384 for 10 bytes.
    let mut sc = Sanitizer::new(10);
    for _ in 0..1638 {
        assert!(sc.check_range(0, 10));
    }
    assert!(!sc.check_range(0, 4), "16384 - 16380 - 4 is not above 0");
    assert!(sc.exhausted());
    assert!(!sc.check_range(0, 0), "an empty range fails once spent");
    let mut sc = Sanitizer::unlimited(10);
    for _ in 0..100_000 {
        assert!(sc.check_range(0, 10));
    }
}

#[test]
fn index_reads_like_harfbuzz() {
    let data = cff_index(&[b"ab".to_vec(), Vec::new(), b"xyz".to_vec()], 1, false);
    let mut sc = Sanitizer::new(data.len());
    let i = HbIndex::sanitize(&data, 0, Version::Cff, &mut sc).unwrap();
    assert_eq!(i.len(), 3);
    assert_eq!(i.get(0), Some(&b"ab"[..]));
    assert_eq!(i.get(1), Some(&b""[..]));
    assert_eq!(i.get(2), Some(&b"xyz"[..]));
    assert_eq!(i.get(3), None);
    assert_eq!(i.get_at(2).unwrap().0, 2 + 1 + 4 + 2);
    // count + offSize + offsets + data.
    assert_eq!(i.size_offset() as usize, data.len());
    // Empty INDEXes: CFF 2 bytes, CFF2 4.
    let e = cff_index(&[], 1, true);
    let mut sc = Sanitizer::new(e.len());
    let i = HbIndex::sanitize(&e, 0, Version::Cff2, &mut sc).unwrap();
    assert_eq!((i.len(), i.size_offset()), (0, 4));
    // Offset size 0 and 5, data past the end.
    for bad in [0u8, 5] {
        let mut d = data.clone();
        d[2] = bad;
        let mut sc = Sanitizer::new(d.len());
        assert!(HbIndex::sanitize(&d, 0, Version::Cff, &mut sc).is_none());
    }
    let short = &data[..data.len() - 1];
    let mut sc = Sanitizer::new(short.len());
    assert!(HbIndex::sanitize(short, 0, Version::Cff, &mut sc).is_none());
    // Reversed offsets: the object is HarfBuzz's empty string (None), the
    // INDEX itself is accepted.
    let mut d = data.clone();
    d[3] = 4;
    let mut sc = Sanitizer::new(d.len());
    let i = HbIndex::sanitize(&d, 0, Version::Cff, &mut sc).unwrap();
    assert_eq!(i.get(0), None);
    assert_eq!(i.get(1), Some(&b""[..]));
    // An offset of 0 starts one byte early, at the last offset.
    let mut d = data.clone();
    d[3] = 0;
    let mut sc = Sanitizer::new(d.len());
    let i = HbIndex::sanitize(&d, 0, Version::Cff, &mut sc).unwrap();
    assert_eq!(i.get(0), Some(&[6u8, 0x61, 0x62][..]));
    // A last offset of 0: get_size wraps to the start of the data.
    let d = [0u8, 1, 1, 0, 0];
    let mut sc = Sanitizer::new(d.len());
    let i = HbIndex::sanitize(&d, 0, Version::Cff, &mut sc).unwrap();
    assert_eq!(i.size_offset(), 4);
    // A count of 0xFFFFFFFF in CFF2 is rejected.
    let d = [0xFFu8, 0xFF, 0xFF, 0xFF, 1];
    let mut sc = Sanitizer::new(d.len());
    assert!(HbIndex::sanitize(&d, 0, Version::Cff2, &mut sc).is_none());
}

fn dict_of(bytes: &[u8], kind: DictKind) -> Option<HbDict> {
    interpret_dict(bytes, kind)
}

#[test]
fn dict_values_and_errors() {
    let top = [
        dict(&[(&[100], 15), (&[200], 16), (&[300], 17), (&[40, 500], 18)]),
        d_int5(391),
        d_int5(392),
        d_real("1.5"),
        vec![12, 30],
        dict(&[(&[600], 0x0C24), (&[700], 0x0C25)]),
    ]
    .concat();
    let d = dict_of(&top, DictKind::Cff1Top).unwrap();
    assert_eq!(
        (
            d.charset,
            d.encoding,
            d.char_strings,
            d.private_size,
            d.private_offset
        ),
        (100, 200, 300, 40, 500)
    );
    assert_eq!((d.registry, d.fd_array, d.fd_select), (391, 600, 700));
    let d = dict_of(&[], DictKind::Cff1Top).unwrap();
    assert_eq!(d, HbDict::default());
    assert_eq!(d.registry, UNDEF_SID);
    // The last value wins.
    let d = dict_of(&dict(&[(&[1], 17), (&[2], 17)]), DictKind::Cff1Top).unwrap();
    assert_eq!(d.char_strings, 2);
    // Underflow, a negative uint, a malformed real, an unknown operator.
    assert!(dict_of(&[17], DictKind::Cff1Top).is_none());
    assert!(dict_of(&dict(&[(&[-1, 5], 18)]), DictKind::Cff1Top).is_none());
    assert!(dict_of(&dict(&[(&[5, -1], 18)]), DictKind::Cff1Top).is_some());
    assert!(dict_of(&[30, 0x1D], DictKind::Cff1Top).is_none());
    assert!(dict_of(&dict(&[(&[1, 2], 25)]), DictKind::Cff1Top).is_some());
    // Truncated numbers.
    assert!(dict_of(&[28, 1], DictKind::Cff1Top).is_none());
    assert!(dict_of(&[29, 1, 2, 3], DictKind::Cff1Top).is_none());
    assert!(dict_of(&[247], DictKind::Cff1Top).is_none());
    // Private DICTs: Subrs; CFF2's vsindex (first one only) and blend.
    let d = dict_of(&dict(&[(&[123], 19), (&[1, 2], 6)]), DictKind::Cff1Private).unwrap();
    assert_eq!(d.subrs, 123);
    let p = dict(&[(&[2], 22), (&[3], 22), (&[1, 2, 3], 23), (&[9], 19)]);
    let d = dict_of(&p, DictKind::Cff2Private).unwrap();
    assert_eq!((d.ivs, d.subrs), (2, 9));
    // In CFF, 22 and 23 are unknown operators.
    let d = dict_of(&dict(&[(&[2], 22)]), DictKind::Cff1Private).unwrap();
    assert_eq!(d.ivs, 0);
    // CFF2 Top and Font DICTs.
    let t = dict(&[(&[10], 17), (&[20], 0x0C24), (&[30], 0x0C25), (&[40], 24)]);
    let d = dict_of(&t, DictKind::Cff2Top).unwrap();
    assert_eq!(
        (d.char_strings, d.fd_array, d.fd_select, d.vstore),
        (10, 20, 30, 40)
    );
    let d = dict_of(&dict(&[(&[5, 60], 18)]), DictKind::Cff2Font).unwrap();
    assert_eq!((d.private_size, d.private_offset), (5, 60));
}

#[test]
fn dict_reals_truncate_toward_zero() {
    let size = |r: &str| {
        let b = [d_real(r), d_int5(1), vec![18]].concat();
        dict_of(&b, DictKind::Cff1Top).map(|d| d.private_size)
    };
    assert_eq!(size("2.9"), Some(2));
    assert_eq!(size("-0.9"), Some(0));
    assert_eq!(size("1E3"), Some(1000));
    assert_eq!(size("25E-1"), Some(2));
    assert_eq!(size("-1.5"), None);
    assert_eq!(size("3E9"), None, "past the int range");
    assert_eq!(size(".5"), Some(0));
    // A dangling exponent: the length does not match.
    assert_eq!(dict_of(&[30, 0x1B, 0xFF, 1, 18], DictKind::Cff1Top), None);
    // 32 characters without an end.
    let mut b = vec![30];
    b.extend([0x11; 16]);
    b.extend(d_int5(1));
    b.push(18);
    assert_eq!(dict_of(&b, DictKind::Cff1Top), None);
}

#[test]
fn charsets_like_harfbuzz() {
    // Format 0: three glyphs.
    let d = [0u8, 0, 5, 1, 0x87];
    let mut sc = Sanitizer::new(d.len());
    let c = sanitize_charset(&d, 0, 3, &mut sc).unwrap();
    assert_eq!(
        (c.sid(0, 3), c.sid(1, 3), c.sid(2, 3), c.sid(3, 3)),
        (0, 5, 391, 0)
    );
    assert_eq!((c.glyph(391, 3), c.glyph(0, 3), c.glyph(6, 3)), (2, 0, 0));
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_charset(&d, 0, 4, &mut sc).is_none());
    // Formats 1 and 2 cover the glyphs exactly; no trimming.
    let d = [1u8, 0, 10, 1, 0, 50, 0];
    let mut sc = Sanitizer::new(d.len());
    let c = sanitize_charset(&d, 0, 4, &mut sc).unwrap();
    assert_eq!((c.sid(1, 4), c.sid(2, 4), c.sid(3, 4)), (10, 11, 50));
    assert_eq!((c.glyph(11, 4), c.glyph(50, 4), c.glyph(12, 4)), (2, 3, 0));
    // The first range alone covers two glyphs; the next is not read.
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_charset(&d, 0, 3, &mut sc).is_some());
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_charset(&d, 0, 5, &mut sc).is_none());
    let d = [2u8, 0, 10, 0, 2];
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_charset(&d, 0, 4, &mut sc).is_some());
    // No glyphs at all: numGlyphs - 1 wraps and fails.
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_charset(&d, 0, 0, &mut sc).is_none());
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_charset(&[3u8, 0], 0, 1, &mut sc).is_none());
    // Predefined: ISOAdobe up to 228, Expert, ExpertSubset, others empty.
    let iso = HbCharset::Predefined(0);
    assert_eq!(
        (iso.sid(228, 300), iso.sid(229, 300), iso.glyph(34, 300)),
        (228, 0, 34)
    );
    let expert = HbCharset::Predefined(1);
    assert_eq!((expert.sid(1, 300), expert.glyph(229, 300)), (1, 2));
    assert_eq!(HbCharset::Predefined(-1).sid(3, 300), 0);
    // seac codes: 65 is "A" (SID 34), 193 "grave" (SID 124).
    assert_eq!(iso.std_code_to_glyph(65, 300), 34);
    assert_eq!(iso.std_code_to_glyph(0, 300), 0);
    assert_eq!(iso.std_code_to_glyph(300, 300), 0);
    assert_eq!(expert.std_code_to_glyph(65, 300), 0);
    let d = [0u8, 0, 34];
    let mut sc = Sanitizer::new(d.len());
    let c = sanitize_charset(&d, 0, 2, &mut sc).unwrap();
    assert_eq!(
        (c.std_code_to_glyph(65, 2), c.std_code_to_glyph(66, 2)),
        (1, 0)
    );
}

#[test]
fn encodings_like_harfbuzz() {
    let ok = |d: &[u8]| {
        let mut sc = Sanitizer::new(d.len());
        sanitize_encoding(d, 0, &mut sc)
    };
    assert!(ok(&[0, 2, 65, 66]));
    assert!(!ok(&[0, 2, 65]));
    assert!(ok(&[1, 1, 65, 3]));
    assert!(!ok(&[2, 0]));
    assert!(ok(&[0x80, 1, 65, 1, 66, 0, 1]));
    assert!(!ok(&[0x80, 1, 65, 1, 66, 0]));
    assert!(!ok(&[0x80, 0, 1, 66, 0, 1]), "a supplement after no codes");
}

#[test]
fn fd_selects_like_harfbuzz() {
    let counts = |fd_count, glyphs, face_glyphs| FdSelectCounts {
        fd_count,
        glyphs,
        face_glyphs,
    };
    let sel = |d: &[u8], v: Version, c: FdSelectCounts| {
        let mut sc = Sanitizer::new(d.len());
        HbFdSelect::sanitize(d, 0, v, c, &mut sc)
            .map(|s| (0..4).map(|g| s.fd(g)).collect::<Vec<_>>())
    };
    // Format 0 does not check the Font DICTs.
    assert_eq!(
        sel(&[0, 1, 9, 0], Version::Cff, counts(2, 3, 3)),
        Some(vec![1, 9, 0, 0])
    );
    assert_eq!(sel(&[0, 1, 9], Version::Cff, counts(2, 3, 3)), None);
    // Format 3.
    let f3 = [3u8, 0, 2, 0, 0, 1, 0, 2, 0, 0, 4];
    assert_eq!(
        sel(&f3, Version::Cff, counts(2, 4, 4)),
        Some(vec![1, 1, 0, 0])
    );
    assert_eq!(
        sel(&f3, Version::Cff, counts(1, 4, 4)),
        None,
        "fd past the FDArray"
    );
    assert_eq!(
        sel(&f3, Version::Cff, counts(2, 4, 5)),
        Some(vec![1, 1, 0, 0]),
        "CFF2 rule"
    );
    assert_eq!(
        sel(&f3, Version::Cff, counts(2, 5, 5)),
        None,
        "sentinel below the glyphs"
    );
    assert_eq!(
        sel(&f3, Version::Cff, counts(2, 3, 3)),
        None,
        "sentinel past the face"
    );
    assert_eq!(
        sel(&[3, 0, 0, 0, 0], Version::Cff, counts(2, 0, 0)),
        None,
        "no ranges"
    );
    // Format 4 only in CFF2.
    let f4 = [4u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 3];
    assert_eq!(
        sel(&f4, Version::Cff2, counts(2, 3, 3)),
        Some(vec![1, 1, 1, 1])
    );
    assert_eq!(sel(&f4, Version::Cff, counts(2, 3, 3)), None);
    // CFF2's FDSelect is at least 2 bytes long.
    assert_eq!(sel(&[0], Version::Cff2, counts(1, 0, 0)), None);
    assert_eq!(
        sel(&[0], Version::Cff, counts(1, 0, 0)),
        Some(vec![0, 0, 0, 0])
    );
    assert_eq!(HbFdSelect::null().fd(5), 0);
}

#[test]
fn cff2_variation_stores_like_harfbuzz() {
    let vs = item_variation_store(2, &[vec![(0, 1, 1), (0, 0, 0)]], &[vec![0], vec![0, 0]]);
    let mut d = (vs.len() as u16).to_be_bytes().to_vec();
    d.extend(&vs);
    let mut sc = Sanitizer::new(d.len());
    assert!(sanitize_cff2_var_store(&d, 0, &mut sc));
    assert_eq!(
        (0..3)
            .map(|i| var_store_region_count(&d, 0, i))
            .collect::<Vec<_>>(),
        vec![1, 2, 0]
    );
    // The length must fit; the format must be 1.
    let mut bad = d.clone();
    bad[1] = bad[1].wrapping_add(1);
    let mut sc = Sanitizer::new(bad.len());
    assert!(!sanitize_cff2_var_store(&bad, 0, &mut sc));
    let mut bad = d.clone();
    bad[3] = 2;
    let mut sc = Sanitizer::new(bad.len());
    assert!(!sanitize_cff2_var_store(&bad, 0, &mut sc));
    // A region list past the table.
    let mut bad = d.clone();
    bad[4..8].copy_from_slice(&1000u32.to_be_bytes());
    let mut sc = Sanitizer::new(bad.len());
    assert!(!sanitize_cff2_var_store(&bad, 0, &mut sc));
}
