//! HarfBuzz's CFF and CFF2 accelerators (ADR 0021): when they are valid
//! (`hb_valid`), and the CFF glyph names in HarfBuzz's order.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::read::GlyphId;
use sofel_font::{Font, HbCff1, HbCff2};

/// endchar.
fn empty_glyph() -> Vec<u8> {
    charstring(&[Cs::Op(14)])
}

fn glyphs(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|_| empty_glyph()).collect()
}

/// A non-CID CFF with three glyphs and a format 0 charset.
fn spec() -> Cff1Spec {
    let mut s = Cff1Spec::new(glyphs(3));
    s.charset = Some(vec![0, 0, 1, 0, 2]);
    s
}

fn otf(num_glyphs: u16, cff: Vec<u8>) -> Vec<u8> {
    otto_font(num_glyphs, vec![(b"CFF ", cff)])
}

fn hb1(data: &[u8]) -> bool {
    HbCff1::new(&Font::new(data, 0).unwrap()).is_some()
}

fn hb2(data: &[u8]) -> bool {
    HbCff2::new(&Font::new(data, 0).unwrap()).is_some()
}

/// A CID-keyed CFF with three glyphs, two Font DICTs and `fd_select`.
fn cid(fd_select: Vec<u8>) -> Cff1Spec {
    let mut s = spec();
    s.private = None;
    s.strings = vec![b"Adobe".to_vec(), b"Identity".to_vec()];
    s.cid = Some(CidSpec {
        ros: (391, 392, 0),
        fds: vec![
            (Vec::new(), Vec::new(), Vec::new()),
            (Vec::new(), Vec::new(), Vec::new()),
        ],
        fd_select,
    });
    s
}

#[test]
fn cff1_valid_and_each_failing_check() {
    assert!(hb1(&otf(3, cff1(&spec()))));
    // No table, an empty one.
    assert!(!hb1(&otto_font(3, vec![])));
    // Version 2 in CFF.
    let mut t = cff1(&spec());
    t[0] = 2;
    assert!(!hb1(&otf(3, t)));
    // A Name INDEX with offSize 5.
    let mut t = cff1(&spec());
    t[6] = 5;
    assert!(!hb1(&otf(3, t)));
    // CharStrings count other than maxp's.
    assert!(!hb1(&otf(4, cff1(&spec()))));
    // A Top DICT whose operator lacks its operand: CharStrings first.
    let mut s = spec();
    s.top = vec![17];
    assert!(!hb1(&otf(3, cff1(&s))));
    // A Top DICT number out of the int range (a real).
    let mut s = spec();
    s.top = [d_real("3E10"), vec![13]].concat();
    assert!(hb1(&otf(3, cff1(&s))), "UniqueID only clears the stack");
    let mut s = spec();
    s.top = [d_real("3E10"), d_op(0x0C22)].concat();
    assert!(!hb1(&otf(3, cff1(&s))), "CIDCount pops an int");
    // A malformed real.
    let mut s = spec();
    s.top = vec![30, 0xDF];
    assert!(!hb1(&otf(3, cff1(&s))));
    // 514 operands overflow the 513-entry stack.
    let mut s = spec();
    s.top = [vec![139u8; 514], vec![13]].concat();
    assert!(!hb1(&otf(3, cff1(&s))));
    let mut s = spec();
    s.top = [vec![139u8; 513], vec![13]].concat();
    assert!(hb1(&otf(3, cff1(&s))));
}

#[test]
fn cff1_charset() {
    // Predefined charsets need no data.
    let mut s = spec();
    s.charset = None;
    assert!(hb1(&otf(3, cff1(&s))));
    // Unknown format.
    let mut s = spec();
    s.charset = Some(vec![3, 0, 1, 0, 2]);
    assert!(!hb1(&otf(3, cff1(&s))));
    // Format 0 needs numGlyphs - 1 SIDs, here checked against the table end:
    // the charset is followed by the CharStrings, so make it the last thing
    // by giving the glyph count a large value through maxp is not possible;
    // a format 1 range that overshoots the glyphs is rejected (no trimming).
    let mut s = spec();
    s.charset = Some(vec![1, 0, 1, 5]);
    assert!(!hb1(&otf(3, cff1(&s))));
    let mut s = spec();
    s.charset = Some(vec![1, 0, 1, 1]);
    assert!(hb1(&otf(3, cff1(&s))));
    let mut s = spec();
    s.charset = Some(vec![2, 0, 1, 0, 1]);
    assert!(hb1(&otf(3, cff1(&s))));
    // Two ranges that cover exactly two glyphs.
    let mut s = spec();
    s.charset = Some(vec![1, 0, 1, 0, 0, 9, 0]);
    assert!(hb1(&otf(3, cff1(&s))));
}

#[test]
fn cff1_encoding() {
    let mut s = spec();
    s.encoding = Some(vec![0, 2, 65, 66]);
    assert!(hb1(&otf(3, cff1(&s))));
    let mut s = spec();
    s.encoding = Some(vec![1, 1, 65, 1]);
    assert!(hb1(&otf(3, cff1(&s))));
    let mut s = spec();
    s.encoding = Some(vec![2, 0]);
    assert!(!hb1(&otf(3, cff1(&s))));
    // A supplement after codes.
    let mut s = spec();
    s.encoding = Some(vec![0x80, 1, 65, 1, 66, 0, 1]);
    assert!(hb1(&otf(3, cff1(&s))));
    // A supplement after no codes is HarfBuzz's Null object.
    let mut s = spec();
    s.encoding = Some(vec![0x80, 0, 1, 66, 0, 1]);
    assert!(!hb1(&otf(3, cff1(&s))));
}

#[test]
fn cff1_private_and_subrs() {
    // No Private DICT at all (offset 0, size 0) is accepted.
    let mut s = spec();
    s.private = None;
    assert!(hb1(&otf(3, cff1(&s))));
    // A Private DICT past the table.
    let mut s = spec();
    s.private = None;
    s.top = [d_int5(10), d_int5(100_000), vec![18]].concat();
    assert!(!hb1(&otf(3, cff1(&s))));
    // A Private size from a real is truncated.
    let mut s = spec();
    s.private = None;
    s.top = [d_real("0.9"), d_int5(1), vec![18]].concat();
    assert!(hb1(&otf(3, cff1(&s))));
    // Broken local subroutines are left out, not an error.
    let mut s = spec();
    s.private = Some(dict(&[(&[30000], 19)]));
    assert!(hb1(&otf(3, cff1(&s))));
    let mut s = spec();
    s.lsubrs = vec![empty_glyph()];
    let t = cff1(&s);
    let f = otf(3, t);
    let font = Font::new(&f, 0).unwrap();
    assert_eq!(HbCff1::new(&font).unwrap().local_subrs(0).len(), 1);
}

#[test]
fn cff1_cid() {
    assert!(hb1(&otf(3, cff1(&cid(vec![0, 0, 1, 0])))));
    let font_data = otf(3, cff1(&cid(vec![3, 0, 2, 0, 0, 0, 0, 2, 1, 0, 3])));
    let font = Font::new(&font_data, 0).unwrap();
    let c = HbCff1::new(&font).unwrap();
    assert!(c.is_cid());
    assert_eq!((c.fd(0), c.fd(1), c.fd(2)), (0, 0, 1));
    assert_eq!(c.glyph_name(1), None);
    // Format 0 with an fd past the FDArray is accepted.
    assert!(hb1(&otf(3, cff1(&cid(vec![0, 0, 1, 7])))));
    // Format 3: a range fd past the FDArray, a sentinel other than the
    // glyph count, a first range not at 0, ranges out of order.
    assert!(!hb1(&otf(3, cff1(&cid(vec![3, 0, 1, 0, 0, 2, 0, 3])))));
    assert!(!hb1(&otf(3, cff1(&cid(vec![3, 0, 1, 0, 0, 0, 0, 4])))));
    assert!(!hb1(&otf(3, cff1(&cid(vec![3, 0, 1, 0, 1, 0, 0, 3])))));
    assert!(!hb1(&otf(
        3,
        cff1(&cid(vec![3, 0, 2, 0, 0, 0, 0, 0, 0, 1, 0, 3]))
    )));
    // Format 4 is CFF2's.
    assert!(!hb1(&otf(
        3,
        cff1(&cid(vec![4, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3]))
    )));
    // A CID-keyed font with a predefined charset.
    let mut s = cid(vec![0, 0, 1, 0]);
    s.charset = None;
    assert!(!hb1(&otf(3, cff1(&s))));
}

fn cff2_spec(fds: usize) -> Cff2Spec {
    Cff2Spec {
        charstrings: glyphs(3),
        fds: (0..fds).map(|_| (Vec::new(), Vec::new())).collect(),
        fd_select: vec![3, 0, 1, 0, 0, 0, 0, 3],
        ..Default::default()
    }
}

fn otf2(num_glyphs: u16, cff: Vec<u8>, extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    let mut tables = vec![(b"CFF2", cff)];
    tables.extend(extra);
    otto_font(num_glyphs, tables)
}

#[test]
fn cff2_valid_and_each_failing_check() {
    assert!(hb2(&otf2(3, cff2(&cff2_spec(1)), vec![])));
    assert!(!hb2(&otto_font(3, vec![])));
    let mut t = cff2(&cff2_spec(1));
    t[0] = 1;
    assert!(!hb2(&otf2(3, t, vec![])));
    // Glyph counts: more glyphs in maxp are allowed only with VARC.
    assert!(!hb2(&otf2(4, cff2(&cff2_spec(1)), vec![])));
    assert!(hb2(&otf2(
        4,
        cff2(&cff2_spec(1)),
        vec![(b"VARC", vec![0; 4])]
    )));
    assert!(!hb2(&otf2(
        2,
        cff2(&cff2_spec(1)),
        vec![(b"VARC", vec![0; 4])]
    )));
    // FDSelect format 4 (which FreeType cannot read) is accepted.
    let mut s = cff2_spec(2);
    s.fd_select = vec![
        4, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 3,
    ];
    let f = otf2(3, cff2(&s), vec![]);
    let font = Font::new(&f, 0).unwrap();
    let c = HbCff2::new(&font).unwrap();
    assert_eq!((c.fd(0), c.fd(1), c.fd(2)), (0, 1, 1));
    // A broken FDSelect fails.
    let mut s = cff2_spec(2);
    s.fd_select = vec![3, 0, 1, 0, 0, 0, 0, 4];
    assert!(!hb2(&otf2(3, cff2(&s), vec![])));
    // vsindex in the Private DICT.
    let mut s = cff2_spec(1);
    s.fds = vec![(dict(&[(&[1], 22)]), Vec::new())];
    let f = otf2(3, cff2(&s), vec![]);
    let font = Font::new(&f, 0).unwrap();
    assert_eq!(HbCff2::new(&font).unwrap().private(0).1, 1);
    // A Private DICT blend is cleared, not an error.
    let mut s = cff2_spec(1);
    s.fds = vec![(dict(&[(&[1, 2, 3], 23), (&[5], 6)]), Vec::new())];
    assert!(hb2(&otf2(3, cff2(&s), vec![])));
}

#[test]
fn cff2_variation_store() {
    let vs = item_variation_store(
        1,
        &[vec![(0, 1, 1)], vec![(-1, -1, 0)]],
        &[vec![0, 1], vec![1]],
    );
    let mut s = cff2_spec(1);
    s.vstore = Some(vs);
    let f = otf2(3, cff2(&s), vec![]);
    let font = Font::new(&f, 0).unwrap();
    let c = HbCff2::new(&font).unwrap();
    assert_eq!(
        (c.region_count(0), c.region_count(1), c.region_count(2)),
        (2, 1, 0)
    );
    // A broken store is left out.
    let mut s = cff2_spec(1);
    s.vstore = Some(vec![0, 2, 0, 0]);
    let f = otf2(3, cff2(&s), vec![]);
    let font = Font::new(&f, 0).unwrap();
    assert_eq!(HbCff2::new(&font).unwrap().region_count(0), 0);
}

/// A CFF whose glyphs are named by `sids` (glyph 0 is .notdef), with
/// `strings` in the String INDEX.
fn named(sids: &[u16], strings: &[&str]) -> Vec<u8> {
    let mut s = Cff1Spec::new(glyphs(sids.len() + 1));
    let mut charset = vec![0u8];
    for sid in sids {
        charset.extend(sid.to_be_bytes());
    }
    s.charset = Some(charset);
    s.strings = strings.iter().map(|x| x.as_bytes().to_vec()).collect();
    otf(sids.len() as u16 + 1, cff1(&s))
}

#[test]
fn cff_glyph_names_in_harfbuzz_order() {
    // 34 is "A" (standard), 391 and 392 are strings; 393 is past them.
    let f = named(&[34, 391, 392, 393], &["foo", ""]);
    let font = Font::new(&f, 0).unwrap();
    assert_eq!(font.glyph_name(GlyphId(0)), Some(&b".notdef"[..]));
    assert_eq!(font.glyph_name(GlyphId(1)), Some(&b"A"[..]));
    assert_eq!(font.glyph_name(GlyphId(2)), Some(&b"foo"[..]));
    // An empty string and a missing one are no name.
    assert_eq!(font.glyph_name(GlyphId(3)), None);
    assert_eq!(font.glyph_name(GlyphId(4)), None);
    assert_eq!(font.glyph_name(GlyphId(5)), None);
    let mut buf = vec![0u32; font.glyph_names_len()];
    assert_eq!(buf.len(), 1 + 2 * 5);
    let names = font.glyph_names(&mut buf).unwrap();
    let cff = HbCff1::new(&font);
    for g in 0..6 {
        assert_eq!(names.glyph_name(GlyphId(g)), font.glyph_name(GlyphId(g)));
        assert_eq!(
            font.glyph_name_with(GlyphId(g), cff.as_ref()),
            font.glyph_name(GlyphId(g))
        );
    }
    assert_eq!(font.glyph_name_with(GlyphId(1), None), None);
    assert_eq!(names.glyph_from_name(b"foo"), Some(GlyphId(2)));
    assert_eq!(names.glyph_from_name(b"A"), Some(GlyphId(1)));
    assert_eq!(names.glyph_from_name(b".notdef"), Some(GlyphId(0)));
    assert_eq!(names.glyph_from_name(b"B"), None);
    assert_eq!(names.glyph_from_name(b""), None);
    let names = font.glyph_names_from(&buf).unwrap();
    assert_eq!(names.glyph_from_name(b"foo"), Some(GlyphId(2)));
}

#[test]
fn cff_duplicate_names_give_the_smaller_glyph() {
    // Glyphs 1 and 3 have the same SID; glyphs 2 and 4 have different SIDs
    // with the same string.
    let f = named(&[34, 391, 34, 392], &["A", "A"]);
    let font = Font::new(&f, 0).unwrap();
    let mut buf = vec![0u32; font.glyph_names_len()];
    let names = font.glyph_names(&mut buf).unwrap();
    assert_eq!(names.glyph_from_name(b"A"), Some(GlyphId(1)));
    // Equal names sort by glyph id: the first one wins, through its SID.
    let f = named(&[391, 34, 34], &["A"]);
    let font = Font::new(&f, 0).unwrap();
    let mut buf = vec![0u32; font.glyph_names_len()];
    let names = font.glyph_names(&mut buf).unwrap();
    assert_eq!(names.glyph_from_name(b"A"), Some(GlyphId(1)));
}

#[test]
fn cff_names_short_buffer_and_post_first() {
    let f = named(&[34], &[]);
    let font = Font::new(&f, 0).unwrap();
    let mut buf = vec![0u32; font.glyph_names_len() - 1];
    assert!(font.glyph_names(&mut buf).is_none());
    assert!(font.glyph_names_from(&buf).is_none());
    // CID-keyed fonts have no names.
    let f = otf(3, cff1(&cid(vec![0, 0, 1, 0])));
    let font = Font::new(&f, 0).unwrap();
    assert_eq!(font.glyph_name(GlyphId(1)), None);
    assert_eq!(font.glyph_names_len(), 1);
    // post names come first.
    let mut tables = vec![(b"CFF " as &[u8; 4], {
        let mut s = Cff1Spec::new(glyphs(2));
        s.charset = Some(vec![0, 0, 34]);
        cff1(&s)
    })];
    tables.push((b"post", post2(&[0, 258], &["Z"])));
    let f = otto_font(2, tables);
    let font = Font::new(&f, 0).unwrap();
    assert_eq!(font.glyph_name(GlyphId(1)), Some(&b"Z"[..]));
    let mut buf = vec![0u32; font.glyph_names_len()];
    let names = font.glyph_names(&mut buf).unwrap();
    assert_eq!(names.glyph_from_name(b"Z"), Some(GlyphId(1)));
    // The CFF name of glyph 1 is still found by name.
    assert_eq!(names.glyph_from_name(b"A"), Some(GlyphId(1)));
}
