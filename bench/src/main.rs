//! Benchmarks of sofel against FreeType 2.15.0 and HarfBuzz 15.1.0
//! (ADR 0006, ADR 0013). Run from the repository root:
//!
//!   cargo run --release --manifest-path bench/Cargo.toml -- [--once] [--filter S] [--testdata DIR]

mod ffi;

use std::path::PathBuf;

use ffi::{FtFace, HbFont, HbUnicode};
use sofel_bench::harness::{Check, Config, Row, Stats, SystemClock, measure, report_checks, table};
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_geom::{Command, FnPen};
use sofel_outline::{Scale, Scratch, draw};
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

const TEXTS: [&str; 4] = [
    "harfbuzz/perf/texts/en-paragraph.txt",
    "harfbuzz/perf/texts/fa-paragraph.txt",
    "harfbuzz/perf/texts/hi-words.txt",
    "harfbuzz/perf/texts/ja-runs.txt",
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

        // Outlines: the number of commands (without closes) of every glyph.
        for ppem in [0u16, 48] {
            let scale = if ppem == 0 {
                Scale::Unscaled
            } else {
                Scale::pixels(&font, ppem, ppem)
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
                            match draw(&font, GlyphId(g), scale, &mut scratch, &mut pen) {
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
                            &font,
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
    let failed = report_checks(&b.checks);
    if !a.once {
        print!("{}", table(&b.rows));
    }
    if let Err(e) = failed {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
