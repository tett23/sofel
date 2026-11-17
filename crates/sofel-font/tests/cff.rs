//! `Cff`: FreeType's load checks, CID queries, glyph names and font
//! information (ADR 0021).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::read::cff::Version;
use sofel_font::read::cff::dict::op;
use sofel_font::read::{GlyphId, Severity};
use sofel_font::{Cff, CffError, CidError, Font, fs_type};

use Cs::{I, Op};

fn glyphs() -> Vec<Vec<u8>> {
    vec![
        charstring(&[Op(14)]),
        charstring(&[I(0), Op(22), I(5), Op(6), Op(14)]),
        charstring(&[Op(14)]),
    ]
}

fn named() -> Cff1Spec {
    let mut spec = Cff1Spec::new(glyphs());
    // Glyph 1 is "A" (SID 34), glyph 2 is "Custom" (SID 391).
    spec.charset = Some(vec![0, 0, 34, 1, 135]);
    spec.strings = vec![
        b"Custom".to_vec(),
        b"Copyright X".to_vec(),
        b"/FSType 8 def".to_vec(),
    ];
    spec.top = dict(&[
        (&[392], op::NOTICE),
        (&[393], op::POSTSCRIPT),
        (&[-12], op::ITALIC_ANGLE),
        (&[1], op::IS_FIXED_PITCH),
    ]);
    spec
}

fn cid_spec() -> Cff1Spec {
    let mut spec = Cff1Spec::new(glyphs());
    spec.private = None;
    spec.charset = Some(vec![2, 0, 100, 0, 1]);
    spec.strings = vec![b"Adobe".to_vec(), b"Japan1".to_vec()];
    spec.cid = Some(CidSpec {
        ros: (391, 392, 6),
        fds: vec![(Vec::new(), Vec::new(), Vec::new())],
        fd_select: vec![0, 0, 0, 0],
    });
    spec
}

fn font(table: &[u8]) -> Vec<u8> {
    otto_font(3, vec![(b"CFF ", table.to_vec())])
}

fn open(bytes: &[u8]) -> Result<Cff<'_>, CffError> {
    let f = Font::new(bytes, 0).unwrap();
    Cff::new(&f)
}

#[test]
fn opens_cff_and_prefers_cff2() {
    let b = font(&cff1(&named()));
    let c = open(&b).unwrap();
    assert_eq!(c.version(), Version::Cff);
    assert_eq!(c.num_glyphs(), 3);
    let both = otto_font(
        3,
        vec![
            (b"CFF ", cff1(&named())),
            (
                b"CFF2",
                cff2(&Cff2Spec {
                    charstrings: glyphs().into_iter().map(|_| Vec::new()).collect(),
                    fds: vec![(Vec::new(), Vec::new())],
                    ..Default::default()
                }),
            ),
        ],
    );
    assert_eq!(open(&both).unwrap().version(), Version::Cff2);
    // Not OTTO: FreeType's TrueType driver takes the font.
    let tt = sfnt(&[
        (b"CFF ", cff1(&named())),
        (b"head", head(HeadSpec::default())),
        (b"maxp", maxp05(3)),
    ]);
    assert_eq!(open(&tt).unwrap_err(), CffError::NotCff);
    let none = otto_font(3, vec![]);
    assert_eq!(open(&none).unwrap_err(), CffError::NotCff);
    let f = Font::new(&b, 0).unwrap();
    assert!(f.is_cff() && !f.is_cff2());
}

#[test]
fn freetype_load_failures() {
    let good = cff1(&named());
    let fail = |t: Vec<u8>| open(&font(&t)).is_err();
    assert!(!fail(good.clone()));
    let mut t = good.clone();
    t[0] = 2;
    assert!(fail(t));
    let mut t = good.clone();
    t[2] = 3;
    assert!(fail(t));
    let mut t = good.clone();
    t[3] = 5;
    assert!(fail(t));
    // offSize 0 is accepted by FreeType.
    let mut t = good.clone();
    t[3] = 0;
    assert!(!fail(t));
    let mut spec = named();
    spec.names.push(b"B".to_vec());
    assert!(fail(cff1(&spec)));
    let mut spec = named();
    spec.charset = Some(vec![7]);
    assert!(matches!(open(&font(&cff1(&spec))), Err(CffError::Charset)));
    let mut spec = named();
    spec.encoding = Some(vec![3, 0]);
    assert!(matches!(open(&font(&cff1(&spec))), Err(CffError::Encoding)));
    let mut spec = named();
    spec.top = dict(&[(&[], op::NOTICE)]);
    assert!(matches!(open(&font(&cff1(&spec))), Err(CffError::Dict(_))));
    // The CID-keyed font with 257 Font DICTs, and the CFF2 FDSelect
    // format 4.
    let mut spec = cid_spec();
    spec.cid.as_mut().unwrap().fds = (0..257)
        .map(|_| (Vec::new(), Vec::new(), Vec::new()))
        .collect();
    assert_eq!(
        open(&font(&cff1(&spec))).unwrap_err(),
        CffError::TooManyFontDicts
    );
    let cff2_with = |fd_select: Vec<u8>| {
        otto_font(
            3,
            vec![(
                b"CFF2",
                cff2(&Cff2Spec {
                    charstrings: glyphs().into_iter().map(|_| Vec::new()).collect(),
                    fds: vec![(Vec::new(), Vec::new()), (Vec::new(), Vec::new())],
                    fd_select,
                    ..Default::default()
                }),
            )],
        )
    };
    assert!(open(&cff2_with(vec![0, 0, 1, 0])).is_ok());
    assert_eq!(
        open(&cff2_with(vec![
            4, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3
        ]))
        .unwrap_err(),
        CffError::FdSelect
    );
}

#[test]
fn validator_errors_mean_freetype_fails() {
    // ADR 0007's invariant: a table without Error opens (except FDSelect
    // format 4, a warning that FreeType cannot read).
    let base = cff1(&named());
    let mut opened = 0;
    for i in 0..base.len() {
        for v in [0u8, 1, 0x7F, 0xFF] {
            let mut t = base.clone();
            t[i] = v;
            let mut error = false;
            let mut format4 = false;
            sofel_font::read::cff::validate(&t, Some(3), &mut |d| {
                error |= d.severity == Severity::Error;
                format4 |= d.kind.id() == "fd_select_format4";
            });
            if !error && !format4 {
                assert!(open(&font(&t)).is_ok(), "byte {i} = {v}");
                opened += 1;
            }
        }
    }
    assert!(opened > 0);
}

#[test]
fn cid_queries() {
    let b = font(&cff1(&cid_spec()));
    let c = open(&b).unwrap();
    assert!(c.is_cid_keyed());
    assert_eq!(c.cid(GlyphId(0)), Ok(0));
    assert_eq!(c.cid(GlyphId(1)), Ok(100));
    assert_eq!(c.cid(GlyphId(2)), Ok(101));
    assert_eq!(c.cid(GlyphId(3)), Err(CidError::InvalidGlyphIndex));
    let r = c.ros().unwrap();
    assert_eq!(
        (r.registry, r.ordering, r.supplement),
        (Some(&b"Adobe"[..]), Some(&b"Japan1"[..]), 6)
    );
    // Not CID-keyed.
    let b = font(&cff1(&named()));
    let c = open(&b).unwrap();
    assert_eq!(c.cid(GlyphId(0)), Err(CidError::NotCidKeyed));
    assert_eq!(c.ros(), Err(CidError::NotCidKeyed));
    // Standard strings, strings outside the String INDEX, a real and a
    // negative supplement.
    let mut spec = cid_spec();
    spec.cid.as_mut().unwrap().ros = (1, 500, -3);
    let b = font(&cff1(&spec));
    let r = open(&b).unwrap().ros().unwrap();
    assert_eq!(
        (r.registry, r.ordering, r.supplement),
        (Some(&b"space"[..]), None, -3)
    );
}

#[test]
fn glyph_names() {
    let b = font(&cff1(&named()));
    let c = open(&b).unwrap();
    assert_eq!(c.charset_name(GlyphId(0)), Some(&b".notdef"[..]));
    assert_eq!(c.charset_name(GlyphId(1)), Some(&b"A"[..]));
    assert_eq!(c.charset_name(GlyphId(2)), Some(&b"Custom"[..]));
    assert_eq!(c.charset_name(GlyphId(3)), None);
    assert_eq!(c.ft_glyph_from_name(b"Custom"), GlyphId(2));
    assert_eq!(c.ft_glyph_from_name(b"none"), GlyphId(0));
    let mut buf = [0u32; 3];
    let idx = c.build_name_index(&mut buf).unwrap().to_vec();
    assert_eq!(c.glyph_from_name(&idx, b"A"), Some(GlyphId(1)));
    assert_eq!(c.glyph_from_name(&idx, b""), None);
    assert!(c.build_name_index(&mut [0u32; 2]).is_none());
    let f = Font::new(&b, 0).unwrap();
    assert_eq!(f.ft_glyph_name(GlyphId(2)), Some(&b"Custom"[..]));
    // Duplicate names: the smaller glyph id.
    let mut spec = named();
    spec.charset = Some(vec![0, 0, 34, 0, 34]);
    let b = font(&cff1(&spec));
    let c = open(&b).unwrap();
    let mut buf = [0u32; 3];
    let idx = c.build_name_index(&mut buf).unwrap().to_vec();
    assert_eq!(c.glyph_from_name(&idx, b"A"), Some(GlyphId(1)));
    // A SID beyond the strings: FreeType gives an empty name.
    let mut spec = named();
    spec.charset = Some(vec![0, 0, 34, 2, 0]);
    let b = font(&cff1(&spec));
    let c = open(&b).unwrap();
    assert_eq!(c.charset_name(GlyphId(2)), None);
    assert_eq!(c.ft_glyph_name(GlyphId(2)), Some(&b""[..]));
    // CID-keyed: no charset names.
    let b = font(&cff1(&cid_spec()));
    assert_eq!(open(&b).unwrap().charset_name(GlyphId(1)), None);
}

#[test]
fn font_info() {
    let b = font(&cff1(&named()));
    let c = open(&b).unwrap();
    let i = c.ps_font_info().unwrap();
    assert_eq!(i.notice, Some(&b"Copyright X"[..]));
    assert_eq!(i.version, None);
    assert_eq!(i.italic_angle, -12 << 16);
    assert!(i.is_fixed_pitch);
    assert_eq!((i.underline_position, i.underline_thickness), (-100, 50));
    assert_eq!(c.ps_fs_type(), 8);
    assert_eq!(fs_type(b"/FSType 8 def"), 8);
    assert_eq!(fs_type(b"x /FSType\r\n 12\n def"), 12);
    assert_eq!(fs_type(b"/FSType 4\tdef"), 0);
    assert_eq!(fs_type(b"/FSType 8"), 0);
    assert_eq!(fs_type(b"/FSType 70000 def"), 0);
    assert_eq!(fs_type(b"/FSType x def"), 0);
    let b2 = otto_font(
        3,
        vec![(
            b"CFF2",
            cff2(&Cff2Spec {
                charstrings: glyphs().into_iter().map(|_| Vec::new()).collect(),
                fds: vec![(Vec::new(), Vec::new())],
                ..Default::default()
            }),
        )],
    );
    assert!(open(&b2).unwrap().ps_font_info().is_err());
}

#[test]
fn matrices_and_subfonts() {
    // FontMatrix 0.0005 0 0 0.0005 0 0: units_per_em 2000.
    let mut spec = named();
    let mut m = Vec::new();
    for s in ["0.0005", "0", "0", "0.0005", "0", "0"] {
        m.extend(d_real(s));
    }
    m.extend(d_op(op::FONT_MATRIX));
    spec.top = m;
    let b = font(&cff1(&spec));
    let c = open(&b).unwrap();
    let t = c.top_matrix();
    assert!(t.is_identity());
    assert_eq!(t.units_per_em, 2000);
    // Without a FontMatrix: head's unitsPerEm.
    let b = font(&cff1(&named()));
    assert_eq!(open(&b).unwrap().top_matrix().units_per_em, 1000);
    let s = open(&b).unwrap().subfont(GlyphId(1)).unwrap();
    // An empty Private DICT is none: FreeType's zeros.
    assert_eq!(s.private.initial_random_seed, 0);
    let mut spec = named();
    spec.private = Some(dict(&[(&[5], op::STD_HW)]));
    let b2 = font(&cff1(&spec));
    let s = open(&b2).unwrap().subfont(GlyphId(1)).unwrap();
    assert_eq!(s.private.initial_random_seed, 987_654_321);
    assert!(open(&b).unwrap().subfont(GlyphId(3)).is_none());
    // A Font DICT with its own matrix is concatenated with the top's.
    let mut spec = cid_spec();
    let mut fm = Vec::new();
    for s in ["0.002", "0", "0", "0.002", "0", "0"] {
        fm.extend(d_real(s));
    }
    fm.extend(d_op(op::FONT_MATRIX));
    spec.cid.as_mut().unwrap().fds[0].0 = fm;
    let b = font(&cff1(&spec));
    let s = open(&b).unwrap().subfont(GlyphId(1)).unwrap();
    assert_eq!(s.matrix.units_per_em, 500);
}

/// Every charset query with a `CharsetIndex` (ADR 0025) equals the one
/// without it, for every glyph and name.
fn assert_index_agrees(c: &Cff<'_>) {
    let charset = c.charset().unwrap();
    let mut starts = vec![0u32; charset.num_ranges()];
    let ix = charset.index_into(&mut starts).unwrap();
    let n = c.num_glyphs();
    for g in (0..n + 2).map(GlyphId) {
        assert_eq!(c.cid_with(&ix, g), c.cid(g), "{g:?}");
        assert_eq!(c.charset_name_with(&ix, g), c.charset_name(g), "{g:?}");
        assert_eq!(c.ft_glyph_name_with(&ix, g), c.ft_glyph_name(g), "{g:?}");
    }
    let mut a = vec![0u32; n as usize];
    let mut b = vec![0u32; n as usize];
    let plain = c.build_name_index(&mut a).map(<[u32]>::to_vec);
    let with = c.build_name_index_with(&ix, &mut b).map(<[u32]>::to_vec);
    assert_eq!(plain, with);
    if let Some(idx) = plain {
        let mut names: Vec<Vec<u8>> = (0..n)
            .filter_map(|g| c.charset_name(GlyphId(g)).map(<[u8]>::to_vec))
            .collect();
        names.extend([b"".to_vec(), b"none".to_vec(), b"zzz".to_vec()]);
        for name in names {
            assert_eq!(
                c.glyph_from_name_with(&ix, &idx, &name),
                c.glyph_from_name(&idx, &name),
                "{name:?}"
            );
        }
    }
}

#[test]
fn charset_index_agrees() {
    // Format 0.
    assert_index_agrees(&open(&font(&cff1(&named()))).unwrap());
    // Format 1 with several ranges, a repeated name and SIDs past the
    // strings.
    let mut spec = named();
    spec.charstrings = vec![charstring(&[Op(14)]); 20];
    spec.charset = Some(vec![1, 0, 34, 4, 1, 135, 1, 0, 34, 0, 1, 244, 10]);
    let b = font(&cff1(&spec));
    let c = open(&b).unwrap();
    assert_eq!(c.charset().unwrap().num_ranges(), 4);
    assert_eq!(c.charset_name(GlyphId(7)), Some(&b"Copyright X"[..]));
    assert_index_agrees(&c);
    // Predefined (ISOAdobe).
    let mut spec = named();
    spec.charset = None;
    assert_index_agrees(&open(&font(&cff1(&spec))).unwrap());
    // CID-keyed, format 2.
    let mut spec = cid_spec();
    spec.charstrings = vec![charstring(&[Op(14)]); 9];
    spec.charset = Some(vec![2, 0, 100, 0, 3, 0, 200, 0, 1, 1, 0, 0, 2]);
    spec.cid.as_mut().unwrap().fd_select = vec![0; 10];
    let b = font(&cff1(&spec));
    let c = open(&b).unwrap();
    assert_eq!(c.cid(GlyphId(8)), Ok(257));
    assert_index_agrees(&c);
}

#[test]
fn name_index_over_many_ranges() {
    // Many glyphs over many ranges, names repeated (ADR 0025): equal
    // names sort by glyph id.
    let mut spec = named();
    spec.charstrings = vec![charstring(&[Op(14)]); 3000];
    spec.charset = Some(
        vec![1, 0, 34, 255, 0, 1, 255, 0, 34, 255, 1, 135, 255]
            .into_iter()
            .chain([1u8, 0, 1, 255].repeat(8))
            .collect(),
    );
    let b = font(&cff1(&spec));
    let c = open(&b).unwrap();
    let mut buf = vec![0u32; 3000];
    let idx = c.build_name_index(&mut buf).unwrap().to_vec();
    assert_eq!(c.glyph_from_name(&idx, b"A"), Some(GlyphId(1)));
    assert_index_agrees(&c);
}

#[test]
fn adr0065_i1_bare_cid_identifiers_select_sparse_charstrings() {
    let data = cff1(&cid_spec());
    let face = sofel_font::BareCffFace::new(&data, 0).unwrap();
    assert_eq!(face.num_glyphs(), 102);
    assert_eq!(face.h_advance(GlyphId(1)), None);
    assert!(face.h_advance(GlyphId(100)).is_some());
    assert_eq!(face.nominal_glyph(100), None);
}
