//! The shaping items of ADR 0020: "shaping" as HarfBuzz's
//! perf/benchmark-shape.cc measures it (the text cut into lines, each
//! shaped with its guessed segment properties; sofel makes the plans once
//! and reuses them, HarfBuzz calls `hb_shape` with its plan cache and the
//! `ot` font functions), and "first shaping", from the font's bytes to
//! the first line shaped (ADR 0006's cold state). Before timing, every
//! line is shaped by both and compared: glyph ids, clusters, flags and
//! positions.
//!
//! HarfBuzz's buffer gets no language (`HB_LANGUAGE_INVALID`) instead of
//! the process locale's, as sofel does not read the locale (ADR 0020).

use std::path::Path;

use sofel_bench::harness::Check;
use sofel_font::Font;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::{Instance, Location, Variation};
use sofel_outline::HbExtents;
use sofel_shape::{
    Direction, GlyphBuffer, ResolvedProperties, Scale, SegmentProperties, ShapePlan, UnicodeBuffer,
};

use crate::ffi::{HbBuffer, HbFont, ShapedLine, hb_first_shape};
use crate::{Bench, Compare};

/// A pair of a font and a text, with the direction to shape in (`None`
/// for the guessed one).
struct Pair {
    font: &'static str,
    path: &'static str,
    text: &'static str,
    direction: Option<Direction>,
    literal: Option<&'static str>,
}

const fn pair(
    font: &'static str,
    path: &'static str,
    text: &'static str,
    direction: Option<Direction>,
) -> Pair {
    Pair {
        font,
        path,
        text,
        direction,
        literal: None,
    }
}

const ROBOTO: &str = "harfbuzz/perf/fonts/Roboto-Regular.ttf";
const SERIF: &str = "harfbuzz/test/subset/data/fonts/SourceSerifVariable-Roman.ttf";
const DEVANAGARI: &str = "harfbuzz/test/subset/data/fonts/NotoSansDevanagari-Regular.ttf";
const AMIRI: &str = "harfbuzz/perf/fonts/Amiri-Regular.ttf";
const NASTALIQ: &str = "harfbuzz/perf/fonts/NotoNastaliqUrdu-Regular.ttf";
const GULZAR: &str = "harfbuzz/perf/fonts/Gulzar-Regular.ttf";
const DUPLOYAN: &str = "harfbuzz/perf/fonts/NotoSansDuployan-Regular.otf";
const MPLUS: &str = "harfbuzz/test/api/fonts/Mplus1p-Regular.ttf";

/// Every pair of perf/benchmark-shape.cc, and M+ 1p with ja-runs.txt
/// both ways.
const PAIRS: [Pair; 12] = [
    pair("Roboto-Regular", ROBOTO, "en-thelittleprince.txt", None),
    pair("Roboto-Regular", ROBOTO, "en-words.txt", None),
    pair("SourceSerifVariable-Roman", SERIF, "react-dom.txt", None),
    pair(
        "NotoSansDevanagari-Regular",
        DEVANAGARI,
        "hi-words.txt",
        None,
    ),
    pair("Amiri-Regular", AMIRI, "fa-thelittleprince.txt", None),
    pair(
        "NotoNastaliqUrdu-Regular",
        NASTALIQ,
        "fa-thelittleprince.txt",
        None,
    ),
    pair("NotoNastaliqUrdu-Regular", NASTALIQ, "fa-words.txt", None),
    pair("Gulzar-Regular", GULZAR, "fa-thelittleprince.txt", None),
    pair("Gulzar-Regular", GULZAR, "fa-words.txt", None),
    pair("NotoSansDuployan-Regular", DUPLOYAN, "duployan.txt", None),
    pair("Mplus1p-Regular", MPLUS, "ja-runs.txt", None),
    pair(
        "Mplus1p-Regular",
        MPLUS,
        "ja-runs.txt",
        Some(Direction::Ttb),
    ),
];

// Fixed role specimens: guessed script, no language, default features and
// coordinates, face zero.
const EXTRA_PAIRS: [Pair; 9] = [
    Pair {
        font: "extra/arabic",
        path: "extra-fonts/fonts/arabic.ttf",
        text: "role-default-und",
        direction: Some(Direction::Rtl),
        literal: Some("بِت\n"),
    },
    Pair {
        font: "extra/devanagari",
        path: "extra-fonts/fonts/devanagari.ttf",
        text: "role-default-und",
        direction: Some(Direction::Ltr),
        literal: Some("क्‍ष\n"),
    },
    Pair {
        font: "extra/khmer",
        path: "extra-fonts/fonts/khmer.ttf",
        text: "role-default-und",
        direction: Some(Direction::Ltr),
        literal: Some("ក្រ\n"),
    },
    Pair {
        font: "extra/myanmar",
        path: "extra-fonts/fonts/myanmar.ttf",
        text: "role-default-und",
        direction: Some(Direction::Ltr),
        literal: Some("င်္ကိ\n"),
    },
    Pair {
        font: "extra/balinese",
        path: "extra-fonts/fonts/balinese.ttf",
        text: "role-default-und",
        direction: Some(Direction::Ltr),
        literal: Some("ᬓ᭄ᬱᬶ\n"),
    },
    Pair {
        font: "extra/hebrew",
        path: "extra-fonts/fonts/hebrew.ttf",
        text: "role-default-und",
        direction: Some(Direction::Rtl),
        literal: Some("שָׁלוֹם\n"),
    },
    Pair {
        font: "extra/source-serif",
        path: "extra-fonts/fonts/source-serif.otf",
        text: "role-default-und",
        direction: Some(Direction::Ltr),
        literal: Some("office\n"),
    },
    Pair {
        font: "extra/source-han",
        path: "extra-fonts/fonts/source-han.ttc",
        text: "role-default-und",
        direction: Some(Direction::Ttb),
        literal: Some("漢字\n"),
    },
    Pair {
        font: "extra/amstelvar",
        path: "extra-fonts/fonts/amstelvar.ttf",
        text: "role-default-und",
        direction: Some(Direction::Ltr),
        literal: Some("office\n"),
    },
];

/// `hb_direction_t` of a direction; 0 for the guessed one.
fn hb_direction(d: Option<Direction>) -> i32 {
    match d {
        None => 0,
        Some(Direction::Ltr) => 4,
        Some(Direction::Rtl) => 5,
        Some(Direction::Ttb) => 6,
        Some(Direction::Btt) => 7,
    }
}

/// The lines as perf/benchmark-shape.cc cuts them: the start and length
/// of every piece followed by `\n` (text after the last one is not
/// shaped).
fn lines(text: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(n) = text[start..].iter().position(|&c| c == b'\n') {
        out.push((start, n));
        start += n + 1;
    }
    out
}

/// The checksum of the shim's `shape_sum`.
fn checksum(g: &GlyphBuffer) -> u64 {
    let mut h = 0u64;
    let mut add = |v: u64| h = h.wrapping_mul(31).wrapping_add(v);
    for (i, p) in g.glyph_infos().iter().zip(g.glyph_positions()) {
        add(u64::from(i.glyph));
        add(u64::from(i.cluster));
        add(u64::from(i.flags.0));
        for v in [p.x_advance, p.y_advance, p.x_offset, p.y_offset] {
            add(i64::from(v) as u64);
        }
    }
    h
}

fn shaped(g: &GlyphBuffer) -> ShapedLine {
    ShapedLine {
        glyphs: g.glyph_infos().iter().map(|i| i.glyph).collect(),
        clusters: g.glyph_infos().iter().map(|i| i.cluster).collect(),
        flags: g.glyph_infos().iter().map(|i| i.flags.0).collect(),
        positions: g
            .glyph_positions()
            .iter()
            .flat_map(|p| [p.x_advance, p.y_advance, p.x_offset, p.y_offset])
            .collect(),
    }
}

/// The properties of a line: guessed, with the pair's direction.
fn properties(b: &UnicodeBuffer, direction: Option<Direction>) -> ResolvedProperties {
    let mut p = SegmentProperties::default().guess(b, None);
    if let Some(d) = direction {
        p.direction = d;
    }
    p
}

/// sofel's side of a pair: the font, its glyph boxes as HarfBuzz's `ot`
/// functions take them, and the plans, one per set of properties.
struct Shaper<'a, 'loc, 'source> {
    font: Font<'a>,
    location: &'loc Location<'source>,
    boxes: HbExtents<'a, 'loc, 'source>,
    scale: Scale,
    plans: Vec<(ResolvedProperties, ShapePlan<'a>)>,
}

impl<'a> Shaper<'a, 'static, 'static> {
    fn new(bytes: &'a [u8]) -> Self {
        Self::with_location(bytes, Location::default_ref())
    }
}

impl<'a, 'loc, 'source> Shaper<'a, 'loc, 'source> {
    fn with_location(bytes: &'a [u8], location: &'loc Location<'source>) -> Self {
        let font = Font::new(bytes, 0).expect("sofel opens the font");
        Shaper {
            font,
            location,
            boxes: HbExtents::new(font.instance(location)),
            scale: Scale::unscaled(&font),
            plans: Vec::new(),
        }
    }

    fn instance(&self) -> Instance<'_, 'a, 'loc, 'source> {
        self.font.instance(self.location)
    }

    /// Makes the plan for `props` unless there is one.
    fn plan_for(&mut self, props: &ResolvedProperties) {
        if !self.plans.iter().any(|(k, _)| k == props) {
            let plan = ShapePlan::new(self.instance(), props.clone(), &[]);
            self.plans.push((props.clone(), plan));
        }
    }

    /// Shapes one line with a plan made before.
    fn shape(&self, b: UnicodeBuffer, direction: Option<Direction>) -> GlyphBuffer {
        let props = properties(&b, direction);
        let plan = &self
            .plans
            .iter()
            .find(|(k, _)| *k == props)
            .expect("a plan for every line")
            .1;
        let boxes = |g: GlyphId| self.boxes.glyph_extents(g);
        sofel_shape::shape_with_extents(self.instance(), &self.scale, plan, b, &boxes)
    }
}

/// How many lines differ and the first one, as text for the check;
/// makes the plans of all lines.
fn first_difference(
    s: &mut Shaper<'_, '_, '_>,
    hb: &HbFont,
    text: &[u8],
    lines: &[(usize, usize)],
    direction: Option<Direction>,
) -> Option<String> {
    let hb_buffer = HbBuffer::new();
    let mut b = UnicodeBuffer::new();
    let mut first = None;
    let mut count = 0;
    for (n, &(start, len)) in lines.iter().enumerate() {
        b.push_utf8(&text[start..], 0, Some(len));
        s.plan_for(&properties(&b, direction));
        let g = s.shape(b, direction);
        let ours = shaped(&g);
        b = g.into_unicode();
        let theirs = hb_buffer.shape_line(hb, &text[start..], len, hb_direction(direction));
        if ours != theirs {
            count += 1;
            first.get_or_insert_with(|| {
                format!("line {}: sofel {ours:?}, HarfBuzz {theirs:?}", n + 1)
            });
        }
    }
    first.map(|f| format!("{count} of {} lines differ; {f}", lines.len()))
}

/// Adds the shaping items of every pair.
pub fn shape_items(b: &mut Bench, testdata: &Path) {
    for p in PAIRS.iter().chain(EXTRA_PAIRS.iter()) {
        let dir = match p.direction {
            Some(Direction::Ttb) => " TTB",
            _ => "",
        };
        let item = format!("shaping {}{dir}", p.text);
        let first = format!("first shaping {}{dir}", p.text);
        if !b.wanted(p.font, &item) && !b.wanted(p.font, &first) {
            continue;
        }
        let text_path = testdata.join("harfbuzz/perf/texts").join(p.text);
        let (Ok(bytes), Ok(text)) = (
            std::fs::read(testdata.join(p.path)),
            p.literal.map_or_else(
                || std::fs::read(&text_path),
                |text| Ok(text.as_bytes().to_vec()),
            ),
        ) else {
            eprintln!("{}: missing; run scripts/fetch-testdata.sh", p.path);
            std::process::exit(1);
        };
        let lines = lines(&text);
        let hb = HbFont::open(&bytes).expect("HarfBuzz opens the font");
        hb.set_ppem(0);
        let mut s = Shaper::new(&bytes);
        let diff = first_difference(&mut s, &hb, &text, &lines, p.direction);
        b.checks.push(Check {
            item: format!("{item} (HarfBuzz, every line)"),
            font: p.font.to_string(),
            ok: diff.is_none(),
            detail: diff.unwrap_or_else(|| format!("{} lines equal", lines.len())),
        });

        let hb_buffer = HbBuffer::new();
        let mut spare = Some(UnicodeBuffer::new());
        b.item(
            p.font,
            item,
            &mut || {
                let mut sum = 0u64;
                for &(start, len) in &lines {
                    let mut u = spare.take().unwrap_or_default();
                    u.push_utf8(&text[start..], 0, Some(len));
                    let g = s.shape(u, p.direction);
                    sum = sum.wrapping_add(checksum(&g));
                    spare = Some(g.into_unicode());
                }
                sum
            },
            None,
            Some((
                &mut || hb_buffer.shape_text(&hb, &text, hb_direction(p.direction)),
                Compare::Yes,
            )),
        );

        let Some(&(start, len)) = lines.first() else {
            continue;
        };
        b.item(
            p.font,
            first,
            &mut || {
                let font = Font::new(&bytes, 0).expect("sofel opens the font");
                let boxes = HbExtents::new(font.default_instance());
                let mut u = UnicodeBuffer::new();
                u.push_utf8(&text[start..], 0, Some(len));
                let plan =
                    ShapePlan::new(font.default_instance(), properties(&u, p.direction), &[]);
                let g = sofel_shape::shape_with_extents(
                    font.default_instance(),
                    &Scale::unscaled(&font),
                    &plan,
                    u,
                    &|g| boxes.glyph_extents(g),
                );
                checksum(&g)
            },
            None,
            Some((
                &mut || hb_first_shape(&bytes, &text[start..], len, hb_direction(p.direction)),
                Compare::Yes,
            )),
        );
    }
}

/// ADR 0049: one shape call per line, with a fresh 128-slot cache each time.
/// The same item can be measured on the pre-cache commit for the uncached time.
pub fn adr0049_items(b: &mut Bench, testdata: &Path) {
    for (name, file) in [
        ("RobotoFlex-Variable", "RobotoFlex-Variable.ttf"),
        ("SourceSerifVariable-Roman", "SourceSerifVariable-Roman.ttf"),
        ("AdobeVFPrototype", "AdobeVFPrototype.otf"),
    ] {
        for text_name in ["react-dom.txt", "en-thelittleprince.txt"] {
            for varied in [false, true] {
                let position = if varied { "wght=500" } else { "default" };
                let item = format!("ADR0049 shaping {text_name} {position}");
                if !b.wanted(name, &item) {
                    continue;
                }
                let bytes =
                    std::fs::read(testdata.join("harfbuzz/test/subset/data/fonts").join(file))
                        .unwrap();
                let text =
                    std::fs::read(testdata.join("harfbuzz/perf/texts").join(text_name)).unwrap();
                let font = Font::new(&bytes, 0).unwrap();
                let location = Location::new(
                    &font,
                    None,
                    &[Variation {
                        tag: Tag::new(b"wght"),
                        value: 500.0,
                    }],
                );
                let instance = font.instance(if varied {
                    &location
                } else {
                    Location::default_ref()
                });
                let scale = Scale::unscaled(&font);
                let boxes = HbExtents::new(instance);
                let hb = HbFont::open(&bytes).unwrap();
                hb.set_ppem(0);
                if varied {
                    hb.set_weight(500.0);
                }
                let lines = lines(&text);
                let mut plans = Vec::new();
                for &(start, len) in &lines {
                    let mut u = UnicodeBuffer::new();
                    u.push_utf8(&text[start..], 0, Some(len));
                    let props = properties(&u, None);
                    if !plans
                        .iter()
                        .any(|(known, _): &(ResolvedProperties, ShapePlan<'_>)| *known == props)
                    {
                        plans.push((props.clone(), ShapePlan::new(instance, props, &[])));
                    }
                }
                let hb_buffer = HbBuffer::new();
                let mut spare = Some(UnicodeBuffer::new());
                b.item(
                    name,
                    item,
                    &mut || {
                        let mut sum = 0u64;
                        for &(start, len) in &lines {
                            let mut u = spare.take().unwrap_or_default();
                            u.push_utf8(&text[start..], 0, Some(len));
                            let props = properties(&u, None);
                            let plan = &plans.iter().find(|(known, _)| *known == props).unwrap().1;
                            let extents = |g: GlyphId| boxes.glyph_extents(g);
                            let g = sofel_shape::shape_with_extents(
                                instance, &scale, plan, u, &extents,
                            );
                            sum = sum.wrapping_add(checksum(&g));
                            spare = Some(g.into_unicode());
                        }
                        sum
                    },
                    None,
                    Some((&mut || hb_buffer.shape_text(&hb, &text, 0), Compare::Yes)),
                );
            }
        }
    }
}

/// ADR 0027's non-default shaping item, with the same per-line protocol
/// and complete comparison as the default `react-dom.txt` item.
pub fn variable_shape_items(b: &mut Bench, testdata: &Path) {
    let item = "shaping react-dom.txt wght=500";
    let name = "SourceSerifVariable-Roman";
    if !b.wanted(name, item) {
        return;
    }
    let bytes = std::fs::read(testdata.join(SERIF)).expect("SourceSerifVariable-Roman test data");
    let text = std::fs::read(testdata.join("harfbuzz/perf/texts/react-dom.txt"))
        .expect("react-dom.txt test data");
    let lines = lines(&text);
    let hb = HbFont::open(&bytes).expect("HarfBuzz opens the font");
    hb.set_ppem(0);
    hb.set_weight(500.0);
    let font = Font::new(&bytes, 0).expect("sofel opens the font");
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: Tag::new(b"wght"),
            value: 500.0,
        }],
    );
    let mut shaper = Shaper::with_location(&bytes, &location);
    let diff = first_difference(&mut shaper, &hb, &text, &lines, None);
    b.checks.push(Check {
        item: format!("{item} (HarfBuzz, every line)"),
        font: name.into(),
        ok: diff.is_none(),
        detail: diff.unwrap_or_else(|| format!("{} lines equal", lines.len())),
    });

    let hb_buffer = HbBuffer::new();
    let mut spare = Some(UnicodeBuffer::new());
    b.item(
        name,
        item.into(),
        &mut || {
            let mut sum = 0u64;
            for &(start, len) in &lines {
                let mut u = spare.take().unwrap_or_default();
                u.push_utf8(&text[start..], 0, Some(len));
                let g = shaper.shape(u, None);
                sum = sum.wrapping_add(checksum(&g));
                spare = Some(g.into_unicode());
            }
            sum
        },
        None,
        Some((&mut || hb_buffer.shape_text(&hb, &text, 0), Compare::Yes)),
    );
}

#[cfg(test)]
mod variable_shape_tests {
    use super::*;

    #[test]
    fn adr0027_weight_shape_matches_harfbuzz_all_lines() {
        let _ffi = crate::variation_bench_tests::lock_ffi();
        let mut bench = Bench {
            once: true,
            filter: Some("shaping react-dom.txt wght=500".into()),
            rows: Vec::new(),
            checks: Vec::new(),
        };
        variable_shape_items(
            &mut bench,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/testdata"),
        );
        assert_eq!(bench.rows.len(), 1);
        assert!(
            bench.checks.iter().all(|check| check.ok),
            "{:?}",
            bench
                .checks
                .iter()
                .map(|check| &check.detail)
                .collect::<Vec<_>>()
        );
    }
}
