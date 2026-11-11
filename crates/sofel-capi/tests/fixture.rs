//! Builds the fonts of the C tests (ADR 0015) and computes, with the
//! Rust API, the results the C ABI must give for them. Both go to
//! `tests/c/fixture.h`, which this test compares with the committed
//! file. Run with `SOFEL_UPDATE_FIXTURE=1` to rewrite it.

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

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use std::fmt::Write as _;

use build::*;
use sofel_font::read::sfnt::FontFile;
use sofel_font::read::{DetailValue, GlyphId, Severity, Tag, validate};
use sofel_font::{Direction, Font, FontError, GlyphExtents, HbScale, MetricsTag};
use sofel_geom::{Command, Fixed, FnPen, Matrix, Pen, Point};
use sofel_outline::{HbExtents, OutlineError, Scale, Scratch, draw, load};
use sofel_raster::{FillRule, RasterError, Rasterizer, RenderOptions, pixel_box};

// The error numbers of sofel.h.
const OK: i32 = 0;
const INVALID_FONT: i32 = -2;
const NO_HEAD: i32 = -3;
const FONT_INDEX: i32 = -4;
const NOT_FOUND: i32 = -5;
const TOO_LARGE: i32 = -10;
const INVALID_ARGUMENT: i32 = -12;

fn outline_error(e: OutlineError) -> i32 {
    match e {
        OutlineError::InvalidGlyphIndex => -6,
        OutlineError::NoGlyphData => -7,
        OutlineError::InvalidOutline => -8,
        OutlineError::InvalidComposite => -9,
        OutlineError::UnsupportedFormat => -7,
        OutlineError::InvalidCff => -13,
        OutlineError::InvalidCharString => -8,
    }
}

fn font_error(e: FontError) -> i32 {
    match e {
        FontError::Read(_) => INVALID_FONT,
        FontError::NoHead => NO_HEAD,
        FontError::IndexOutOfRange => FONT_INDEX,
    }
}

/// Glyphs: 0 empty, 1 'A' quadratic curves, 2 'B' with the overlap
/// flag, 3 'C' a composite of 1 and 2, 4 'D' broken, 5 'E' a composite
/// of itself, 6 'F' 30 em tall. Each left side bearing is the glyph's
/// xMin, so that FreeType does not shift the outlines (ADR 0018).
fn glyphs() -> Vec<Vec<u8>> {
    use Pt::{Off, On};
    let a = simple_glyph(
        &[
            &[
                On(0, 0),
                On(0, 700),
                Off(250, 900),
                On(500, 700),
                On(500, 0),
            ],
            &[On(100, 100), Off(200, 300), Off(300, 300), On(400, 100)],
        ],
        false,
        &[],
    );
    let b = simple_glyph(
        &[
            &[On(0, 0), On(0, 400), On(400, 400), On(400, 0)],
            &[On(200, 200), On(200, 600), On(600, 600), On(600, 200)],
        ],
        true,
        &[],
    );
    let c = composite_glyph(
        &[comp(1, Args::Xy(0, 0)), comp(2, Args::Xy(600, 100))],
        [0, 0, 1200, 900],
        false,
    );
    let mut d = simple_glyph(&[&[On(0, 0), On(0, 700), On(500, 700)]], false, &[]);
    d.truncate(14);
    let e = composite_glyph(&[comp(5, Args::Xy(0, 0))], [0, 0, 100, 100], false);
    let f = simple_glyph(
        &[&[On(0, 0), On(0, 30000), On(100, 30000), On(100, 0)]],
        false,
        &[],
    );
    vec![Vec::new(), a, b, c, d, e, f]
}

fn x_min(g: &[u8]) -> i16 {
    if g.len() >= 4 {
        i16::from_be_bytes([g[2], g[3]])
    } else {
        0
    }
}

fn font_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let glyphs = glyphs();
    let n = glyphs.len() as u16;
    let (glyf, loca) = glyf_loca(&glyphs, false);
    let metrics: Vec<(u16, i16)> = glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| (500 + 10 * i as u16, x_min(g)))
        .collect();
    let uvs = cmap14_15(
        false,
        &[Uvs {
            selector: 0xFE00,
            defaults: Some(vec![(0x42, 0)]),
            mappings: Some(vec![(0x41, 3)]),
        }],
    );
    let cmap_table = cmap(&[
        (0, 5, uvs),
        (
            3,
            1,
            cmap4(&[seg(0x41, 0x46, 1 - 0x41), seg(0xFFFF, 0xFFFF, 1)]),
        ),
    ]);
    let mut tables: Vec<(&'static [u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap_table),
        (b"glyf", glyf),
        (b"head", head(HeadSpec::default())),
        (b"hhea", hhea(n)),
        (b"hmtx", hmtx(&metrics, &[])),
        (b"loca", loca),
        (b"maxp", maxp05(n)),
    ];
    tables.sort_by_key(|(t, _)| **t);
    tables
}

/// A font with problems for the validator: unsorted cmap segments and
/// more horizontal metrics than glyphs.
fn invalid_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let mut tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(t, _)| *t != b"cmap" && *t != b"hhea")
        .collect();
    tables.push((
        b"cmap",
        cmap(&[(
            3,
            1,
            cmap4(&[
                seg(0x50, 0x5A, 1),
                seg(0x41, 0x45, 1),
                seg(0xFFFF, 0xFFFF, 1),
            ]),
        )]),
    ));
    tables.push((b"hhea", hhea(40)));
    tables.sort_by_key(|(t, _)| **t);
    tables
}

/// `font_tables()` with OS/2, vhea, vmtx and post, for the metrics
/// (ADR 0016), and BASE (ADR 0017). No VORG, so that the vertical origin
/// comes from glyf's phantom points.
fn metrics_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let n = glyphs().len() as u16;
    let long: Vec<(u16, i16)> = (0..n).map(|g| (1000 + 7 * g, 30 + g as i16)).collect();
    let mut tables = font_tables();
    tables.push((b"OS/2", os2(Os2Spec::default())));
    tables.push((b"vhea", hhea(n)));
    tables.push((b"vmtx", hmtx(&long, &[])));
    tables.push((
        b"post",
        post2(&[0, 36, 258, 259, 258, 3, 300], &["alpha", "beta"]),
    ));
    let ps = utf16be("Sofel-Test");
    let records: Vec<NameRec<'_>> = vec![
        (1, 0, 0, 1, b"MacFamily"),
        (3, 1, 0x409, 1, &[0, b'F', 0, b'a', 0xD8, 0x3D, 0xDE, 0x00]),
        (3, 1, 0x40C, 1, &[0, b'F', 0, b'r']),
        (3, 1, 0x409, 6, &ps),
    ];
    tables.push((b"name", name(0, &records, &[])));
    let h = AxisSpec {
        tags: Some(vec![*b"hang", *b"ideo", *b"romn"]),
        scripts: Some(vec![
            (
                *b"DFLT",
                Some(ScriptSpec {
                    values: Some((2, vec![None, Some(Coord::F1(-120)), Some(Coord::F1(0))])),
                    ..Default::default()
                }),
            ),
            (
                *b"latn",
                Some(ScriptSpec {
                    values: Some((
                        2,
                        vec![Some(Coord::F2(650, 1, 0)), None, Some(Coord::F1(5))],
                    )),
                    default_min_max: Some(MinMaxSpec {
                        min: Some(Coord::F1(-300)),
                        max: Some(Coord::F1(1100)),
                        features: vec![],
                    }),
                    lang_sys: vec![(
                        *b"ENG ",
                        Some(MinMaxSpec {
                            min: None,
                            max: Some(Coord::F1(1200)),
                            features: vec![],
                        }),
                    )],
                }),
            ),
        ]),
    };
    let v = simple_axis(b"DFLT", &[(*b"idtp", 450)]);
    tables.push((b"BASE", base(Some(&h), Some(&v))));
    tables.sort_by_key(|(t, _)| **t);
    tables
}

/// The baseline cases of ADR 0017 at one scale.
fn baseline_cases(out: &mut String, prefix: &str, font: &Font<'_>, scale: HbScale) {
    let extents = HbExtents::new(font.default_instance());
    let boxes = |g: GlyphId| extents.glyph_extents(g);
    let dirs = [(0, Direction::Horizontal), (1, Direction::Vertical)];
    let scripts = [*b"hani", *b"latn", *b"deva", *b"DFLT"];
    let mut rows = Vec::new();
    let mut ext_rows = Vec::new();
    for (d, dir) in dirs {
        for script in scripts {
            let script = Tag(script);
            for tag in sofel_font::baseline::ALL {
                let (s, val) = match scale.baseline(font, tag, dir, script) {
                    Some(v) => (OK, v),
                    None => (NOT_FOUND, 0),
                };
                let fb = scale.baseline_with_fallback(font, tag, dir, script, boxes);
                rows.push(format!(
                    "{{{d}, 0x{:08x}u, 0x{:08x}u, {s}, {val}, {fb}}}",
                    script.to_u32(),
                    tag.to_u32()
                ));
            }
            for lang in [*b"dflt", *b"ENG "] {
                let lang = Tag(lang);
                let (e, found) = scale.layout_font_extents(font, dir, script, lang);
                ext_rows.push(format!(
                    "{{{d}, 0x{:08x}u, 0x{:08x}u, {}, {{{}, {}, {}}}}}",
                    script.to_u32(),
                    lang.to_u32(),
                    i32::from(found),
                    e.ascender,
                    e.descender,
                    e.line_gap
                ));
            }
        }
    }
    writeln!(
        out,
        "static const struct baseline_case {prefix}_baselines[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    writeln!(
        out,
        "static const struct layout_extents_case {prefix}_layout_extents[] = {{{}}};",
        ext_rows.join(", ")
    )
    .unwrap();
}

/// The name cases of ADR 0016.
fn name_cases(out: &mut String, font: &Font<'_>) {
    let mut ps = [0u8; 128];
    let n = font.postscript_name(&mut ps).unwrap();
    writeln!(
        out,
        "#define FIXTURE_PS_NAME \"{}\"",
        String::from_utf8_lossy(&ps[..n])
    )
    .unwrap();
    let rows: Vec<String> = font
        .names()
        .map(|e| {
            let mut b = [0u8; 16];
            let n = e.language.write(&mut b);
            format!(
                "{{{}, \"{}\"}}",
                e.name_id,
                String::from_utf8_lossy(&b[..n])
            )
        })
        .collect();
    writeln!(
        out,
        "static const struct {{ uint16_t id; const char *lang; }} name_list[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let mut rows = Vec::new();
    for (id, lang) in [
        (1u16, None),
        (1, Some(&b"fr"[..])),
        (1, Some(&b"en-GB"[..])),
        (6, None),
        (9, None),
    ] {
        for cap in [16usize, 3] {
            let mut b8 = vec![0u8; cap];
            let n8 = font.name_utf8(id, lang, &mut b8);
            let mut b16 = vec![0u16; cap];
            let n16 = font.name_utf16(id, lang, &mut b16);
            let mut b32 = vec![0u32; cap];
            let n32 = font.name_utf32(id, lang, &mut b32);
            let units = |v: Vec<String>| {
                if v.is_empty() {
                    "0".to_string()
                } else {
                    v.join(", ")
                }
            };
            rows.push(format!(
                "{{{id}, \"{}\", {cap}, {{{}, {}, {}, {}, {}, {}}}, {{{}}}, {{{}}}, {{{}}}}}",
                lang.map_or(String::new(), |l| String::from_utf8_lossy(l).into_owned()),
                n8.written,
                n8.total,
                n16.written,
                n16.total,
                n32.written,
                n32.total,
                units(b8[..n8.written].iter().map(|v| v.to_string()).collect()),
                units(b16[..n16.written].iter().map(|v| v.to_string()).collect()),
                units(b32[..n32.written].iter().map(|v| v.to_string()).collect()),
            ));
        }
    }
    writeln!(
        out,
        "static const struct name_case name_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let rows: Vec<String> = (0..font.sfnt_name_count())
        .map(|i| {
            let r = font.sfnt_name(i).unwrap();
            format!(
                "{{{{{}, {}, {}, {}}}, {}}}",
                r.platform_id,
                r.encoding_id,
                r.language_id,
                r.name_id,
                r.string.len()
            )
        })
        .collect();
    writeln!(
        out,
        "static const struct {{ uint16_t ids[4]; size_t len; }} sfnt_names[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let mut buf = vec![0u32; font.glyph_names_len()];
    let index = font.glyph_names(&mut buf).unwrap();
    let rows: Vec<String> = (0..font.num_glyphs() + 2)
        .map(|g| {
            let g = GlyphId(g);
            let s = |n: Option<&[u8]>| match n {
                Some(n) => format!("\"{}\"", String::from_utf8_lossy(n)),
                None => "NULL".to_string(),
            };
            let hb = font.glyph_name(g);
            let ft = font.ft_glyph_name(g);
            let back = |n: Option<&[u8]>, ft: bool| {
                n.and_then(|n| {
                    if ft {
                        index.ft_glyph_from_name(n)
                    } else {
                        index.glyph_from_name(n)
                    }
                })
                .map_or(-1, |g| i64::from(g.0))
            };
            format!(
                "{{{}, {}, {}, {}, {}, {}}}",
                g.0,
                s(hb),
                s(ft),
                back(hb, false),
                back(ft, true),
                font.default_instance()
                    .ft_v_advance(g)
                    .map_or(-1, i64::from)
            )
        })
        .collect();
    writeln!(
        out,
        "static const struct glyph_name_case glyph_names[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let m = font.default_instance().ft_face_metrics();
    let mut v = vec![
        m.ascender,
        m.descender,
        m.height,
        m.max_advance_width,
        m.max_advance_height,
        m.underline_position,
        m.underline_thickness,
    ];
    v.extend(m.bbox);
    ints(out, "int32_t", "ft_face_metrics", &v);
}

fn ext4(e: GlyphExtents) -> [i32; 4] {
    [e.x_bearing, e.y_bearing, e.width, e.height]
}

/// The metrics cases at one scale (`None`: the sofel_font_* functions).
fn metrics_cases(out: &mut String, prefix: &str, font: &Font<'_>, scale: HbScale) {
    let extents = HbExtents::new(font.default_instance());
    let boxes = |g: GlyphId| extents.glyph_extents(g);
    let h = scale.h_extents(font);
    let v = scale.v_extents(font);
    ints(
        out,
        "int32_t",
        &format!("{prefix}_h_extents"),
        &[h.ascender, h.descender, h.line_gap],
    );
    ints(
        out,
        "int32_t",
        &format!("{prefix}_v_extents"),
        &[v.ascender, v.descender, v.line_gap],
    );
    let glyphs = 0..font.num_glyphs() + 2;
    let rows: Vec<String> = glyphs
        .map(|g| {
            let g = GlyphId(g);
            let (ox, oy) = scale.v_origin(font, g, boxes);
            let (es, e) = match extents.scaled_glyph_extents(g, &scale) {
                Some(e) => (OK, ext4(e)),
                None => (NOT_FOUND, [0; 4]),
            };
            format!(
                "{{{}, {}, {}, {{{ox}, {oy}}}, {es}, {{{}, {}, {}, {}}}}}",
                g.0,
                scale.h_advance(font, g),
                scale.v_advance(font, g),
                e[0],
                e[1],
                e[2],
                e[3]
            )
        })
        .collect();
    writeln!(
        out,
        "static const struct vglyph_case {prefix}_glyphs[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let rows: Vec<String> = MetricsTag::ALL
        .iter()
        .map(|&t| {
            let (s, val) = match scale.metrics_position(font, t) {
                Some(v) => (OK, v),
                None => (NOT_FOUND, 0),
            };
            let fb = scale.metrics_position_with_fallback(font, t, boxes);
            format!("{{0x{:08x}u, {s}, {val}, {fb}}}", t.tag().to_u32())
        })
        .collect();
    writeln!(
        out,
        "static const struct metric_case {prefix}_metrics[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
}

/// A CFF font for the boxes and names of ADR 0021: glyph 1 "A" a line,
/// 2 "grave" a curve, 3 "dup" a seac of A and grave, 4 "dup" a
/// charstring without endchar (no box), 5 "B" with a hint mask.
fn cff_metrics_font() -> Vec<u8> {
    use Cs::*;
    let glyphs = vec![
        charstring(&[Op(14)]),
        charstring(&[I(10), I(-20), Op(21), I(300), I(700), Op(5), Op(14)]),
        charstring(&[
            I(0),
            I(0),
            Op(21),
            I(50),
            I(90),
            I(40),
            I(-10),
            I(20),
            I(-60),
            Op(8),
            Op(14),
        ]),
        charstring(&[I(400), I(-30), I(100), I(65), I(193), Op(14)]),
        charstring(&[I(0), I(0), Op(21), I(5), I(5), Op(5)]),
        charstring(&[
            I(1),
            I(2),
            Op(1),
            Op(19),
            Raw(vec![0x80]),
            F(0x1_8000),
            F(-0x8000),
            Op(21),
            I(9),
            I(9),
            Op(5),
            Op(14),
        ]),
    ];
    let mut spec = Cff1Spec::new(glyphs);
    spec.charset = Some(vec![0, 0, 34, 0, 124, 1, 135, 1, 136, 0, 35]);
    spec.strings = vec![b"dup".to_vec(), b"dup".to_vec()];
    otto_font(6, vec![(b"CFF ", cff1(&spec))])
}

/// A CID-keyed CFF font for the CID queries and the font information
/// (ADR 0021): registry "Adobe", ordering "Japan1", supplement 6, glyph
/// 1 CID 100, Notice "Note".
fn cid_font() -> Vec<u8> {
    use Cs::*;
    let glyphs = vec![
        charstring(&[Op(14)]),
        charstring(&[Op(14)]),
        charstring(&[Op(14)]),
    ];
    let mut spec = Cff1Spec::new(glyphs);
    spec.private = None;
    spec.charset = Some(vec![2, 0, 100, 0, 1]);
    spec.strings = vec![b"Adobe".to_vec(), b"Japan1".to_vec(), b"Note".to_vec()];
    spec.top = dict(&[(&[393], 1)]);
    spec.cid = Some(CidSpec {
        ros: (391, 392, 6),
        fds: vec![(Vec::new(), Vec::new(), Vec::new())],
        fd_select: vec![0, 0, 0, 0],
    });
    otto_font(3, vec![(b"CFF ", cff1(&spec))])
}

/// The HarfBuzz glyph names of the CFF font: name and the glyph found by
/// it.
fn cff_name_cases(out: &mut String, font: &Font<'_>) {
    let mut buf = vec![0u32; font.glyph_names_len()];
    let index = font.glyph_names(&mut buf).unwrap();
    let rows: Vec<String> = (0..font.num_glyphs() + 2)
        .map(|g| {
            let g = GlyphId(g);
            let hb = font.glyph_name(g);
            let name = hb.map_or("NULL".to_string(), |n| {
                format!("\"{}\"", String::from_utf8_lossy(n))
            });
            let back = hb
                .and_then(|n| index.glyph_from_name(n))
                .map_or(-1, |g| i64::from(g.0));
            format!("{{{}, {name}, {back}}}", g.0)
        })
        .collect();
    writeln!(
        out,
        "static const struct cff_name_case {{ uint32_t glyph; const char *hb; int64_t back; }} \
         cff_names[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
}

fn no_head_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    sample_tables()
        .into_iter()
        .filter(|(t, _)| *t != b"head")
        .collect()
}

fn bytes(out: &mut String, name: &str, data: &[u8]) {
    writeln!(out, "static const uint8_t {name}[{}] = {{", data.len()).unwrap();
    for line in data.chunks(16) {
        let items: Vec<String> = line.iter().map(|b| format!("0x{b:02x}")).collect();
        writeln!(out, "  {},", items.join(", ")).unwrap();
    }
    writeln!(out, "}};").unwrap();
}

fn ints<T: std::fmt::Display>(out: &mut String, ty: &str, name: &str, values: &[T]) {
    let items: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    // C has no empty arrays: keep one unused element.
    let body = if items.is_empty() {
        "0".to_string()
    } else {
        items.join(", ")
    };
    writeln!(out, "static const {ty} {name}[] = {{{body}}};").unwrap();
}

fn encode(commands: &[Command]) -> Vec<i32> {
    commands
        .iter()
        .flat_map(|c| match *c {
            Command::MoveTo(p) => vec!['M' as i32, p.x, p.y],
            Command::LineTo(p) => vec!['L' as i32, p.x, p.y],
            Command::QuadTo(c, p) => vec!['Q' as i32, c.x, c.y, p.x, p.y],
            Command::CubicTo(c1, c2, p) => vec!['C' as i32, c1.x, c1.y, c2.x, c2.y, p.x, p.y],
            Command::Close => vec!['Z' as i32],
        })
        .collect()
}

fn severity(s: Severity) -> i32 {
    match s {
        Severity::Error => 0,
        Severity::Warning => 1,
    }
}

struct Bitmap {
    status: i32,
    b: [i32; 4],
    pixels: Vec<u8>,
}

fn bitmap(r: Result<sofel_raster::Bitmap<'_>, i32>) -> Bitmap {
    match r {
        Ok(b) => Bitmap {
            status: OK,
            b: [b.left, b.top, b.width as i32, b.rows as i32],
            pixels: b.pixels.to_vec(),
        },
        Err(e) => Bitmap {
            status: e,
            b: [0; 4],
            pixels: Vec::new(),
        },
    }
}

fn bitmap_case(out: &mut String, cases: &mut Vec<String>, name: &str, prefix: &str, bm: &Bitmap) {
    ints(out, "uint8_t", name, &bm.pixels);
    cases.push(format!(
        "{{{prefix}{}, {{{}, {}, {}, {}}}, {}, {name}}}",
        bm.status,
        bm.b[0],
        bm.b[1],
        bm.b[2],
        bm.b[3],
        bm.pixels.len()
    ));
}

fn raster_error(e: RasterError) -> i32 {
    match e {
        RasterError::TooLarge => TOO_LARGE,
    }
}

/// The diagnostics of `data` as `sofel_validate` reports them.
fn diagnostics(out: &mut String, name: &str, data: &[u8]) {
    let mut rows = Vec::new();
    let mut emit = |font: Option<u32>, d: sofel_font::read::Diagnostic| {
        let mut details = String::new();
        d.kind.details(&mut |k, v| {
            let (kind, value) = match v {
                DetailValue::Uint(n) => (0, i64::from(n)),
                DetailValue::Int(n) => (1, i64::from(n)),
                DetailValue::Tag(t) => (2, i64::from(t.to_u32())),
            };
            write!(details, "{k}:{kind}:{value};").unwrap();
        });
        rows.push(format!(
            "{{{}, 0x{:08x}u, {}u, {}, \"{}\", \"{}\", \"{}\"}}",
            font.map_or(-1, i64::from),
            d.table.to_u32(),
            d.offset,
            severity(d.severity),
            d.kind.id(),
            d.kind
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\""),
            details
        ));
    };
    match FontFile::new(data).unwrap() {
        FontFile::Sfnt(s) => validate::sfnt(&s, &mut |d| emit(None, d)),
        FontFile::Collection(c) => {
            validate::collection(&c, &mut |d| emit(None, d));
            for i in 0..c.num_fonts() {
                if let Ok(s) = c.font(i) {
                    validate::sfnt(&s, &mut |d| emit(Some(i), d));
                }
            }
        }
    }
    assert!(!rows.is_empty(), "{name} has no problems to report");
    writeln!(
        out,
        "static const struct diagnostic_case {name}[] = {{\n  {},\n}};",
        rows.join(",\n  ")
    )
    .unwrap();
}

fn generate() -> String {
    let font_data = sfnt(&font_tables());
    let ttc_data = ttc(false, &[invalid_tables(), font_tables()]);
    let invalid_data = sfnt(&invalid_tables());
    let no_head_data = sfnt(&no_head_tables());
    let font = Font::new(&font_data, 0).unwrap();

    let mut out = String::new();
    out.push_str(
        "/* Generated by crates/sofel-capi/tests/fixture.rs from the Rust API; do\n \
         * not edit. `SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test\n \
         * fixture` rewrites it. */\n\n\
         #include <stddef.h>\n#include <stdint.h>\n\n",
    );
    out.push_str(
        "struct glyph_case { uint32_t codepoint; uint32_t selector; int32_t status; uint32_t glyph; };\n\
         struct table_case { uint32_t tag; int32_t status; size_t offset; size_t len; };\n\
         struct outline_case { uint32_t glyph; uint32_t mode; int32_t x_scale; int32_t y_scale;\n\
         \x20 int32_t status; int32_t cbox[4]; uint32_t overlap; size_t len; const int32_t *commands; };\n\
         struct render_case { uint32_t glyph; uint16_t ppem; int32_t status; int32_t box[4];\n\
         \x20 size_t len; const uint8_t *pixels; };\n\
         struct raster_case { uint32_t fill; uint32_t overlap; uint16_t ppem; int32_t status;\n\
         \x20 int32_t box[4]; size_t len; const uint8_t *pixels; };\n\
         struct pixel_box_case { int32_t cbox[4]; int32_t box[4]; int32_t shift[2]; };\n\
         struct fix_case { int32_t a; int32_t b; int32_t c; int32_t mul_fix; int32_t div_fix;\n\
         \x20 int32_t mul_div; int32_t mul_div_no_round; int32_t vector_length; };\n\
         struct vglyph_case { uint32_t glyph; int32_t h_advance; int32_t v_advance; int32_t origin[2];\n\
         \x20 int32_t extents_status; int32_t extents[4]; };\n\
         struct metric_case { uint32_t tag; int32_t status; int32_t value; int32_t fallback; };\n\
         struct baseline_case { int32_t direction; uint32_t script; uint32_t tag; int32_t status;\n\
         \x20 int32_t value; int32_t fallback; };\n\
         struct layout_extents_case { int32_t direction; uint32_t script; uint32_t language;\n\
         \x20 int32_t found; int32_t extents[3]; };\n\
         struct os2_case { int32_t field; int32_t status; int64_t value; };\n\
         struct name_case { uint16_t id; const char *lang; size_t cap; size_t n[6];\n\
         \x20 uint8_t utf8[16]; uint16_t utf16[16]; uint32_t utf32[16]; };\n\
         struct glyph_name_case { uint32_t glyph; const char *hb; const char *ft;\n\
         \x20 int64_t hb_back; int64_t ft_back; int64_t ft_v_advance; };\n\
         struct diagnostic_case { int64_t font; uint32_t table; uint32_t offset; int32_t severity;\n\
         \x20 const char *kind; const char *message; const char *details; };\n\
         struct open_case { const uint8_t *data; size_t len; uint32_t index; int32_t status; };\n\
         struct unicode_case { uint32_t codepoint; uint8_t gc; uint8_t ccc; uint32_t script;\n\
         \x20 uint32_t mirror; int32_t status; uint32_t a; uint32_t b; uint8_t pict; uint8_t vo; };\n\
         struct compose_case { uint32_t a; uint32_t b; int32_t status; uint32_t ab; };\n\
         struct script_case { const char *tag; int32_t status; uint32_t script; };\n\n",
    );

    bytes(&mut out, "font_data", &font_data);
    bytes(&mut out, "ttc_data", &ttc_data);
    bytes(&mut out, "invalid_data", &invalid_data);
    bytes(&mut out, "no_head_data", &no_head_data);
    let not_font = b"not a font";
    bytes(&mut out, "not_font_data", not_font);

    // Opening.
    let opens: [(&str, &[u8], u32); 6] = [
        ("font_data", &font_data, 0),
        ("font_data", &font_data, 1),
        ("ttc_data", &ttc_data, 1),
        ("ttc_data", &ttc_data, 2),
        ("no_head_data", &no_head_data, 0),
        ("not_font_data", not_font, 0),
    ];
    let rows: Vec<String> = opens
        .iter()
        .map(|(name, data, index)| {
            let status = Font::new(data, *index).map_or_else(font_error, |_| OK);
            format!("{{{name}, sizeof {name}, {index}, {status}}}")
        })
        .collect();
    writeln!(
        out,
        "static const struct open_case open_cases[] = {{\n  {},\n}};",
        rows.join(",\n  ")
    )
    .unwrap();
    let in_ttc = Font::new(&ttc_data, 1).unwrap();
    assert_eq!(in_ttc.num_glyphs(), font.num_glyphs());

    writeln!(out, "#define FIXTURE_UPEM {}", font.units_per_em()).unwrap();
    writeln!(out, "#define FIXTURE_NUM_GLYPHS {}u", font.num_glyphs()).unwrap();

    // Characters to glyphs.
    let status = |g: Option<GlyphId>| g.map_or((NOT_FOUND, 0), |g| (OK, g.0));
    let rows: Vec<String> = [0x41u32, 0x42, 0x46, 0x47, 0x20, 0x1F600]
        .iter()
        .map(|&c| {
            let (s, g) = status(font.nominal_glyph(c));
            format!("{{0x{c:x}, 0, {s}, {g}}}")
        })
        .collect();
    writeln!(
        out,
        "static const struct glyph_case nominal_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let rows: Vec<String> = [
        (0x41u32, 0xFE00u32),
        (0x42, 0xFE00),
        (0x43, 0xFE00),
        (0x41, 0xFE01),
    ]
    .iter()
    .map(|&(c, v)| {
        let (s, g) = status(font.variation_glyph(c, v));
        format!("{{0x{c:x}, 0x{v:x}, {s}, {g}}}")
    })
    .collect();
    writeln!(
        out,
        "static const struct glyph_case variation_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();

    // Advances, two glyphs past the end included.
    let advances: Vec<u32> = (0..font.num_glyphs() + 2)
        .map(|g| font.default_instance().advance(GlyphId(g)))
        .collect();
    ints(&mut out, "uint32_t", "advances", &advances);
    let bearings: Vec<i16> = (0..font.num_glyphs() + 2)
        .map(|g| font.default_instance().ft_left_side_bearing(GlyphId(g)))
        .collect();
    ints(&mut out, "int16_t", "ft_bearings", &bearings);

    // Tables.
    let rows: Vec<String> = [b"cmap", b"head", b"glyf", b"GSUB"]
        .iter()
        .map(|t| {
            let tag = Tag::new(t);
            match font.table(tag) {
                Some(d) => format!(
                    "{{0x{:08x}u, {OK}, {}, {}}}",
                    tag.to_u32(),
                    d.as_ptr() as usize - font_data.as_ptr() as usize,
                    d.len()
                ),
                None => format!("{{0x{:08x}u, {NOT_FOUND}, 0, 0}}", tag.to_u32()),
            }
        })
        .collect();
    writeln!(
        out,
        "static const struct table_case table_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();

    // Scales.
    let scales = [(16u16, 16u16), (37, 23)];
    let mut scale_rows = Vec::new();
    for (x, y) in scales {
        let Scale::Scaled { x: sx, y: sy } = Scale::pixels(&font, x, y) else {
            unreachable!()
        };
        scale_rows.push(format!("{{{x}, {y}, {}, {}}}", sx.0, sy.0));
    }
    writeln!(
        out,
        "static const struct {{ uint16_t x_ppem; uint16_t y_ppem; int32_t x_scale; int32_t y_scale; }} \
         scale_cases[] = {{{}}};",
        scale_rows.join(", ")
    )
    .unwrap();

    // Outlines: every glyph and one past the end, unscaled and scaled.
    let mut cases = Vec::new();
    let mut scratch = Scratch::default();
    let modes: Vec<(u32, Scale)> = [(0, Scale::Unscaled)]
        .into_iter()
        .chain(scales.iter().map(|&(x, y)| (1, Scale::pixels(&font, x, y))))
        .collect();
    for g in 0..=font.num_glyphs() {
        for (i, (mode, scale)) in modes.iter().enumerate() {
            let mut log = Vec::new();
            let r = draw(
                font.default_instance(),
                GlyphId(g),
                *scale,
                &mut scratch,
                &mut FnPen(|c| log.push(c)),
            );
            let name = format!("outline_{g}_{i}");
            let enc = encode(&log);
            ints(&mut out, "int32_t", &name, &enc);
            let (status, cbox, overlap) = match r {
                Ok(info) => (
                    OK,
                    [
                        info.cbox.x_min,
                        info.cbox.y_min,
                        info.cbox.x_max,
                        info.cbox.y_max,
                    ],
                    u32::from(info.overlap),
                ),
                Err(e) => (outline_error(e), [0; 4], 0),
            };
            let (sx, sy) = match scale {
                Scale::Unscaled => (0, 0),
                Scale::Scaled { x, y } => (x.0, y.0),
            };
            cases.push(format!(
                "{{{g}, {mode}, {sx}, {sy}, {status}, {{{}, {}, {}, {}}}, {overlap}, {}, {name}}}",
                cbox[0],
                cbox[1],
                cbox[2],
                cbox[3],
                enc.len()
            ));
        }
    }
    writeln!(
        out,
        "static const struct outline_case outline_cases[] = {{\n  {},\n}};",
        cases.join(",\n  ")
    )
    .unwrap();

    // Decomposing a loaded outline with an offset and a multiplier.
    load(
        font.default_instance(),
        GlyphId(1),
        modes[1].1,
        &mut scratch,
    )
    .unwrap();
    let mut log = Vec::new();
    scratch
        .decompose(Point { x: 100, y: -50 }, 4, &mut FnPen(|c| log.push(c)))
        .unwrap();
    ints(&mut out, "int32_t", "decomposed", &encode(&log));

    // Rendering every glyph and one past the end.
    let mut cases = Vec::new();
    let mut raster = Rasterizer::new();
    let outlines = sofel_outline::Outlines::new(font.default_instance());
    for g in 0..=font.num_glyphs() {
        for ppem in [16u16, 41] {
            let r =
                sofel_cli::glyph::render(&outlines, GlyphId(g), ppem, &mut scratch, &mut raster)
                    .map_err(|e| match e {
                        sofel_cli::glyph::RenderError::Outline(e) => outline_error(e),
                        sofel_cli::glyph::RenderError::Raster(e) => raster_error(e),
                    });
            let bm = bitmap(r);
            let name = format!("render_{g}_{ppem}");
            bitmap_case(&mut out, &mut cases, &name, &format!("{g}, {ppem}, "), &bm);
        }
    }
    writeln!(
        out,
        "static const struct render_case render_cases[] = {{\n  {},\n}};",
        cases.join(",\n  ")
    )
    .unwrap();

    // The rasterizer as a pen: a 10-pixel square with a hole made of a
    // cubic curve, under each fill rule, with and without overlap, and
    // with a ppem limit it exceeds.
    let p = |x: i32, y: i32| Point {
        x: x * 64,
        y: y * 64,
    };
    let shape = |r: &mut Rasterizer| {
        r.clear();
        r.move_to(p(0, 0));
        r.line_to(p(0, 10));
        r.line_to(p(10, 10));
        r.line_to(p(10, 0));
        r.line_to(p(0, 0));
        r.close();
        r.move_to(p(2, 2));
        r.cubic_to(p(2, 8), p(8, 8), p(8, 2));
        r.quad_to(p(5, 0), p(2, 2));
        r.close();
    };
    let mut cases = Vec::new();
    for (fill, rule) in [(0u32, FillRule::NonZero), (1, FillRule::EvenOdd)] {
        for (overlap, ppem) in [(0u32, 0u16), (1, 0), (0, 1)] {
            shape(&mut raster);
            let opts = RenderOptions {
                fill: rule,
                overlap: overlap != 0,
                ppem: (ppem != 0).then_some((ppem, ppem)),
            };
            let bm = bitmap(raster.render(opts).map_err(raster_error));
            let name = format!("raster_{fill}_{overlap}_{ppem}");
            bitmap_case(
                &mut out,
                &mut cases,
                &name,
                &format!("{fill}, {overlap}, {ppem}, "),
                &bm,
            );
        }
    }
    writeln!(
        out,
        "static const struct raster_case raster_cases[] = {{\n  {},\n}};",
        cases.join(",\n  ")
    )
    .unwrap();

    // render_placed: glyph 2 (overlap) at 41 ppem with the even-odd rule,
    // composed from load, pixel_box and decompose.
    let info = load(
        font.default_instance(),
        GlyphId(2),
        Scale::pixels(&font, 41, 41),
        &mut scratch,
    )
    .unwrap();
    let c = info.cbox;
    let pb = pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
    raster.clear();
    scratch.decompose(pb.shift(), 4, &mut raster).unwrap();
    let opts = RenderOptions {
        fill: FillRule::EvenOdd,
        overlap: true,
        ppem: Some((41, 41)),
    };
    let bm = bitmap(raster.render_placed(pb, opts).map_err(raster_error));
    let mut cases = Vec::new();
    bitmap_case(&mut out, &mut cases, "placed_pixels", "1, 1, 41, ", &bm);
    writeln!(
        out,
        "static const struct raster_case placed_case = {};",
        cases[0]
    )
    .unwrap();

    // Pixel boxes.
    let rows: Vec<String> = [
        [0, 0, 0, 0],
        [-100, -37, 640, 700],
        [65, 1, 127, 63],
        [-2_000_000_000, -5, 2_000_000_000, 5],
    ]
    .iter()
    .map(|c| {
        let b = pixel_box(c[0], c[1], c[2], c[3]);
        let s = b.shift();
        format!(
            "{{{{{}, {}, {}, {}}}, {{{}, {}, {}, {}}}, {{{}, {}}}}}",
            c[0], c[1], c[2], c[3], b.left, b.top, b.width, b.rows, s.x, s.y
        )
    })
    .collect();
    writeln!(
        out,
        "static const struct pixel_box_case pixel_box_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();

    // Fixed-point arithmetic.
    let rows: Vec<String> = [
        (0x18000, 0x28000, 3),
        (-0x18000, 0x8000, -7),
        (i32::MAX, i32::MAX, 1),
        (1000, 0, 0),
        (-12345, 678, 90),
    ]
    .iter()
    .map(|&(a, b, c)| {
        format!(
            "{{{a}, {b}, {c}, {}, {}, {}, {}, {}}}",
            sofel_geom::mul_fix(a, b),
            sofel_geom::div_fix(a, b),
            sofel_geom::mul_div(a, b, c),
            sofel_geom::mul_div_no_round(a, b, c),
            sofel_geom::vector_length(a, b)
        )
    })
    .collect();
    writeln!(
        out,
        "static const struct fix_case fix_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let m = Matrix {
        xx: Fixed(0x10000),
        xy: Fixed(0x4000),
        yx: Fixed(-0x8000),
        yy: Fixed(0x18000),
    };
    let t = sofel_geom::transform(Point { x: 300, y: -77 }, &m);
    writeln!(
        out,
        "#define TRANSFORM_IN 300, -77, 0x10000, 0x4000, -0x8000, 0x18000\n\
         #define TRANSFORM_X {}\n#define TRANSFORM_Y {}",
        t.x, t.y
    )
    .unwrap();

    // Metrics (ADR 0016).
    let metrics_data = sfnt(&metrics_tables());
    bytes(&mut out, "metrics_font_data", &metrics_data);
    let mfont = Font::new(&metrics_data, 0).unwrap();
    writeln!(
        out,
        "#define FIXTURE_FS_TYPE {}\n#define FIXTURE_HB_UPEM {}",
        mfont.fs_type(),
        mfont.hb_units_per_em()
    )
    .unwrap();
    let os2_view = mfont.os2().unwrap();
    let rows: Vec<String> = (-1..=48)
        .map(|field: i32| {
            let o = &os2_view;
            let v: Option<i64> = match field {
                0 => Some(o.version().into()),
                2 => Some(o.us_weight_class().into()),
                4 => Some(o.fs_type().into()),
                20 => Some(o.ach_vend_id().to_u32().into()),
                21 => Some(o.fs_selection().into()),
                24 => Some(o.s_typo_ascender().into()),
                32 => o.s_cap_height().map(i64::from),
                37 => o.us_upper_optical_point_size().map(i64::from),
                39 => Some(o.panose()[1].into()),
                _ => return String::new(),
            };
            match v {
                Some(v) => format!("{{{field}, {OK}, {v}}}"),
                None => format!("{{{field}, {NOT_FOUND}, 0}}"),
            }
        })
        .filter(|r| !r.is_empty())
        .chain(["{-1, -12, 0}".to_string(), "{48, -12, 0}".to_string()])
        .collect();
    writeln!(
        out,
        "static const struct os2_case os2_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    metrics_cases(&mut out, "unscaled", &mfont, HbScale::unscaled(&mfont));
    baseline_cases(&mut out, "unscaled", &mfont, HbScale::unscaled(&mfont));
    name_cases(&mut out, &mfont);
    let scale = HbScale::for_font(&mfont, 768, -1536);
    metrics_cases(&mut out, "scaled", &mfont, scale);
    baseline_cases(&mut out, "scaled", &mfont, scale);
    let ems: Vec<String> = [-1000, -3, 0, 3, 501, 1000]
        .iter()
        .map(|&v| format!("{{{v}, {}, {}}}", scale.em_scale_x(v), scale.em_scale_y(v)))
        .collect();
    writeln!(
        out,
        "#define SCALE_X 768\n#define SCALE_Y (-1536)\n\
         static const struct {{ int32_t v; int32_t x; int32_t y; }} em_cases[] = {{{}}};",
        ems.join(", ")
    )
    .unwrap();
    let e = scale.scale_glyph_extents(GlyphExtents {
        x_bearing: 30,
        y_bearing: 700,
        width: 401,
        height: -703,
    });
    ints(&mut out, "int32_t", "scaled_box", &ext4(e));

    // CFF boxes and names (ADR 0021).
    let cff_data = cff_metrics_font();
    bytes(&mut out, "cff_font_data", &cff_data);
    let cfont = Font::new(&cff_data, 0).unwrap();
    metrics_cases(&mut out, "cff_unscaled", &cfont, HbScale::unscaled(&cfont));
    metrics_cases(
        &mut out,
        "cff_scaled",
        &cfont,
        HbScale::for_font(&cfont, 768, -1536),
    );
    cff_name_cases(&mut out, &cfont);
    bytes(&mut out, "cid_font_data", &cid_font());
    unicode(&mut out);

    // Validation.
    diagnostics(&mut out, "invalid_diagnostics", &invalid_data);
    diagnostics(&mut out, "ttc_diagnostics", &ttc_data);
    color_cases(&mut out);
    subset_cases(&mut out, &font_data);
    subset_cff_cases(&mut out, &cff_data);
    out
}

/// The CFF charstring accessors (ADR 0022 stage 11) on the CFF fixture
/// font: the length of the CharStrings INDEX and glyph 1's charstring.
fn subset_cff_cases(out: &mut String, cff_data: &[u8]) {
    let font = Font::new(cff_data, 0).unwrap();
    let index = sofel_subset::cff_charstrings_index(&font).unwrap();
    writeln!(out, "#define CFF_CHARSTRINGS_INDEX_LEN {}", index.len()).unwrap();
    bytes(
        out,
        "cff_charstring_1",
        sofel_subset::cff_charstring_data(&font, 1).unwrap(),
    );
}

/// Unicode properties (ADR 0019): the code points of sofel-unicode's
/// tests and the defaults beyond U+10FFFF.
fn unicode(out: &mut String) {
    let tag = |s: sofel_unicode::Script| u32::from_be_bytes(s.tag());
    let rows: Vec<String> = [
        0x41,
        0x301,
        0x3042,
        0x30A2,
        0x4E00,
        0x30FC,
        0x28,
        0x378,
        0xE000,
        0xD800,
        0xC5,
        0x212B,
        0xAC00,
        0xAC01,
        0x958,
        0x1F600,
        0xA9,
        0x3001,
        0xFF08,
        0x10FFFF,
        0x110000,
        u32::MAX,
    ]
    .iter()
    .map(|&cp| {
        let (status, (a, b)) = match sofel_unicode::decompose(cp) {
            Some(d) => (OK, d),
            None => (NOT_FOUND, (0, 0)),
        };
        format!(
            "{{{cp:#x}, {}, {}, {:#x}, {:#x}, {status}, {a:#x}, {b:#x}, {}, {}}}",
            sofel_unicode::general_category(cp) as u8,
            sofel_unicode::combining_class(cp),
            tag(sofel_unicode::script(cp)),
            sofel_unicode::mirroring(cp),
            u8::from(sofel_unicode::is_extended_pictographic(cp)),
            sofel_unicode::vertical_orientation(cp) as u8,
        )
    })
    .collect();
    writeln!(
        out,
        "#define UNICODE_VERSION {:#x}\nstatic const struct unicode_case unicode_cases[] = {{{}}};",
        {
            let (a, b, c) = sofel_unicode::UNICODE_VERSION;
            (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c)
        },
        rows.join(",\n  ")
    )
    .unwrap();
    let rows: Vec<String> = [
        (0x41, 0x30A),
        (0x1100, 0x1161),
        (0xAC00, 0x11A8),
        (0x915, 0x93C),
        (0, 0x30A),
        (0x41, 0),
    ]
    .iter()
    .map(|&(a, b)| match sofel_unicode::compose(a, b) {
        Some(ab) => format!("{{{a:#x}, {b:#x}, {OK}, {ab:#x}}}"),
        None => format!("{{{a:#x}, {b:#x}, {NOT_FOUND}, 0}}"),
    })
    .collect();
    writeln!(
        out,
        "static const struct compose_case compose_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
    let rows: Vec<String> = ["latn", "Qaai", "Aran", "Lat", "Latnx", "La1n"]
        .iter()
        .map(
            |t| match sofel_unicode::Script::from_iso15924(t.as_bytes()) {
                Some(s) => format!("{{\"{t}\", {OK}, {:#x}}}", tag(s)),
                None => format!("{{\"{t}\", {INVALID_ARGUMENT}, 0}}"),
            },
        )
        .collect();
    writeln!(
        out,
        "static const struct script_case script_cases[] = {{{}}};",
        rows.join(", ")
    )
    .unwrap();
}

#[test]
fn fixture_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/c/fixture.h");
    let generated = generate();
    if std::env::var_os("SOFEL_UPDATE_FIXTURE").is_some() {
        std::fs::write(path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == generated,
        "{path} is out of date; run SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture"
    );
}

/// The version macros of sofel.h follow Cargo.toml.
#[test]
fn header_version_is_the_crate_version() {
    let header = include_str!("../include/sofel.h");
    for (name, value) in [
        ("MAJOR", env!("CARGO_PKG_VERSION_MAJOR")),
        ("MINOR", env!("CARGO_PKG_VERSION_MINOR")),
        ("PATCH", env!("CARGO_PKG_VERSION_PATCH")),
    ] {
        let line = format!("#define SOFEL_VERSION_{name} {value}\n");
        assert!(header.contains(&line), "sofel.h lacks {line:?}");
    }
}

/// The color font of the C tests (ADR 0023): glyph 1 a square, glyph 2
/// COLR version 0 layers (1, palette 1) and (1, foreground), glyph 3 a
/// version 1 PaintGlyph(1) of a linear gradient, glyph 4 PaintGlyph(1)
/// of PaintSolid, glyph 5 nothing; CPAL version 1 with two palettes
/// (the second for dark backgrounds).
fn color_font() -> Vec<u8> {
    let square = simple_glyph(
        &[&[
            Pt::On(100, 0),
            Pt::On(100, 700),
            Pt::On(600, 700),
            Pt::On(600, 0),
        ]],
        false,
        &[],
    );
    let mut glyph1 = Buf::new().u8(10).u24(6).u16(1).done();
    let line = Buf::new()
        .u8(2)
        .u16(2)
        .i16(0x4000)
        .u16(1)
        .i16(0x4000)
        .i16(0)
        .u16(0xFFFF)
        .i16(0x2000)
        .done();
    let mut linear = Buf::new()
        .u8(4)
        .u24(16)
        .i16(0)
        .i16(0)
        .i16(500)
        .i16(100)
        .i16(0)
        .i16(300)
        .done();
    linear.extend(line);
    glyph1.extend(linear);
    let mut glyph2 = Buf::new().u8(10).u24(6).u16(1).done();
    glyph2.extend(Buf::new().u8(2).u16(0).i16(0x3000).done());
    let colr = colr_v1(
        &[(2, 0, 2)],
        &[(1, 1), (1, 0xFFFF)],
        &[glyph1, glyph2],
        &[(3, 0), (4, 1)],
        &[],
        &[],
    );
    let cpal = cpal(
        2,
        &[0, 2],
        &[
            [0, 0, 255, 255],
            [0, 255, 0, 200],
            [255, 0, 0, 255],
            [9, 8, 7, 6],
        ],
        Some((Some(vec![1, 2]), Some(vec![256, 257]), Some(vec![300, 301]))),
    );
    let glyphs = vec![
        Vec::new(),
        square,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    glyph_font_with(
        &glyphs,
        false,
        false,
        vec![(b"COLR", colr), (b"CPAL", cpal)],
    )
}

/// A font without outlines whose EBLC strike of 12 ppem has glyph 1 as a
/// 3×2 one-bit image: rows 101 and 010.
fn bitmap_font() -> Vec<u8> {
    let image = Buf::new()
        .bytes(&[2, 3, 1, 2, 4])
        .bytes(&[0b1010_0000, 0b0100_0000])
        .done();
    let data = Buf::new().u16(2).u16(0).bytes(&image).done();
    let loc = cblc(
        2,
        &[SizeSpec {
            ppem_x: 12,
            ppem_y: 12,
            bit_depth: 1,
            start_glyph: 1,
            end_glyph: 1,
            subtables: vec![(1, 1, index_offsets(1, 1, 4, &[0, image.len() as u32]))],
            ..SizeSpec::default()
        }],
    );
    let mut t = sample_tables();
    t.push((b"EBDT", data));
    t.push((b"EBLC", loc));
    t.sort_by_key(|(tag, _)| **tag);
    sfnt(&t)
}

fn color_cases(out: &mut String) {
    bytes(out, "bitmap_font", &bitmap_font());
    use sofel_color::{PaintOptions, Palette, Recording, Scratch};
    let data = color_font();
    let font = Font::new(&data, 0).unwrap();
    bytes(out, "color_font", &data);
    let texts: Vec<String> = (0..6u32)
        .flat_map(|g| [(g, 0u16), (g, 1)])
        .map(|(g, palette)| {
            let mut rec = Recording::new();
            let options = PaintOptions {
                palette: Palette::Font(palette),
                foreground: sofel_geom::Color::new(10, 20, 30, 255),
                ppem: 0,
            };
            let painted =
                sofel_color::paint_recording(&font, g, &options, &mut Scratch::default(), &mut rec);
            let mut text = sofel_cli::paint::PaintText::new(false);
            rec.replay(&mut text);
            let code = match painted {
                sofel_color::Painted::None => 0,
                sofel_color::Painted::ColrV1 => 1,
                sofel_color::Painted::ColrV0 => 2,
                _ => 9,
            };
            format!("{{{g}, {palette}, {code}, {:?}}}", text.out)
        })
        .collect();
    writeln!(
        out,
        "struct paint_case {{ uint32_t glyph; uint16_t palette; int32_t painted; const char *text; }};\n\
         static const struct paint_case paint_cases[] = {{{}}};",
        texts.join(", ")
    )
    .unwrap();
    let colors: Vec<String> = (0..2u16)
        .flat_map(|p| sofel_color::palette_colors(&font, p))
        .map(|c| format!("{{{}, {}, {}, {}}}", c.r, c.g, c.b, c.a))
        .collect();
    writeln!(
        out,
        "#define COLOR_PALETTES {}\n#define COLOR_ENTRIES {}\n#define COLOR_FORMATS {}u\n\
         static const uint8_t color_palette_colors[][4] = {{{}}};",
        sofel_color::palette_count(&font),
        sofel_color::palette_entry_count(&font),
        sofel_color::color_formats(&font).0,
        colors.join(", ")
    )
    .unwrap();
}

/// The shaping cases of the C tests (ADR 0020): real fonts of the test
/// data, shaped as Golem's `golem_font_shape` calls HarfBuzz (UTF-8 text,
/// LTR or TTB, the script and language as strings, comma-separated
/// features). `(font, text, vertical, script, language, features)`.
const SHAPE_CASES: [(&str, &str, bool, &str, &str, &str); 5] = [
    (
        "harfbuzz/perf/fonts/Roboto-Regular.ttf",
        "AVfi Wa.",
        false,
        "",
        "",
        "kern,liga",
    ),
    (
        "harfbuzz/perf/fonts/Roboto-Regular.ttf",
        "AVfi Wa.",
        false,
        "Latn",
        "en",
        "-liga,kern=0",
    ),
    (
        "harfbuzz/test/api/fonts/Mplus1p-Regular.ttf",
        "「明日」、ー",
        true,
        "",
        "ja",
        "",
    ),
    (
        "harfbuzz/test/api/fonts/Mplus1p-Regular.ttf",
        "「明日」、ー",
        false,
        "",
        "ja",
        "",
    ),
    (
        "harfbuzz/test/api/fonts/Mplus1p-Regular.ttf",
        "明\u{0}x\u{FFFF}",
        true,
        "Hani",
        "",
        "vert=0",
    ),
];

/// A C string literal of `s`, with every byte that is not a plain
/// printable ASCII character as an octal escape.
fn c_string(s: &str) -> String {
    let mut out = String::from("\"");
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b" ,.=-_:[]+/".contains(&b) {
            out.push(char::from(b));
        } else {
            write!(out, "\\{b:03o}").unwrap();
        }
    }
    out.push('"');
    out
}

/// `golem_font_shape` with the Rust API: the glyphs, clusters, flags and
/// positions.
fn shape_case(
    bytes: &[u8],
    text: &str,
    vertical: bool,
    script: &str,
    language: &str,
    features: &str,
) -> (Vec<sofel_shape::GlyphInfo>, Vec<sofel_shape::GlyphPosition>) {
    use sofel_shape::{Direction, Feature, Language, Scale, SegmentProperties, ShapePlan};
    let font = Font::new(bytes, 0).unwrap();
    let outlines = sofel_outline::Outlines::new(font.default_instance());
    let mut buffer = sofel_shape::UnicodeBuffer::new();
    buffer.push_utf8(text.as_bytes(), 0, None);
    let mut tag = [b' '; 4];
    for (t, &c) in tag.iter_mut().zip(script.as_bytes()) {
        *t = c;
    }
    let props = SegmentProperties {
        direction: Some(if vertical {
            Direction::Ttb
        } else {
            Direction::Ltr
        }),
        script: (!script.is_empty()).then(|| sofel_shape::Script::from_iso15924(&tag).unwrap()),
        language: Language::new(language.as_bytes()),
    }
    .guess(&buffer, None);
    let features: Vec<Feature> = features
        .split(',')
        .filter_map(|f| Feature::parse(f.as_bytes()))
        .collect();
    let plan = ShapePlan::new(font.default_instance(), props, &features);
    let boxes = |g: GlyphId| {
        sofel_color::glyph_extents(&outlines, g.0, &mut sofel_color::Scratch::default())
    };
    let glyphs = sofel_shape::shape_with_extents(
        font.default_instance(),
        &Scale::unscaled(&font),
        &plan,
        buffer,
        &boxes,
    );
    (
        glyphs.glyph_infos().to_vec(),
        glyphs.glyph_positions().to_vec(),
    )
}

/// `tests/c/fixture_shape.h`, or `None` without the test data.
fn generate_shape() -> Option<String> {
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata")
        });
    let mut out = String::from(
        "/* Generated by crates/sofel-capi/tests/fixture.rs from the Rust API and\n \
         * the test data (scripts/fetch-testdata.sh); do not edit.\n \
         * `SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture`\n \
         * rewrites it. */\n\n\
         #include <stddef.h>\n#include <stdint.h>\n\n\
         struct shape_case { const char *font; const char *text; size_t text_len; int32_t vertical;\n\
         \x20 const char *script; const char *language; const char *features;\n\
         \x20 size_t n; const uint32_t *glyphs; const uint32_t *clusters; const uint32_t *flags;\n\
         \x20 const int32_t *positions; };\n\n",
    );
    let mut cases = String::new();
    for (i, &(path, text, vertical, script, language, features)) in SHAPE_CASES.iter().enumerate() {
        let bytes = std::fs::read(dir.join(path)).ok()?;
        let (infos, positions) = shape_case(&bytes, text, vertical, script, language, features);
        let list = |v: Vec<String>| v.join(", ");
        writeln!(
            out,
            "static const uint32_t shape_{i}_glyphs[] = {{{}}};",
            list(infos.iter().map(|g| g.glyph.to_string()).collect())
        )
        .unwrap();
        writeln!(
            out,
            "static const uint32_t shape_{i}_clusters[] = {{{}}};",
            list(infos.iter().map(|g| g.cluster.to_string()).collect())
        )
        .unwrap();
        writeln!(
            out,
            "static const uint32_t shape_{i}_flags[] = {{{}}};",
            list(infos.iter().map(|g| g.flags.0.to_string()).collect())
        )
        .unwrap();
        writeln!(
            out,
            "static const int32_t shape_{i}_positions[] = {{{}}};",
            list(
                positions
                    .iter()
                    .flat_map(|p| [p.x_advance, p.y_advance, p.x_offset, p.y_offset])
                    .map(|v| v.to_string())
                    .collect()
            )
        )
        .unwrap();
        writeln!(
            cases,
            "  {{{}, {}, {}, {}, {}, {}, {}, {}, shape_{i}_glyphs, shape_{i}_clusters,\n   \
             shape_{i}_flags, shape_{i}_positions}},",
            c_string(path),
            c_string(text),
            text.len(),
            i32::from(vertical),
            c_string(script),
            c_string(language),
            c_string(features),
            infos.len(),
        )
        .unwrap();
    }
    write!(
        out,
        "static const struct shape_case shape_cases[] = {{\n{cases}}};\n"
    )
    .unwrap();
    Some(out)
}

/// `tests/c/fixture_shape.h` is current; skipped without the test data
/// unless `SOFEL_REQUIRE_TESTDATA` is set.
#[test]
fn shape_fixture_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/c/fixture_shape.h");
    let Some(generated) = generate_shape() else {
        assert!(
            std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
            "no test data; run scripts/fetch-testdata.sh"
        );
        eprintln!("no test data; skipped");
        return;
    };
    if std::env::var_os("SOFEL_UPDATE_FIXTURE").is_some() {
        std::fs::write(path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == generated,
        "{path} is out of date; run SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture"
    );
}

fn generate_ppem() -> String {
    use sofel_shape::{
        Direction as ShapeDirection, Language, Scale, SegmentProperties, ShapePlan, UnicodeBuffer,
    };

    let coverage = coverage1(&[1]);
    let x_device = device(12, 12, 3, &[0x0200]);
    let y_device = device(16, 16, 3, &[0xfd00]);
    let positioning = Buf::new()
        .u16(1)
        .u16(10)
        .u16(0x0030)
        .u16(16)
        .u16(24)
        .bytes(&coverage)
        .bytes(&x_device)
        .bytes(&y_device)
        .done();
    let mut tables = sample_tables();
    tables.push((
        b"GPOS",
        gsubgpos(
            0x0001_0000,
            script_list(vec![(b"DFLT", script(lang_sys(0xFFFF, &[0]), vec![]))]),
            feature_list(&[(b"kern", &[0])]),
            lookup_list(vec![lookup(1, 0, vec![positioning], None)], false),
        ),
    ));
    tables.sort_by_key(|(tag, _)| **tag);
    let data = sfnt(&tables);
    let font = Font::new(&data, 0).unwrap();
    let buffer = || {
        let mut buffer = UnicodeBuffer::new();
        buffer.push_utf8(b"A", 0, None);
        buffer
    };
    let props = SegmentProperties {
        direction: Some(ShapeDirection::Ltr),
        script: sofel_shape::Script::from_iso15924(b"Latn"),
        language: None::<Language>,
    }
    .guess(&buffer(), None);
    let instance = font.default_instance();
    let plan = ShapePlan::new(instance, props.clone(), &[]);
    let position = |x_ppem, y_ppem, fallback| {
        let scale = Scale::new(HbScale::for_font(&font, 1000, 2000), x_ppem, y_ppem);
        let glyphs = if fallback {
            sofel_shape::shape_fallback(instance, &scale, &props, buffer())
        } else {
            sofel_shape::shape(instance, &scale, &plan, buffer())
        };
        let positions = glyphs.glyph_positions();
        assert_eq!(positions.len(), 1);
        let p = positions[0];
        [p.x_advance, p.y_advance, p.x_offset, p.y_offset]
    };
    let mut out = String::from(
        "/* Generated by crates/sofel-capi/tests/fixture.rs; do not edit. */\n\n#include <stdint.h>\n\n",
    );
    bytes(&mut out, "ppem_font_data", &data);
    out.push_str("struct ppem_case { uint32_t x, y; int32_t normal[4], fallback[4]; };\n");
    out.push_str("static const struct ppem_case ppem_cases[] = {\n");
    for (x, y) in [(0, 0), (1, 0), (0, 1), (12, 16), (u32::MAX, 1)] {
        let normal = position(x, y, false);
        let fallback = position(x, y, true);
        writeln!(
            out,
            "  {{{x}, {y}, {{{}, {}, {}, {}}}, {{{}, {}, {}, {}}}}},",
            normal[0],
            normal[1],
            normal[2],
            normal[3],
            fallback[0],
            fallback[1],
            fallback[2],
            fallback[3]
        )
        .unwrap();
    }
    out.push_str("};\n");
    let null_position = |fallback| {
        let glyphs = if fallback {
            sofel_shape::shape_fallback(instance, &Scale::unscaled(&font), &props, buffer())
        } else {
            sofel_shape::shape(instance, &Scale::unscaled(&font), &plan, buffer())
        };
        let p = glyphs.glyph_positions()[0];
        [p.x_advance, p.y_advance, p.x_offset, p.y_offset]
    };
    for (name, value) in [
        ("normal", null_position(false)),
        ("fallback", null_position(true)),
    ] {
        writeln!(
            out,
            "static const int32_t ppem_null_{name}[4] = {{{}, {}, {}, {}}};",
            value[0], value[1], value[2], value[3]
        )
        .unwrap();
    }
    let with_ppem = position(12, 16, false);
    let without_ppem = position(0, 0, false);
    assert_ne!(with_ppem[2], without_ppem[2]);
    assert_ne!(with_ppem[3], without_ppem[3]);
    out
}

#[test]
fn ppem_fixture_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/c/fixture_ppem.h");
    let generated = generate_ppem();
    if std::env::var_os("SOFEL_UPDATE_FIXTURE").is_some() {
        std::fs::write(path, &generated).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap_or_default(), generated);
}

/// FNV-1a 64 of bytes: the C tests compare subset fonts by length and
/// hash.
fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| {
        (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3)
    })
}

fn tag_u32(t: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*t)
}

/// The input that the C test builds with the input functions and
/// compares through `sofel_subset_input_to_args` (ADR 0022).
fn args_input() -> sofel_subset::Input {
    use sofel_subset::{Flags, Input, SetKind};
    Input::new()
        .map_set(SetKind::Unicodes, |s| {
            s.add_range(0x41, 0x43);
            s.remove(0x42);
        })
        .map_set(SetKind::Glyphs, |s| s.add(2))
        .map_set(SetKind::DropTables, |s| s.add(tag_u32(b"GPOS")))
        .map_set(SetKind::NameIds, |s| {
            s.clear();
            s.add(1);
            s.add(2);
        })
        .map_set(SetKind::LayoutFeatures, |s| s.invert())
        .map_set(SetKind::LayoutScripts, |s| s.remove(tag_u32(b"latn")))
        .with_flags(Flags::RETAIN_GIDS | Flags::NO_HINTING)
        .with_glyph_mapping([(5, 7)].into_iter().collect())
}

/// The inputs of the C tests' plans: characters; glyphs with a glyph
/// mapping; a name override. Each runs through `sofel_subset`, a plan and
/// its mappings.
fn plan_inputs() -> Vec<sofel_subset::Input> {
    use sofel_subset::{Input, SetKind};
    vec![
        Input::new().map_set(SetKind::Unicodes, |s| {
            s.add(0x41);
            s.add(0x43);
        }),
        Input::new()
            .map_set(SetKind::Glyphs, |s| {
                s.add(1);
                s.add(2);
            })
            .with_glyph_mapping([(1, 2), (2, 1)].into_iter().collect()),
        Input::new()
            .map_set(SetKind::Unicodes, |s| s.add(0x41))
            .with_name_override((3, 1, 0x409, 1), b"Sofel".to_vec()),
    ]
}

fn pair_list(v: &[(u32, u32)]) -> String {
    v.iter()
        .flat_map(|&(a, b)| [a, b])
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The subsetting cases of the C tests (ADR 0022) on the fixture font.
fn subset_cases(out: &mut String, font_data: &[u8]) {
    use sofel_subset::{Flags, Plan, SubsetError, axis_range_from_string, subset};
    let font = Font::new(font_data, 0).unwrap();
    writeln!(
        out,
        "#define SUBSET_ARGS {}",
        c_string(&args_input().to_args().join(" "))
    )
    .unwrap();
    out.push_str(
        "struct subset_case { int32_t status; uint32_t num_output_glyphs; size_t len; uint64_t fnv;\n\
         \x20 size_t n_old_new; const uint32_t *old_new; size_t n_new_old; const uint32_t *new_old;\n\
         \x20 size_t n_unicode_old; const uint32_t *unicode_old; };\n",
    );
    let mut rows = Vec::new();
    for (i, input) in plan_inputs().iter().enumerate() {
        let plan = match Plan::new(&font, input).and_then(|p| p.execute(&font).map(|_| p)) {
            Ok(p) => p,
            Err(e) => {
                assert_eq!(subset(&font, input).err(), Some(e));
                let status = match e {
                    SubsetError::Unsupported { .. } => -18,
                    _ => panic!("{e:?}"),
                };
                rows.push(format!("{{{status}, 0, 0, 0, 0, NULL, 0, NULL, 0, NULL}}"));
                continue;
            }
        };
        let bytes = plan.execute(&font).unwrap();
        assert_eq!(subset(&font, input).unwrap(), bytes);
        // A plan made with the prepared value gives the same bytes.
        let prepared = sofel_subset::Prepared::new(&font);
        let fast = Plan::with_prepared(&font, &prepared, input).unwrap();
        assert_eq!(fast.execute(&font).unwrap(), bytes);
        let old_new = plan.old_to_new_glyphs();
        let new_old = plan.new_to_old_glyphs().to_vec();
        let unicode_old: Vec<(u32, u32)> = plan.unicode_to_old_glyphs().collect();
        for (name, v) in [
            ("old_new", &old_new),
            ("new_old", &new_old),
            ("unicode_old", &unicode_old),
        ] {
            writeln!(
                out,
                "static const uint32_t subset_{i}_{name}[] = {{{}0}};",
                pair_list(v)
                    .split(", ")
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("{s}, "))
                    .collect::<String>()
            )
            .unwrap();
        }
        rows.push(format!(
            "{{{OK}, {}, {}, {:#x}u, {}, subset_{i}_old_new, {}, subset_{i}_new_old, {}, \
             subset_{i}_unicode_old}}",
            plan.num_output_glyphs(),
            bytes.len(),
            fnv(&bytes),
            old_new.len(),
            new_old.len(),
            unicode_old.len()
        ));
    }
    writeln!(
        out,
        "static const struct subset_case subset_cases[] = {{\n  {},\n}};",
        rows.join(",\n  ")
    )
    .unwrap();
    // RETAIN_GIDS with a glyph mapping: HarfBuzz refuses it.
    let refused = plan_inputs()[1].clone().with_flags(Flags::RETAIN_GIDS);
    assert_eq!(
        Plan::new(&font, &refused).err(),
        Some(SubsetError::InvalidInput)
    );
    writeln!(out, "#define SUBSET_INVALID_INPUT (-16)").unwrap();

    // Axis ranges as strings: (string, status, bits of min, default, max).
    out.push_str("struct axis_string_case { const char *s; int32_t status; uint32_t bits[3]; };\n");
    let rows: Vec<String> = [
        "drop",
        "100",
        "100:200",
        "100:150:200",
        ":150:",
        "1:2:3:4",
        "x",
        "",
    ]
    .iter()
    .map(|s| match axis_range_from_string(s) {
        Some((a, d, b)) => {
            let bits = |v: Option<f32>| format!("{:#x}u", v.unwrap_or(f32::NAN).to_bits());
            format!(
                "{{{}, {OK}, {{{}, {}, {}}}}}",
                c_string(s),
                bits(a),
                bits(d),
                bits(b)
            )
        }
        None => format!("{{{}, {INVALID_ARGUMENT}, {{0, 0, 0}}}}", c_string(s)),
    })
    .collect();
    writeln!(
        out,
        "static const struct axis_string_case axis_string_cases[] = {{\n  {},\n}};",
        rows.join(",\n  ")
    )
    .unwrap();
}

/// Golem's subsetting (ADR 0022): the fonts of the C tests, whose glyph
/// sets are those of the hb-dump comparisons (sofel-conformance's
/// `hb-dump/golem/<font>:keep`).
const GOLEM_FONTS: [&str; 2] = [
    "harfbuzz/test/subset/data/fonts/Roboto-Regular.ttf",
    "harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf",
];

/// `tests/c/fixture_subset.h`, or `None` without the test data. When
/// `hb-dump` (tools/hb-dump/build.sh; `SOFEL_HB_DUMP` overrides
/// target/hb-dump/hb-dump) is there, the Rust output is also checked
/// against HarfBuzz's for the same arguments, so that the C test's
/// output, equal to the Rust output, is HarfBuzz's.
fn generate_subset() -> Option<String> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/testdata"));
    let hb_dump = std::env::var_os("SOFEL_HB_DUMP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/hb-dump/hb-dump"));
    let texts: Vec<String> = [
        "harfbuzz/perf/texts/ja-runs.txt",
        "harfbuzz/perf/texts/en-paragraph.txt",
    ]
    .iter()
    .map(|t| std::fs::read_to_string(dir.join(t)).ok())
    .collect::<Option<Vec<_>>>()?;
    let mut out = String::from(
        "/* Generated by crates/sofel-capi/tests/fixture.rs from the Rust API and\n \
         * the test data (scripts/fetch-testdata.sh); do not edit.\n \
         * `SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture`\n \
         * rewrites it. */\n\n\
         #include <stddef.h>\n#include <stdint.h>\n\n\
         struct golem_case { const char *font; size_t n; const uint32_t *glyphs; size_t len;\n\
         \x20 uint64_t fnv; };\n\n",
    );
    let mut cases = String::new();
    for (i, path) in GOLEM_FONTS.iter().enumerate() {
        let bytes = std::fs::read(dir.join(path)).ok()?;
        let font = Font::new(&bytes, 0).unwrap();
        let mut glyphs: std::collections::BTreeSet<u32> = texts
            .iter()
            .flat_map(|t| t.chars())
            .filter_map(|c| font.nominal_glyph(u32::from(c)).map(|g| g.0))
            .collect();
        glyphs.insert(1);
        glyphs.insert(font.num_glyphs() - 1);
        let glyphs: Vec<u32> = glyphs.into_iter().collect();
        let input = sofel_subset::Input::new()
            .map_set(sofel_subset::SetKind::Glyphs, |s| {
                glyphs.iter().for_each(|&g| s.add(g))
            })
            .with_flags(sofel_subset::Flags::RETAIN_GIDS);
        let subset = sofel_subset::subset(&font, &input).unwrap();
        if hb_dump.exists() {
            let tmp = std::env::temp_dir().join(format!("sofel-capi-golem-{}", std::process::id()));
            let list: Vec<String> = glyphs.iter().map(u32::to_string).collect();
            let status = std::process::Command::new(&hb_dump)
                .arg("--subset")
                .arg(format!("--font-file={}", dir.join(path).display()))
                .arg(format!("--gids={}", list.join(",")))
                .arg("--retain-gids")
                .arg(format!("--output-file={}", tmp.display()))
                .status()
                .unwrap();
            assert!(status.success(), "hb-dump --subset failed for {path}");
            let theirs = std::fs::read(&tmp).unwrap();
            let _ = std::fs::remove_file(&tmp);
            assert!(
                theirs == subset,
                "{path}: sofel's subset differs from HarfBuzz's"
            );
        } else {
            eprintln!("no hb-dump; the Golem cases are not checked against HarfBuzz");
        }
        writeln!(
            out,
            "static const uint32_t golem_{i}_glyphs[] = {{{}}};",
            glyphs
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        writeln!(
            cases,
            "  {{{}, {}, golem_{i}_glyphs, {}, {:#x}u}},",
            c_string(path),
            glyphs.len(),
            subset.len(),
            fnv(&subset)
        )
        .unwrap();
    }
    write!(
        out,
        "static const struct golem_case golem_cases[] = {{\n{cases}}};\n"
    )
    .unwrap();
    axis_cases(&mut out, &dir.join(AXIS_FONT))?;
    Some(out)
}

/// The variable font of the C tests of the axis functions.
const AXIS_FONT: &str = "harfbuzz/test/subset/data/fonts/Roboto-Variable.ABC.ttf";

/// The axis functions of the C tests (ADR 0022) on [`AXIS_FONT`]: the
/// ranges of `wght` after pinning it to its default, pinning it at 1000
/// and restricting its maximum to 500, as the bits of min, default and
/// max; and the subset of U+0041..U+0043 with every axis pinned to its
/// default.
fn axis_cases(out: &mut String, path: &std::path::Path) -> Option<()> {
    use sofel_subset::{AxisRange, Input, SetKind};
    let bytes = std::fs::read(path).ok()?;
    let font = Font::new(&bytes, 0).unwrap();
    let wght = sofel_font::read::Tag::new(b"wght");
    let inputs = [
        Input::new().with_axis_pinned_to_default(wght, &font),
        Input::new().with_axis_pinned(wght, 1000.0, &font),
        Input::new().with_axis_range(wght, None, Some(500.0), None, &font),
    ];
    let rows: Vec<String> = inputs
        .iter()
        .map(|i| {
            let AxisRange { min, default, max } = i.as_ref().unwrap().axis_range(wght).unwrap();
            format!(
                "{{{:#x}u, {:#x}u, {:#x}u}}",
                min.to_bits(),
                default.to_bits(),
                max.to_bits()
            )
        })
        .collect();
    let input = Input::new()
        .map_set(SetKind::Unicodes, |s| s.add_range(0x41, 0x43))
        .with_all_axes_pinned_to_default(&font)
        .unwrap();
    let subset = sofel_subset::subset(&font, &input).unwrap();
    write!(
        out,
        "\n#define AXIS_FONT {}\n\
         static const uint32_t axis_ranges[3][3] = {{\n  {},\n}};\n\
         static const size_t axis_pinned_len = {};\n\
         static const uint64_t axis_pinned_fnv = {:#x}u;\n",
        c_string(AXIS_FONT),
        rows.join(",\n  "),
        subset.len(),
        fnv(&subset)
    )
    .unwrap();
    Some(())
}

/// `tests/c/fixture_subset.h` is current; skipped without the test data
/// unless `SOFEL_REQUIRE_TESTDATA` is set.
#[test]
fn subset_fixture_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/c/fixture_subset.h");
    let Some(generated) = generate_subset() else {
        assert!(
            std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
            "no test data; run scripts/fetch-testdata.sh"
        );
        eprintln!("no test data; skipped");
        return;
    };
    if std::env::var_os("SOFEL_UPDATE_FIXTURE").is_some() {
        std::fs::write(path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == generated,
        "{path} is out of date; run SOFEL_UPDATE_FIXTURE=1 cargo test -p sofel-capi --test fixture"
    );
}
