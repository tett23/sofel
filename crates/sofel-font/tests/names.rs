//! Names, glyph names and FreeType's face values (ADR 0016).

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
use sofel_font::{Font, FtFaceMetrics, NameLength};

fn font_with(replace: &[(&'static [u8; 4], Vec<u8>)], remove: &[&[u8; 4]]) -> Vec<u8> {
    let mut tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(t, _)| !replace.iter().any(|(r, _)| r == t) && !remove.contains(t))
        .collect();
    tables.extend(replace.iter().cloned());
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

fn ps_name(records: &[NameRec<'_>]) -> Option<String> {
    let data = font_with(&[(b"name", name(0, records, &[]))], &[]);
    let f = Font::new(&data, 0).unwrap();
    let mut out = [0u8; 127];
    f.postscript_name(&mut out)
        .map(|n| String::from_utf8(out[..n.min(127)].to_vec()).unwrap())
}

// ------------------------------------------------------- postscript_name

#[test]
fn postscript_name_prefers_windows_english() {
    let jp = utf16be("JpName");
    let en = utf16be("EnName");
    assert_eq!(
        ps_name(&[(3, 1, 0x411, 6, &jp), (3, 1, 0x409, 6, &en)]).as_deref(),
        Some("EnName")
    );
    // Without 0x409, the first Windows record.
    assert_eq!(
        ps_name(&[(3, 1, 0x411, 6, &jp), (3, 1, 0x40C, 6, &en)]).as_deref(),
        Some("JpName")
    );
    // Windows before Apple.
    assert_eq!(
        ps_name(&[(1, 0, 0, 6, b"MacName"), (3, 1, 0x411, 6, &jp)]).as_deref(),
        Some("JpName")
    );
}

#[test]
fn postscript_name_from_apple_only() {
    assert_eq!(
        ps_name(&[(1, 0, 0, 6, b"MacName"), (1, 0, 0, 1, b"Family")]).as_deref(),
        Some("MacName")
    );
}

#[test]
fn postscript_name_drops_bad_characters() {
    let mixed = utf16be("Sofel Bold[x]/é");
    assert_eq!(
        ps_name(&[(3, 1, 0x409, 6, &mixed)]).as_deref(),
        Some("SofelBoldx")
    );
    // Nothing usable in the Windows record: the Apple one.
    let bad = utf16be("  ()");
    assert_eq!(
        ps_name(&[(1, 0, 0, 6, b"Mac Name"), (3, 1, 0x409, 6, &bad)]).as_deref(),
        Some("MacName")
    );
    assert_eq!(ps_name(&[(3, 1, 0x409, 6, &bad)]), None);
}

#[test]
fn postscript_name_needs_name_id_6() {
    assert_eq!(ps_name(&[(3, 1, 0x409, 1, &utf16be("Family"))]), None);
    let data = font_with(&[], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().postscript_name(&mut [0; 8]),
        None
    );
}

#[test]
fn postscript_name_is_cut_to_the_buffer() {
    let long = utf16be("ABCDEFGHIJ");
    let data = font_with(&[(b"name", name(0, &[(3, 1, 0x409, 6, &long)], &[]))], &[]);
    let f = Font::new(&data, 0).unwrap();
    let mut out = [0u8; 4];
    assert_eq!(f.postscript_name(&mut out), Some(10));
    assert_eq!(&out, b"ABCD");
}

// --------------------------------------------------------- HarfBuzz names

#[test]
fn names_pick_the_best_record_per_language() {
    let fr = utf16be("Famille");
    let en = utf16be("Family");
    let records: Vec<NameRec<'_>> = vec![
        (1, 0, 0, 1, b"MacFamily"),
        (3, 1, 0x409, 1, &en),
        (3, 1, 0x40C, 1, &fr),
        (3, 1, 0x409, 2, &en),
        (7, 0, 0, 3, b"unsupported"),
    ];
    let data = font_with(&[(b"name", name(0, &records, &[]))], &[]);
    let f = Font::new(&data, 0).unwrap();
    let list: Vec<(u16, String, u16)> = f
        .names()
        .map(|e| {
            let mut b = [0u8; 16];
            let n = e.language.write(&mut b);
            (
                e.name_id,
                String::from_utf8(b[..n].to_vec()).unwrap(),
                e.index,
            )
        })
        .collect();
    // (3, 1) scores better than (1, 0) for English.
    assert_eq!(
        list,
        vec![
            (1, "en".to_string(), 1),
            (1, "fr".to_string(), 2),
            (2, "en".to_string(), 3)
        ]
    );
    let mut out = [0u8; 32];
    let n = f.name_utf8(1, Some(b"fr"), &mut out);
    assert_eq!(
        n,
        NameLength {
            written: 7,
            total: 7
        }
    );
    assert_eq!(&out[..8], b"Famille\0");
    // A more specific request matches the font's language.
    let n = f.name_utf8(1, Some(b"FR_ca"), &mut out);
    assert_eq!(&out[..n.written], b"Famille");
    // No language: English.
    let n = f.name_utf8(1, None, &mut out);
    assert_eq!(&out[..n.written], b"Family");
    // Cut to the buffer, keeping room for the 0.
    let mut small = [0xFFu8; 4];
    assert_eq!(
        f.name_utf8(1, None, &mut small),
        NameLength {
            written: 3,
            total: 6
        }
    );
    assert_eq!(&small, b"Fam\0");
    let mut u16s = [0u16; 8];
    let n = f.name_utf16(2, None, &mut u16s);
    assert_eq!(n.written, 6);
    let mut u32s = [0u32; 2];
    assert_eq!(f.name_utf32(1, None, &mut u32s).total, 6);
    assert_eq!(f.name_utf8(9, None, &mut out), NameLength::default());
    assert_eq!(out[0], 0);
}

#[test]
fn names_drop_everything_for_a_record_out_of_the_table() {
    let mut data = name(0, &[(3, 1, 0x409, 1, &utf16be("Family"))], &[]);
    data[6 + 8..6 + 10].copy_from_slice(&200u16.to_be_bytes());
    let font = font_with(&[(b"name", data)], &[]);
    let f = Font::new(&font, 0).unwrap();
    assert_eq!(f.names().count(), 0);
    // FreeType skips the record instead.
    assert_eq!(f.sfnt_name_count(), 0);
}

#[test]
fn mac_names_decode_as_ascii() {
    let data = font_with(&[(b"name", name(0, &[(1, 0, 0, 1, b"Caf\x8e")], &[]))], &[]);
    let f = Font::new(&data, 0).unwrap();
    let mut out = [0u32; 8];
    let n = f.name_utf32(1, None, &mut out);
    assert_eq!(&out[..n.written], &[0x43, 0x61, 0x66, 0xFFFD]);
}

// ------------------------------------------------------------ sfnt_name

#[test]
fn sfnt_names_keep_what_freetype_keeps() {
    let records: Vec<NameRec<'_>> = vec![
        (3, 1, 0x409, 1, b"\0A"),
        (3, 1, 0x409, 2, b""),
        (3, 1, 0x8000, 3, b"\0B"),
        (3, 1, 0x8001, 4, b"\0C"),
    ];
    let data = font_with(&[(b"name", name(1, &records, &[b"\0e\0n"]))], &[]);
    let f = Font::new(&data, 0).unwrap();
    // The empty string and the missing language tag are dropped.
    assert_eq!(f.sfnt_name_count(), 2);
    let r = f.sfnt_name(1).unwrap();
    assert_eq!(
        (r.language_id, r.name_id, r.string),
        (0x8000, 3, &b"\0B"[..])
    );
    assert_eq!(f.sfnt_name(2), None);
}

// ----------------------------------------------------------- glyph names

#[test]
fn glyph_names_from_post() {
    let post = post2(&[0, 36, 258, 259, 258], &["alpha", "beta"]);
    let data = font_with(&[(b"post", post)], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.glyph_name(GlyphId(0)), Some(&b".notdef"[..]));
    assert_eq!(f.glyph_name(GlyphId(1)), Some(&b"A"[..]));
    assert_eq!(f.glyph_name(GlyphId(3)), Some(&b"beta"[..]));
    assert_eq!(f.glyph_name(GlyphId(5)), None);
    let mut buf = vec![0u32; f.glyph_names_len()];
    let names = f.glyph_names(&mut buf).unwrap();
    assert_eq!(names.glyph_name(GlyphId(2)), Some(&b"alpha"[..]));
    let alpha = names.glyph_from_name(b"alpha").unwrap();
    assert!(alpha == GlyphId(2) || alpha == GlyphId(4));
    assert_eq!(names.glyph_from_name(b"beta"), Some(GlyphId(3)));
    assert_eq!(names.glyph_from_name(b"gamma"), None);
    assert_eq!(names.glyph_from_name(b""), None);
    // FreeType: the first glyph with the name, and .notdef past post's glyphs.
    assert_eq!(names.ft_glyph_from_name(b"alpha"), Some(GlyphId(2)));
    assert_eq!(f.ft_glyph_name(GlyphId(5)), Some(&b".notdef"[..]));
    assert!(f.glyph_names(&mut [0u32; 2]).is_none());
}

#[test]
fn glyph_names_of_other_versions() {
    let data = font_with(&[(b"post", post_header(0x0001_0000).done())], &[]);
    let f = Font::new(&data, 0).unwrap();
    // HarfBuzz: the standard names; FreeType: only with 258 glyphs.
    assert_eq!(f.glyph_name(GlyphId(3)), Some(&b"space"[..]));
    assert_eq!(f.ft_glyph_name(GlyphId(3)), Some(&b".notdef"[..]));
    let data = font_with(&[(b"post", post_header(0x0003_0000).done())], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.glyph_name(GlyphId(3)), None);
    assert_eq!(f.ft_glyph_name(GlyphId(3)), None);
    let v25 = post_header(0x0002_5000).u16(28).bytes(&[3; 28]).done();
    let data = font_with(&[(b"post", v25)], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.glyph_name(GlyphId(0)), None);
    assert_eq!(f.ft_glyph_name(GlyphId(0)), Some(&b"space"[..]));
}

// ------------------------------------------------------------- FreeType

#[test]
fn ft_face_metrics_like_freetype() {
    let (glyf, loca) = glyf_loca(&vec![Vec::new(); 28], false);
    let base = [(b"glyf", glyf), (b"loca", loca)];
    let data = font_with(&base, &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.ft_face_metrics(),
        FtFaceMetrics {
            ascender: 800,
            descender: -200,
            height: 1090,
            max_advance_width: 1200,
            max_advance_height: 1090,
            underline_position: 0,
            underline_thickness: 0,
            bbox: [-50, -200, 950, 800],
        }
    );
    let mut t = base.to_vec();
    t.push((
        b"OS/2",
        os2(Os2Spec {
            fs_selection: 0x80,
            ..Default::default()
        }),
    ));
    t.push((b"post", post_header(0x0003_0000).done()));
    t.push((b"vhea", hhea(1)));
    t.push((b"vmtx", hmtx(&[(1100, 0)], &[])));
    let data = font_with(&t, &[]);
    let m = Font::new(&data, 0).unwrap().ft_face_metrics();
    assert_eq!((m.ascender, m.descender, m.height), (750, -250, 1100));
    assert_eq!(m.max_advance_height, 1200);
    assert_eq!((m.underline_position, m.underline_thickness), (-100, 50));
    // Zero hhea values: OS/2's win values when sTypo* are 0 too.
    let mut zero = hhea(20);
    zero[4..8].copy_from_slice(&[0; 4]);
    let mut t = base.to_vec();
    t.push((b"hhea", zero));
    t.push((
        b"OS/2",
        os2(Os2Spec {
            typo: (0, 0, 7),
            ..Default::default()
        }),
    ));
    let data = font_with(&t, &[]);
    let m = Font::new(&data, 0).unwrap().ft_face_metrics();
    assert_eq!((m.ascender, m.descender, m.height), (900, -300, 1200));
    // Without outlines, FreeType leaves the values at 0.
    let data = font_with(&[], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().ft_face_metrics(),
        FtFaceMetrics::default()
    );
}

#[test]
fn ft_v_advance_like_freetype() {
    let data = font_with(
        &[
            (b"vhea", hhea(2)),
            (b"vmtx", hmtx(&[(1100, 0), (900, 0)], &[])),
        ],
        &[],
    );
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.ft_v_advance(GlyphId(0)), Some(1100));
    assert_eq!(f.ft_v_advance(GlyphId(20)), Some(900));
    assert_eq!(f.ft_v_advance(GlyphId(28)), None);
    let data = font_with(&[(b"OS/2", os2(Os2Spec::default()))], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().ft_v_advance(GlyphId(1)),
        Some(1000)
    );
    let data = font_with(&[], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().ft_v_advance(GlyphId(1)),
        Some(1000)
    );
}
