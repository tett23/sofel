//! Benchmarks of sofel against FreeType 2.15.0 and HarfBuzz 15.1.0
//! (ADR 0006, ADR 0013). Run from the repository root:
//!
//!   cargo run --release --manifest-path bench/Cargo.toml -- [--once] [--filter S] [--testdata DIR]

mod ffi;
mod shape;
mod subset;

use std::path::PathBuf;

use ffi::{FtFace, HbFont, HbUnicode};
use sofel_bench::harness::{Check, Config, Row, Stats, SystemClock, measure, report_checks, table};
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_geom::{Command, FnPen};
use sofel_outline::{HbExtents, Outlines, Scale, Scratch};
use sofel_raster::Rasterizer;

const FONTS: [(&str, &str); 5] = [
    ("Roboto-Regular", "harfbuzz/perf/fonts/Roboto-Regular.ttf"),
    (
        "Comfortaa-Regular-new",
        "harfbuzz/test/subset/data/fonts/Comfortaa-Regular-new.ttf",
    ),
    (
        "NotoNastaliqUrdu-Regular",
        "harfbuzz/perf/fonts/NotoNastaliqUrdu-Regular.ttf",
    ),
    (
        "RobotoFlex-Variable",
        "harfbuzz/test/subset/data/fonts/RobotoFlex-Variable.ttf",
    ),
    (
        "SourceSerifVariable-Roman",
        "harfbuzz/test/subset/data/fonts/SourceSerifVariable-Roman.ttf",
    ),
];

/// One file for each role in ADR 0031's fixed extra corpus. `--once`
/// checks bounded cmap and advance samples against both references.
const EXTRA_FONTS: [(&str, &str); 13] = [
    ("extra/arabic", "extra-fonts/fonts/arabic.ttf"),
    ("extra/devanagari", "extra-fonts/fonts/devanagari.ttf"),
    ("extra/khmer", "extra-fonts/fonts/khmer.ttf"),
    ("extra/myanmar", "extra-fonts/fonts/myanmar.ttf"),
    ("extra/balinese", "extra-fonts/fonts/balinese.ttf"),
    ("extra/hebrew", "extra-fonts/fonts/hebrew.ttf"),
    ("extra/source-serif", "extra-fonts/fonts/source-serif.otf"),
    ("extra/stix-math", "extra-fonts/fonts/stix-math.otf"),
    ("extra/source-han", "extra-fonts/fonts/source-han.ttc"),
    ("extra/amstelvar", "extra-fonts/fonts/amstelvar.ttf"),
    ("extra/color-cff", "extra-fonts/fonts/color-cff.otf"),
    ("extra/color-sbix", "extra-fonts/fonts/color-sbix.ttf"),
    ("extra/color-svg", "extra-fonts/fonts/color-svg.ttf"),
];

/// The CFF and CFF2 fonts of HarfBuzz's perf/benchmark-font.cc (the
/// CFF2 one at its default instance) and a CID-keyed Japanese font
/// (ADR 0021).
const CFF_FONTS: [(&str, &str); 4] = [
    (
        "SourceSansPro-Regular",
        "harfbuzz/test/subset/data/fonts/SourceSansPro-Regular.otf",
    ),
    (
        "NotoSerifMyanmar-Regular",
        "harfbuzz/test/subset/data/fonts/NotoSerifMyanmar-Regular.otf",
    ),
    (
        "AdobeVFPrototype",
        "harfbuzz/test/subset/data/fonts/AdobeVFPrototype.otf",
    ),
    (
        "SourceHanSans-Regular_subset",
        "harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf",
    ),
];

const TEXTS: [&str; 4] = [
    "harfbuzz/perf/texts/en-paragraph.txt",
    "harfbuzz/perf/texts/fa-paragraph.txt",
    "harfbuzz/perf/texts/hi-words.txt",
    "harfbuzz/perf/texts/ja-runs.txt",
];

/// The color fonts of ADR 0023: COLRv0, COLRv1 (glyf and CFF layers
/// alike), CBDT, sbix and SVG.
const COLOR_FONTS: [(&str, &str); 6] = [
    (
        "TwemojiMozilla.subset",
        "harfbuzz/test/subset/data/fonts/TwemojiMozilla.subset.ttf",
    ),
    (
        "test_glyphs-glyf_colr_1",
        "harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1.ttf",
    ),
    (
        "noto_handwriting-glyf_colr_1",
        "harfbuzz/test/fuzzing/fonts/noto_handwriting-glyf_colr_1.ttf",
    ),
    (
        "NotoColorEmoji.subset",
        "harfbuzz/test/subset/data/fonts/NotoColorEmoji.subset.ttf",
    ),
    (
        "noto_handwriting-sbix",
        "harfbuzz/test/fuzzing/fonts/noto_handwriting-sbix.ttf",
    ),
    (
        "chromacheck-svg",
        "harfbuzz/test/api/fonts/chromacheck-svg.ttf",
    ),
];

/// Every text of HarfBuzz's perf/texts/, for the Unicode properties.
const ALL_TEXTS: &str = "harfbuzz/perf/texts";

struct Args {
    once: bool,
    filter: Option<String>,
    testdata: PathBuf,
}

fn args() -> Args {
    let mut a = Args {
        once: false,
        filter: None,
        testdata: PathBuf::from("target/testdata"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(s) = it.next() {
        match s.as_str() {
            "--once" => a.once = true,
            "--filter" => a.filter = it.next(),
            "--testdata" => a.testdata = it.next().map(PathBuf::from).unwrap_or(a.testdata),
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    a
}

/// The body of one item for one implementation; returns a value that is
/// checked against sofel's.
type Body<'a> = &'a mut dyn FnMut() -> u64;

/// Whether another implementation's result is compared with sofel's.
#[derive(Clone, Copy, PartialEq)]
enum Compare {
    Yes,
    /// Only timed: the result is computed differently (e.g. HarfBuzz
    /// decomposes glyf outlines into other commands) or checked apart.
    No,
}

struct Bench {
    once: bool,
    filter: Option<String>,
    rows: Vec<Row>,
    checks: Vec<Check>,
}

impl Bench {
    fn wanted(&self, font: &str, item: &str) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|f| item.contains(f.as_str()) || font.contains(f.as_str()))
    }

    fn item(
        &mut self,
        font: &str,
        item: String,
        sofel: Body<'_>,
        freetype: Option<(Body<'_>, Compare)>,
        harfbuzz: Option<(Body<'_>, Compare)>,
    ) {
        if !self.wanted(font, &item) {
            return;
        }
        eprintln!("{font}: {item}");
        // Check the results once before timing.
        let ours = sofel();
        let timed = |who: &str, other: Option<(Body<'_>, Compare)>, checks: &mut Vec<Check>| {
            let (body, cmp) = other?;
            let theirs = body();
            if cmp == Compare::Yes {
                checks.push(Check {
                    item: format!("{item} ({who})"),
                    font: font.to_string(),
                    ok: theirs == ours,
                    detail: format!("sofel {ours}, {who} {theirs}"),
                });
            }
            time(self.once, body)
        };
        let f = timed("FreeType", freetype, &mut self.checks);
        let h = timed("HarfBuzz", harfbuzz, &mut self.checks);
        let s = time(self.once, sofel);
        self.rows.push(Row {
            item,
            font: font.to_string(),
            sofel: s,
            freetype: f,
            harfbuzz: h,
        });
    }
}

fn time(once: bool, body: Body<'_>) -> Option<Stats> {
    if once {
        std::hint::black_box(body());
        None
    } else {
        Some(measure(
            &mut SystemClock::default(),
            body,
            &Config::default(),
        ))
    }
}

fn extra_items(b: &mut Bench, testdata: &std::path::Path) {
    const CHARS: [u32; 10] = [
        0x41, 0x627, 0x915, 0x1780, 0x1000, 0x1b05, 0x5b57, 0x2211, 0x5d0, 0x1f600,
    ];
    for (name, path) in EXTRA_FONTS {
        if !b.wanted(name, "cmap") && !b.wanted(name, "advance") {
            continue;
        }
        let path = testdata.join(path);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run scripts/fetch-fonts.sh", path.display()));
        let font = Font::new(&bytes, 0).expect("sofel opens the extra font");
        let ft = FtFace::open(&bytes).expect("FreeType opens the extra font");
        let hb = HbFont::open(&bytes).expect("HarfBuzz opens the extra font");
        hb.set_ppem(0);
        let n = font.num_glyphs().min(128);
        b.item(
            name,
            "cmap".into(),
            &mut || {
                CHARS
                    .iter()
                    .map(|&c| u64::from(font.nominal_glyph(c).map_or(0, |g| g.0)))
                    .sum()
            },
            Some((
                &mut || CHARS.iter().map(|&c| u64::from(ft.char_index(c))).sum(),
                Compare::Yes,
            )),
            Some((
                &mut || CHARS.iter().map(|&c| u64::from(hb.nominal_glyph(c))).sum(),
                Compare::Yes,
            )),
        );
        b.item(
            name,
            "advance".into(),
            &mut || (0..n).map(|g| u64::from(font.advance(GlyphId(g)))).sum(),
            Some((
                &mut || (0..n).map(|g| wrap(ft.advance(g))).sum(),
                Compare::Yes,
            )),
            Some((
                &mut || (0..n).map(|g| wrap(hb.advance(g))).sum(),
                Compare::Yes,
            )),
        );
    }
}

/// FreeType's and HarfBuzz's results, with failures (-1) kept distinct.
fn wrap(v: i64) -> u64 {
    v as u64
}

/// The Unicode properties of every character of every text of
/// perf/texts/ (ADR 0019): the sum of General_Category,
/// Canonical_Combining_Class, the script tag and Bidi_Mirroring_Glyph.
fn unicode_properties(b: &mut Bench, testdata: &std::path::Path) {
    let dir = testdata.join(ALL_TEXTS);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}; run scripts/fetch-testdata.sh", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    files.sort();
    let text: Vec<u32> = files
        .iter()
        .flat_map(|p| {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .chars()
                .collect::<Vec<_>>()
        })
        .map(u32::from)
        .collect();
    let hb = HbUnicode::get();
    b.item(
        "perf/texts",
        format!("Unicode properties ({} characters)", text.len()),
        &mut || {
            text.iter()
                .map(|&c| {
                    u64::from(sofel_unicode::general_category(c) as u8)
                        + u64::from(sofel_unicode::combining_class(c))
                        + u64::from(u32::from_be_bytes(sofel_unicode::script(c).tag()))
                        + u64::from(sofel_unicode::mirroring(c))
                })
                .sum()
        },
        None,
        Some((&mut || text.iter().map(|&c| hb.sum(c)).sum(), Compare::Yes)),
    );
}

/// A box as one number, the way the shim sums HarfBuzz's.
fn box_sum(e: Option<sofel_font::GlyphExtents>) -> u64 {
    e.map_or(wrap(i64::MIN), |e| {
        wrap(
            i64::from(e.x_bearing)
                + i64::from(e.y_bearing)
                + i64::from(e.width)
                + i64::from(e.height),
        )
    })
}

/// The HarfBuzz items of the CFF and CFF2 fonts (ADR 0021): glyph boxes,
/// glyph names and names to glyphs, and opening the font with the first
/// box. The outline, raster and FreeType items are added with the
/// outlines.
fn cff_items(b: &mut Bench, testdata: &std::path::Path) {
    for (name, path) in CFF_FONTS {
        let Ok(bytes) = std::fs::read(testdata.join(path)) else {
            eprintln!(
                "{}: missing; run scripts/fetch-testdata.sh",
                testdata.join(path).display()
            );
            std::process::exit(1);
        };
        let font = Font::new(&bytes, 0).expect("sofel opens the font");
        let hb = HbFont::open(&bytes).expect("HarfBuzz opens the font");
        hb.set_ppem(0);
        let n = font.num_glyphs();
        // FreeType's half (ADR 0021): outlines, rendering, CIDs, opening.
        let ft = FtFace::open(&bytes).expect("FreeType opens the font");
        outline_items(b, name, &font, &ft, &hb);
        hb.set_ppem(0);
        let cff = sofel_font::Cff::new(&font).expect("sofel opens the CFF");
        if cff.is_cid_keyed() {
            // ADR 0025: lookups through the charset index, which the
            // "open (FreeType)" item builds; and without it, the cost of
            // a caller that looks glyphs up once each.
            let charset = cff.charset().expect("a CFF charset");
            let mut starts = vec![0u32; charset.num_ranges()];
            let index = charset.index_into(&mut starts).expect("long enough");
            let mut ft_cids = || (0..n).map(|g| wrap(ft.cid(g))).fold(0, u64::wrapping_add);
            b.item(
                name,
                "cid".into(),
                &mut || {
                    (0..n)
                        .map(|g| cff.cid_with(&index, GlyphId(g)).map_or(u64::MAX, u64::from))
                        .fold(0, u64::wrapping_add)
                },
                Some((&mut ft_cids, Compare::Yes)),
                None,
            );
            b.item(
                name,
                "cid (no index)".into(),
                &mut || {
                    (0..n)
                        .map(|g| cff.cid(GlyphId(g)).map_or(u64::MAX, u64::from))
                        .fold(0, u64::wrapping_add)
                },
                Some((&mut ft_cids, Compare::Yes)),
                None,
            );
        }
        // Opening includes building the charset index (ADR 0025) with
        // its array, as FreeType's cff_charset_load builds its SID array.
        b.item(
            name,
            "open (FreeType)".into(),
            &mut || {
                sofel_font::Cff::new(&font).map_or(0, |c| {
                    if let Some(cs) = c.charset() {
                        let mut starts = vec![0u32; cs.num_ranges()];
                        std::hint::black_box(cs.index_into(&mut starts).ok());
                    }
                    u64::from(c.num_glyphs())
                })
            },
            Some((
                &mut || FtFace::open(&bytes).map_or(0, |f| u64::from(f.num_glyphs())),
                Compare::Yes,
            )),
            None,
        );
        let extents = HbExtents::new(&font);
        b.item(
            name,
            "glyph extents".into(),
            &mut || {
                (0..n)
                    .map(|g| box_sum(extents.glyph_extents(GlyphId(g))))
                    .fold(0, u64::wrapping_add)
            },
            None,
            Some((
                &mut || {
                    (0..n)
                        .map(|g| hb.glyph_extents(g).map_or(wrap(i64::MIN), wrap))
                        .fold(0, u64::wrapping_add)
                },
                Compare::Yes,
            )),
        );

        let first = 1.min(n.saturating_sub(1));
        b.item(
            name,
            "open (HarfBuzz)".into(),
            &mut || {
                let f = Font::new(&bytes, 0).expect("sofel opens the font");
                box_sum(HbExtents::new(&f).glyph_extents(GlyphId(first)))
            },
            None,
            Some((
                &mut || HbFont::open_extents(&bytes, first).map_or(wrap(i64::MIN), wrap),
                Compare::Yes,
            )),
        );

        let mut buf = vec![0u32; font.glyph_names_len()];
        let Some(names) = font.glyph_names(&mut buf) else {
            continue;
        };
        if extents.cff1().is_none_or(|c| c.is_cid()) {
            continue;
        }
        // The name as hb_font_get_glyph_name copies it: cut at a 0 byte
        // and at 255 bytes.
        let cut = |s: &[u8]| -> usize {
            let s = &s[..s.len().min(255)];
            s.iter().position(|&c| c == 0).unwrap_or(s.len())
        };
        let mut hb_buf = [0u8; 256];
        b.item(
            name,
            "glyph names".into(),
            &mut || {
                (0..n)
                    .map(|g| {
                        names
                            .glyph_name(GlyphId(g))
                            .map_or(wrap(-1), |s| cut(s) as u64)
                    })
                    .fold(0, u64::wrapping_add)
            },
            None,
            Some((
                &mut || {
                    (0..n)
                        .map(|g| hb.glyph_name_len(g, &mut hb_buf).map_or(wrap(-1), wrap))
                        .fold(0, u64::wrapping_add)
                },
                Compare::Yes,
            )),
        );
        let all: Vec<&[u8]> = (0..n)
            .filter_map(|g| names.glyph_name(GlyphId(g)))
            .map(|s| &s[..cut(s)])
            .collect();
        b.item(
            name,
            "glyph from name".into(),
            &mut || {
                all.iter()
                    .map(|s| {
                        names
                            .glyph_from_name(s)
                            .map_or(wrap(-1), |g| u64::from(g.0))
                    })
                    .fold(0, u64::wrapping_add)
            },
            None,
            Some((
                &mut || {
                    all.iter()
                        .map(|s| hb.glyph_from_name(s).map_or(wrap(-1), u64::from))
                        .fold(0, u64::wrapping_add)
                },
                Compare::Yes,
            )),
        );
    }
}

/// Outlines (ADR 0011, ADR 0021): the number of commands (without
/// closes) of every glyph, unscaled and at 48 px, against FreeType (and
/// HarfBuzz's drawing, not compared); rendering (ADR 0012) at 12, 24, 48
/// and 96 px against FreeType.
fn outline_items(b: &mut Bench, name: &str, font: &Font<'_>, ft: &FtFace, hb: &HbFont) {
    let n = font.num_glyphs();
    let outlines = Outlines::new(font);
    // Outlines: the number of commands (without closes) of every glyph.
    for ppem in [0u16, 48] {
        let scale = if ppem == 0 {
            Scale::Unscaled
        } else {
            Scale::pixels(font, ppem, ppem)
        };
        if ppem > 0 {
            ft.set_ppem(u32::from(ppem));
        }
        hb.set_ppem(u32::from(ppem));
        let mut scratch = Scratch::default();
        let label = if ppem == 0 {
            "outline unscaled".to_string()
        } else {
            format!("outline {ppem}px")
        };
        b.item(
            name,
            label,
            &mut || {
                (0..n)
                    .map(|g| {
                        let mut count = 0u64;
                        let mut pen = FnPen(|c| {
                            if c != Command::Close {
                                count += 1;
                            }
                        });
                        match outlines.draw(GlyphId(g), scale, &mut scratch, &mut pen) {
                            Ok(_) => count,
                            Err(_) => u64::MAX,
                        }
                    })
                    .fold(0u64, u64::wrapping_add)
            },
            Some((
                &mut || {
                    (0..n)
                        .map(|g| wrap(ft.outline(g, ppem == 0)))
                        .fold(0, u64::wrapping_add)
                },
                Compare::Yes,
            )),
            Some((
                &mut || (0..n).map(|g| wrap(hb.draw(g))).fold(0, u64::wrapping_add),
                Compare::No,
            )),
        );
    }

    // Rendering: the sum of all pixels of every glyph.
    eprintln!("{name}: {n} glyphs");
    for ppem in [12u16, 24, 48, 96] {
        ft.set_ppem(u32::from(ppem));
        let mut scratch = Scratch::default();
        let mut raster = Rasterizer::new();
        let mut ours = |glyphs: &mut dyn Iterator<Item = u32>| {
            glyphs
                .map(|g| {
                    match sofel_cli::glyph::render(
                        &outlines,
                        GlyphId(g),
                        ppem,
                        &mut scratch,
                        &mut raster,
                    ) {
                        Ok(bm) => bm.pixels.iter().map(|&v| u64::from(v)).sum(),
                        Err(_) => u64::MAX,
                    }
                })
                .fold(0u64, u64::wrapping_add)
        };
        let theirs = |glyphs: &mut dyn Iterator<Item = u32>| {
            glyphs
                .map(|g| wrap(ft.render(g)))
                .fold(0, u64::wrapping_add)
        };
        let item = format!("raster {ppem}px");
        if b.wanted(name, &item) {
            let (o, t) = (ours(&mut (0..n)), theirs(&mut (0..n)));
            b.checks.push(Check {
                item: format!("{item} (FreeType)"),
                font: name.to_string(),
                ok: o == t,
                detail: format!("sofel {o}, FreeType {t}"),
            });
        }
        b.item(
            name,
            item,
            &mut || ours(&mut (0..n)),
            Some((&mut || theirs(&mut (0..n)), Compare::No)),
            None,
        );
    }
}

/// An EBLC/EBDT font built for the benchmark (the test data has none):
/// `n` glyphs of 16×16 8-bit pixels in one 16-ppem strike, index format
/// 1, image format 1, after the tables of a minimal font.
fn ebdt_font(n: u16) -> Vec<u8> {
    let mut seed = 0x1234_5678u32;
    let mut data = vec![0, 2, 0, 0];
    let mut offsets = vec![0u32];
    for _ in 0..n {
        data.extend([16, 16, 0, 14, 17]);
        for _ in 0..256 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            data.push((seed >> 24) as u8);
        }
        offsets.push(data.len() as u32 - 4);
    }
    let mut index = Vec::new();
    index.extend(1u16.to_be_bytes());
    index.extend(1u16.to_be_bytes());
    index.extend(4u32.to_be_bytes());
    for o in &offsets {
        index.extend(o.to_be_bytes());
    }
    let mut eblc = Vec::new();
    eblc.extend([0, 2, 0, 0, 0, 0, 0, 1]);
    let array = 8 + 48u32;
    eblc.extend(array.to_be_bytes());
    eblc.extend((8 + index.len() as u32).to_be_bytes());
    eblc.extend(1u32.to_be_bytes());
    eblc.extend(0u32.to_be_bytes());
    eblc.extend([14, 0xFE, 16, 1, 0, 0, 0, 0, 14, 0xFE, 0, 0]);
    eblc.extend([14, 0xFE, 16, 1, 0, 0, 0, 0, 14, 0xFE, 0, 0]);
    eblc.extend(1u16.to_be_bytes());
    eblc.extend(n.to_be_bytes());
    eblc.extend([16, 16, 8, 1]);
    eblc.extend(1u16.to_be_bytes());
    eblc.extend(n.to_be_bytes());
    eblc.extend(8u32.to_be_bytes());
    eblc.extend(index);
    let glyphs = u32::from(n) + 1;
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let mut maxp = vec![0, 0, 0x50, 0];
    maxp.extend((glyphs as u16).to_be_bytes());
    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    let mut hmtx = vec![0x03, 0xE8, 0, 0];
    hmtx.extend(vec![0u8; 2 * (glyphs as usize - 1)]);
    let tables: [(&[u8; 4], Vec<u8>); 6] = [
        (b"EBDT", data),
        (b"EBLC", eblc),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"maxp", maxp),
    ];
    let mut out = Vec::new();
    out.extend(0x0001_0000u32.to_be_bytes());
    out.extend((tables.len() as u16).to_be_bytes());
    out.extend([0u8; 6]);
    let mut at = 12 + 16 * tables.len() as u32;
    let mut body = Vec::new();
    for (tag, t) in &tables {
        out.extend(*tag);
        out.extend(0u32.to_be_bytes());
        out.extend(at.to_be_bytes());
        out.extend((t.len() as u32).to_be_bytes());
        body.extend(t);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        at = 12 + 16 * tables.len() as u32 + body.len() as u32;
    }
    out.extend(body);
    out
}

/// The EBDT item of ADR 0023 (stage 5c): every glyph's bitmap decoded.
fn ebdt_items(b: &mut Bench) {
    let bytes = ebdt_font(200);
    let font = Font::new(&bytes, 0).expect("sofel opens the EBDT font");
    let ft = FtFace::open(&bytes).expect("FreeType opens the EBDT font");
    ft.set_ppem(16);
    let strike = sofel_bitmap::strikes(&font).next().expect("a strike");
    let n = font.num_glyphs();
    let mut buf = vec![0u8; 4096];
    b.item(
        "EBDT (built, 200 glyphs)",
        "bitmap pixels 16 ppem".into(),
        &mut || {
            (0..n)
                .map(
                    |g| match sofel_bitmap::draw_bitmap(&font, &strike, g, &mut buf) {
                        Ok(l) => {
                            u64::from(l.width)
                                + u64::from(l.rows)
                                + buf[..l.len()].iter().map(|&v| u64::from(v)).sum::<u64>()
                        }
                        // FreeType gives a missing glyph of a bitmap-only
                        // font an empty bitmap (whitespace).
                        Err(sofel_bitmap::BitmapError::Missing) => 0,
                        Err(_) => wrap(-1),
                    },
                )
                .fold(0u64, u64::wrapping_add)
        },
        Some((
            &mut || {
                (0..n)
                    .map(|g| wrap(ft.sbit(g)))
                    .fold(0u64, u64::wrapping_add)
            },
            Compare::Yes,
        )),
        None,
    );
}

/// A [`sofel_geom::Painter`] that counts the commands and image bytes,
/// as the shim's HarfBuzz paint functions do (without HarfBuzz's root
/// and inverse scales).
#[derive(Default)]
struct CountPainter(u64);

impl sofel_geom::Painter for CountPainter {
    fn push_transform(&mut self, _: sofel_geom::Transform) {
        self.0 += 1;
    }
    fn pop_transform(&mut self) {
        self.0 += 1;
    }
    fn push_clip_glyph(&mut self, _: u32) {
        self.0 += 1;
    }
    fn push_clip_rectangle(&mut self, _: sofel_geom::Rect) {
        self.0 += 1;
    }
    fn pop_clip(&mut self) {
        self.0 += 1;
    }
    fn fill(&mut self, _: &sofel_geom::Brush<'_>) {
        self.0 += 1;
    }
    fn fill_glyph(&mut self, _: u32, _: &sofel_geom::Solid) {
        self.0 += 1;
    }
    fn image(&mut self, image: &sofel_geom::Image<'_>) -> bool {
        self.0 += 1 + image.data.len() as u64;
        true
    }
    fn push_group(&mut self, _: sofel_geom::CompositeMode) {
        self.0 += 1;
    }
    fn pop_group(&mut self, _: sofel_geom::CompositeMode) {
        self.0 += 1;
    }
}

/// The color glyph items of ADR 0023, over every glyph of each font.
fn color_items(b: &mut Bench, testdata: &std::path::Path) {
    use sofel_color::{
        ColorFormat, PaintOptions, Painted, Recording, Scratch, glyph_color_format, layers, paint,
        paint_recording, svg_document,
    };
    for (name, path) in COLOR_FONTS {
        let Ok(bytes) = std::fs::read(testdata.join(path)) else {
            eprintln!("{path}: missing; run scripts/fetch-testdata.sh");
            std::process::exit(1);
        };
        let font = Font::new(&bytes, 0).expect("sofel opens the font");
        let ft = FtFace::open(&bytes).expect("FreeType opens the font");
        let hb = HbFont::open(&bytes).expect("HarfBuzz opens the font");
        hb.set_ppem(0);
        let n = font.num_glyphs();
        let formats = sofel_color::color_formats(&font);
        use sofel_color::ColorFormats as F;
        b.item(
            name,
            "glyph color format".into(),
            &mut || {
                (0..n)
                    .map(|g| {
                        let paint =
                            u64::from(glyph_color_format(&font, g) == Some(ColorFormat::ColrV1));
                        paint * 65536 + layers(&font, g).count() as u64
                    })
                    .sum()
            },
            None,
            Some((
                &mut || (0..n).map(|g| wrap(hb.color_format(g))).sum(),
                Compare::Yes,
            )),
        );
        if formats.has(F::COLR_V0) {
            b.item(
                name,
                "COLRv0 layers".into(),
                &mut || (0..n).map(|g| layers(&font, g).count() as u64).sum(),
                Some((
                    &mut || (0..n).map(|g| wrap(ft.layers(g))).sum(),
                    Compare::Yes,
                )),
                Some((
                    &mut || (0..n).map(|g| wrap(hb.color_layers(g))).sum(),
                    Compare::Yes,
                )),
            );
        }
        if formats.has(F::COLR_V0) {
            // ADR 0023 (5d): COLRv0 glyphs in color as FT_LOAD_COLOR.
            let outlines = Outlines::new(&font);
            let options = PaintOptions::default();
            for ppem in [12u16, 24, 48] {
                ft.set_ppem(u32::from(ppem));
                let mut scratch = sofel_outline::Scratch::default();
                let mut raster = Rasterizer::new();
                let mut image = sofel_raster::BgraImage::default();
                let mut ours = || {
                    (0..n)
                        .map(|g| {
                            use sofel_cli::glyph::{ColorGlyph, render_color};
                            match render_color(
                                &outlines,
                                GlyphId(g),
                                ppem,
                                &options,
                                &mut scratch,
                                &mut raster,
                                &mut image,
                            ) {
                                Ok(ColorGlyph::Gray(bm)) => {
                                    bm.pixels.iter().map(|&v| u64::from(v)).sum()
                                }
                                Ok(ColorGlyph::Color) => {
                                    image.pixels.iter().map(|&v| u64::from(v)).sum()
                                }
                                Err(_) => u64::MAX,
                            }
                        })
                        .fold(0u64, u64::wrapping_add)
                };
                b.item(
                    name,
                    format!("COLRv0 raster {ppem}px"),
                    &mut ours,
                    Some((
                        &mut || {
                            (0..n)
                                .map(|g| wrap(ft.colr_render(g)))
                                .fold(0u64, u64::wrapping_add)
                        },
                        Compare::Yes,
                    )),
                    None,
                );
            }
        }
        let mut scratch = Scratch::default();
        let mut painter = CountPainter::default();
        let options = PaintOptions::default();
        let mut paint_all = |ppem: u16| {
            let options = PaintOptions { ppem, ..options };
            (0..n)
                .map(|g| {
                    painter.0 = 0;
                    match paint(&font, g, &options, &mut scratch, &mut painter) {
                        Painted::None => wrap(-1),
                        _ => painter.0,
                    }
                })
                .fold(0u64, u64::wrapping_add)
        };
        let v1 = formats.has(F::COLR_V1);
        if formats.has(F::COLR_V0) || v1 {
            let item = if v1 { "COLRv1 paint" } else { "COLRv0 paint" };
            // HarfBuzz adds its root and inverse scales to COLRv1 paints:
            // the counts differ, so COLRv1 is only timed.
            let cmp = if v1 { Compare::No } else { Compare::Yes };
            let mut ft_walk = || (0..n).map(|g| wrap(ft.colr_walk(g))).sum();
            b.item(
                name,
                item.into(),
                &mut || paint_all(0),
                v1.then_some((&mut ft_walk as Body<'_>, Compare::No)),
                Some((
                    &mut || {
                        (0..n)
                            .map(|g| wrap(hb.paint(g)))
                            .fold(0u64, u64::wrapping_add)
                    },
                    cmp,
                )),
            );
        }
        if v1 {
            let mut scratch = Scratch::default();
            let mut rec = Recording::new();
            b.item(
                name,
                "COLRv1 recording".into(),
                &mut || {
                    (0..n)
                        .map(|g| {
                            rec.clear();
                            paint_recording(&font, g, &options, &mut scratch, &mut rec);
                            rec.commands().len() as u64
                        })
                        .sum()
                },
                None,
                None,
            );
        }
        if formats.has(F::CBDT) || formats.has(F::SBIX) {
            for ppem in [16u16, 109, 136, 0] {
                hb.set_bitmap_ppem(u32::from(ppem));
                b.item(
                    name,
                    format!("bitmap data {ppem} ppem"),
                    &mut || paint_all(ppem),
                    None,
                    Some((
                        &mut || {
                            (0..n)
                                .map(|g| wrap(hb.paint(g)))
                                .fold(0u64, u64::wrapping_add)
                        },
                        Compare::Yes,
                    )),
                );
            }
            hb.set_bitmap_ppem(0);
        }
        if formats.has(F::SVG) {
            b.item(
                name,
                "SVG documents".into(),
                &mut || {
                    (0..n)
                        .map(|g| svg_document(&font, g).map_or(0, |d| d.data.len() as u64))
                        .sum()
                },
                None,
                Some((
                    &mut || (0..n).map(|g| wrap(hb.svg_len(g))).sum(),
                    Compare::Yes,
                )),
            );
        }
        let outlines = Outlines::new(&font);
        let mut scratch = Scratch::default();
        b.item(
            name,
            "color glyph extents".into(),
            &mut || {
                (0..n)
                    .map(|g| box_sum(sofel_color::glyph_extents(&outlines, g, &mut scratch)))
                    .fold(0u64, u64::wrapping_add)
            },
            None,
            Some((
                &mut || {
                    (0..n)
                        .map(|g| hb.glyph_extents(g).map_or(wrap(i64::MIN), wrap))
                        .fold(0u64, u64::wrapping_add)
                },
                Compare::Yes,
            )),
        );
    }
}

fn main() {
    let a = args();
    let text: Vec<u32> = TEXTS
        .iter()
        .flat_map(|t| {
            std::fs::read_to_string(a.testdata.join(t))
                .unwrap_or_default()
                .chars()
                .collect::<Vec<_>>()
        })
        .map(u32::from)
        .collect();
    if text.is_empty() {
        eprintln!(
            "{}: no texts; run scripts/fetch-testdata.sh",
            a.testdata.display()
        );
        std::process::exit(1);
    }
    let mut b = Bench {
        once: a.once,
        filter: a.filter.clone(),
        rows: Vec::new(),
        checks: Vec::new(),
    };
    unicode_properties(&mut b, &a.testdata);
    extra_items(&mut b, &a.testdata);
    cff_items(&mut b, &a.testdata);
    color_items(&mut b, &a.testdata);
    ebdt_items(&mut b);
    for (name, path) in FONTS {
        let bytes = match std::fs::read(a.testdata.join(path)) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "{}: {e}; run scripts/fetch-testdata.sh",
                    a.testdata.join(path).display()
                );
                std::process::exit(1);
            }
        };
        let font = Font::new(&bytes, 0).expect("sofel opens the font");
        let ft = FtFace::open(&bytes).expect("FreeType opens the font");
        let hb = HbFont::open(&bytes).expect("HarfBuzz opens the font");
        let n = font.num_glyphs();
        assert_eq!(n, ft.num_glyphs(), "{name}: glyph counts");

        // Characters to glyphs: the sum of the glyph ids of every character.
        b.item(
            name,
            "cmap".into(),
            &mut || {
                text.iter()
                    .map(|&c| u64::from(font.nominal_glyph(c).map_or(0, |g| g.0)))
                    .sum()
            },
            Some((
                &mut || text.iter().map(|&c| u64::from(ft.char_index(c))).sum(),
                Compare::Yes,
            )),
            Some((
                &mut || text.iter().map(|&c| u64::from(hb.nominal_glyph(c))).sum(),
                Compare::Yes,
            )),
        );

        // Advances of every glyph, in font units.
        hb.set_ppem(0);
        b.item(
            name,
            "advance".into(),
            &mut || (0..n).map(|g| u64::from(font.advance(GlyphId(g)))).sum(),
            Some((
                &mut || (0..n).map(|g| wrap(ft.advance(g))).sum(),
                Compare::Yes,
            )),
            Some((
                &mut || (0..n).map(|g| wrap(hb.advance(g))).sum(),
                Compare::Yes,
            )),
        );

        // Vertical advances and origins of every glyph, in font units
        // (ADR 0016); HarfBuzz only, FreeType has no vertical origin.
        b.item(
            name,
            "v advance".into(),
            &mut || {
                (0..n)
                    .map(|g| wrap(i64::from(font.v_advance(GlyphId(g)))))
                    .sum()
            },
            None,
            Some((
                &mut || (0..n).map(|g| wrap(hb.v_advance(g))).sum(),
                Compare::Yes,
            )),
        );
        let v_boxes = HbExtents::new(&font);
        b.item(
            name,
            "v origin".into(),
            &mut || {
                (0..n)
                    .map(|g| {
                        let (x, y) = font.v_origin(GlyphId(g), |g| v_boxes.glyph_extents(g));
                        wrap(i64::from(x) + i64::from(y))
                    })
                    .sum()
            },
            None,
            Some((
                &mut || (0..n).map(|g| wrap(hb.v_origin(g))).sum(),
                Compare::Yes,
            )),
        );

        outline_items(&mut b, name, &font, &ft, &hb);
    }
    shape::shape_items(&mut b, &a.testdata);
    subset::subset_items(&mut b, &a.testdata);
    let failed = report_checks(&b.checks);
    if !a.once {
        print!("{}", table(&b.rows));
    }
    if let Err(e) = failed {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
