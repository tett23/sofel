//! sofel-shape against HarfBuzz through `hb-dump` (ADR 0020,
//! HarfBuzzとの突き合わせ), run with `LC_ALL=C` so that HarfBuzz's
//! language is none, as sofel's.
//!
//! - `--shape FONT TEXTFILE DIRECTION FEATURES`: every line of every
//!   pair of HarfBuzz's `perf/benchmark-shape.cc` and of Mplus1p-Regular
//!   with `ja-runs.txt` (left to right and top to bottom, with no
//!   features, `vrt2`, `-vert`, `palt` and `vkrn`), shaped by HarfBuzz
//!   with `hb_buffer_add_utf8`, `hb_buffer_guess_segment_properties` and
//!   `hb_shape`, and by `sofel_cli::shape` with `--text-file
//!   --utf8-clusters --no-glyph-names --show-flags`. Items
//!   `<font>+<text>:<direction>:<features>`, failing when one of the
//!   lines differs (the category must fit every line that does), judged
//!   against
//!   conformance/expected-failures-shape-pairs.txt. Variable fonts are
//!   shaped at their default instance.
//! - `--shape-random FONT COUNT SEED`: for every HarfBuzz test font, the
//!   characters its cmap maps, grouped by script; for each script COUNT
//!   strings of 1 to 16 of them from the same linear congruential
//!   generator on both sides (the strings are compared too), shaped with
//!   `BOT | EOT | PRODUCE_UNSAFE_TO_CONCAT`. Items
//!   `<font>#<face>:<script>`, failing when one of the strings differs,
//!   judged against conformance/expected-failures-shape-random.txt.
//! - `--layout-map FONT`: for every test font, every script and the
//!   languages of the font's language systems, the script and language
//!   system `hb_ot_map_builder_t` picks in `GSUB` and `GPOS`, against
//!   [`sofel_shape::layout_selection`]. Items `<font>#<face>`, judged
//!   against conformance/expected-failures-layout-map.txt.
//!
//! The categories of the lists are those of
//! conformance/expected-failures.txt, checked the same way.
//! `SOFEL_SHAPE_VERBOSE=1` prints the differences; `SOFEL_SHAPE_SUGGEST=1`
//! suggests categories for unlisted failures.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use sofel_conformance::shape::{self, Subject};
use sofel_conformance::{fonts, paths};
use sofel_font::Font;
use sofel_font::read::{GlyphId, Tag};
use sofel_outline::HbExtents;
use sofel_shape::{
    BufferFlags, Direction, EmptyFont, Language, Scale, Script, SegmentProperties, SerializeFlags,
    SerializeFormat, ShapePlan, UnicodeBuffer, layout_selection, ot_tags_from_script_and_language,
    serialize_glyphs, shape_with_extents,
};

const PAIRS_LIST: &str = "expected-failures-shape-pairs.txt";
const RANDOM_LIST: &str = "expected-failures-shape-random.txt";
const LAYOUT_LIST: &str = "expected-failures-layout-map.txt";
const RANDOM_COUNT: u32 = 200;
const RANDOM_SEED: u32 = 20;

/// The pairs: font, text, directions, features (relative to HarfBuzz).
const PAIRS: &[(&str, &str, &[&str], &[&str])] = &[
    (
        "perf/fonts/Roboto-Regular.ttf",
        "perf/texts/en-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Roboto-Regular.ttf",
        "perf/texts/en-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "test/subset/data/fonts/SourceSerifVariable-Roman.ttf",
        "perf/texts/react-dom.txt",
        &["auto"],
        &[""],
    ),
    (
        "test/subset/data/fonts/NotoSansDevanagari-Regular.ttf",
        "perf/texts/hi-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Amiri-Regular.ttf",
        "perf/texts/fa-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/NotoNastaliqUrdu-Regular.ttf",
        "perf/texts/fa-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/NotoNastaliqUrdu-Regular.ttf",
        "perf/texts/fa-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Gulzar-Regular.ttf",
        "perf/texts/fa-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Gulzar-Regular.ttf",
        "perf/texts/fa-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/NotoSansDuployan-Regular.otf",
        "perf/texts/duployan.txt",
        &["auto"],
        &[""],
    ),
    (
        "test/subset/data/fonts/Mplus1p-Regular.ttf",
        "perf/texts/ja-runs.txt",
        &["ltr", "ttb"],
        &["", "vrt2", "-vert", "palt", "vkrn"],
    ),
];

fn hb_dump(args: &[&OsString]) -> String {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(&exe)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(out.status.success(), "hb-dump {args:?} failed");
    String::from_utf8(out.stdout).unwrap()
}

fn name(p: &str) -> &str {
    p.rsplit('/').next().unwrap()
}

/// Runs `jobs` on all cores, keeping their order.
fn parallel<T: Send, R: Send>(jobs: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let job = queue.lock().unwrap().pop();
                    let Some((i, job)) = job else { break };
                    let r = f(job);
                    results.lock().unwrap().push((i, r));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(i, _)| *i);
    results.into_iter().map(|(_, r)| r).collect()
}

/// One compared line: its item, whether it passed, and what the
/// category check needs.
struct Line {
    id: String,
    /// The line of the text, for `--shape`; 0 otherwise.
    line: usize,
    passed: bool,
    font: PathBuf,
    face: u32,
    text: Vec<u32>,
    direction: Option<Direction>,
    got: String,
    want: String,
}

fn failure_fonts(
    lines: &[Line],
    mut read: impl FnMut(&Path) -> std::io::Result<Vec<u8>>,
) -> BTreeMap<&Path, Vec<u8>> {
    let paths: BTreeSet<_> = lines
        .iter()
        .filter(|l| !l.passed)
        .map(|l| l.font.as_path())
        .collect();
    paths
        .into_iter()
        .filter_map(|path| read(path).ok().map(|bytes| (path, bytes)))
        .collect()
}

fn judge(lines: &[Line], count: usize, list: &str, what: &str, elapsed: std::time::Duration) {
    let verbose = std::env::var_os("SOFEL_SHAPE_VERBOSE").is_some();
    let suggest = std::env::var_os("SOFEL_SHAPE_SUGGEST").is_some();
    let fonts = failure_fonts(lines, |p| std::fs::read(p));
    let options = Vec::new();
    let subject = |i: usize| {
        let l = lines.get(i)?;
        Some(Subject {
            data: fonts.get(l.font.as_path())?,
            face: l.face,
            options: &options,
            shaper: "ot",
            text: &l.text,
            props: SegmentProperties {
                direction: l.direction,
                ..SegmentProperties::default()
            },
        })
    };
    let results: Vec<(String, bool)> = lines.iter().map(|l| (l.id.clone(), l.passed)).collect();
    let failed = lines.iter().filter(|l| !l.passed).count();
    let passes = count - failed;
    println!("{what}: {count} cases, {passes} passed, {failed} failed, in {elapsed:.1?}");
    if verbose {
        for l in lines.iter().filter(|l| !l.passed) {
            let text: Vec<String> = l.text.iter().map(|u| format!("{u:04X}")).collect();
            println!(
                "FAIL {}:{} [{}]\n  hb:    {}\n  sofel: {}",
                l.id,
                l.line,
                text.join(","),
                l.want,
                l.got
            );
        }
    }
    let (report, by_category) = shape::judge(&results, &subject, list, suggest);
    for (cat, n) in &by_category {
        println!("  expected failures {cat}: {n}");
    }
    assert!(report.is_empty(), "{report}");
}

/// Preserve each failure for its category and verbose report. All successful
/// lines of the same pair contribute just one identifier to the group verdict.
fn pair_cases(
    id: &str,
    font: &Path,
    direction: Option<Direction>,
    input: &str,
    got: &str,
    want: &str,
) -> Result<(usize, Vec<Line>), String> {
    let mut texts = input.split_terminator('\n');
    let mut got = got.lines();
    let mut want = want.lines();
    let mut count = 0;
    let mut lines = Vec::new();
    loop {
        match (texts.next(), got.next(), want.next()) {
            (None, None, None) => break,
            (Some(text), Some(g), Some(w)) => {
                count += 1;
                if g != w {
                    lines.push(Line {
                        id: id.to_owned(),
                        line: count,
                        passed: false,
                        font: font.to_owned(),
                        face: 0,
                        text: text.chars().map(u32::from).collect(),
                        direction,
                        got: g.to_owned(),
                        want: w.to_owned(),
                    });
                }
            }
            _ => {
                return Err(format!(
                    "{id}: input, sofel and HarfBuzz line counts differ after {count} lines"
                ));
            }
        }
    }
    if count > 0 && lines.is_empty() {
        lines.push(Line {
            id: id.to_owned(),
            line: 0,
            passed: true,
            font: font.to_owned(),
            face: 0,
            text: Vec::new(),
            direction,
            got: String::new(),
            want: String::new(),
        });
    }
    Ok((count, lines))
}

#[test]
fn shape_pairs_agree_with_harfbuzz() {
    let hb = paths::testdata().join("harfbuzz");
    let started = Instant::now();
    let mut jobs = Vec::new();
    for &(font, text, dirs, feats) in PAIRS {
        for &dir in dirs {
            for &feat in feats {
                jobs.push((font, text, dir, feat));
            }
        }
    }
    let pairs = parallel(jobs, |(font, text, dir, feat)| {
        let font_path = hb.join(font);
        let text_path = hb.join(text);
        let want = hb_dump(&[
            &"--shape".into(),
            &font_path.clone().into(),
            &text_path.clone().into(),
            &dir.into(),
            &feat.into(),
        ]);
        let mut args: Vec<OsString> = vec![
            font_path.clone().into(),
            "--text-file".into(),
            text_path.clone().into(),
            "--utf8-clusters".into(),
            "--no-glyph-names".into(),
            "--show-flags".into(),
        ];
        if dir != "auto" {
            args.push(format!("--direction={dir}").into());
        }
        if !feat.is_empty() {
            args.push(format!("--features={feat}").into());
        }
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = sofel_cli::shape::run_with_input(&args, &mut &b""[..], &mut out, &mut err);
        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&err));
        let got = String::from_utf8(out).unwrap();
        let texts = std::fs::read_to_string(&text_path).unwrap();
        let direction = Direction::from_bytes(dir.as_bytes()).filter(|_| dir != "auto");
        let feat_id = if feat.is_empty() { "-" } else { feat };
        let id = format!("{}+{}:{dir}:{feat_id}", name(font), name(text));
        pair_cases(&id, &font_path, direction, &texts, &got, &want).unwrap()
    });
    let count = pairs.iter().map(|(count, _)| count).sum();
    let lines: Vec<_> = pairs.into_iter().flat_map(|(_, lines)| lines).collect();
    judge(&lines, count, PAIRS_LIST, "--shape", started.elapsed());
}

/// The generator of `hb-dump --shape-random`: the C standard's example
/// LCG, the high 15 bits of the state.
fn random_next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
    (*state >> 16) & 0x7FFF
}

/// `(script tag, strings)` for a face, as `hb-dump --shape-random`
/// generates them from sofel's cmap.
fn random_strings(font: &Font<'_>) -> Vec<(u32, Vec<Vec<u32>>)> {
    let mut groups: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for u in 0..=0x10_FFFFu32 {
        if font.nominal_glyph_raw(u).is_some() {
            let tag = u32::from_be_bytes(sofel_unicode::script(u).tag());
            groups.entry(tag).or_default().push(u);
        }
    }
    groups
        .into_iter()
        .map(|(tag, cps)| {
            let mut state = RANDOM_SEED;
            let strings = (0..RANDOM_COUNT)
                .map(|_| {
                    let len = 1 + random_next(&mut state) % 16;
                    (0..len)
                        .map(|_| cps[(random_next(&mut state) as usize) % cps.len()])
                        .collect()
                })
                .collect();
            (tag, strings)
        })
        .collect()
}

/// The faces of `hb-dump --shape-random`: `(face, [(script, [(text,
/// glyphs)])])`.
type RandomDump = Vec<(u32, Vec<(u32, Vec<(Vec<u32>, String)>)>)>;

fn parse_random(text: &str) -> RandomDump {
    let mut faces: RandomDump = Vec::new();
    for line in text.lines() {
        let mut w = line.splitn(3, ' ');
        match w.next().unwrap() {
            "face" => faces.push((w.next().unwrap().parse().unwrap(), Vec::new())),
            "script" => {
                let tag = u32::from_str_radix(w.next().unwrap(), 16).unwrap();
                faces.last_mut().unwrap().1.push((tag, Vec::new()));
            }
            "s" => {
                let cps = w
                    .next()
                    .unwrap()
                    .split(',')
                    .map(|c| u32::from_str_radix(c, 16).unwrap())
                    .collect();
                let glyphs = w.next().unwrap_or("").to_string();
                let face = faces.last_mut().unwrap();
                face.1.last_mut().unwrap().1.push((cps, glyphs));
            }
            "end" => {}
            other => panic!("unknown line {other}"),
        }
    }
    faces
}

/// sofel's result for a string, as `hb-dump` writes it.
fn sofel_random<'a>(
    font: &Font<'a>,
    extents: &HbExtents<'_>,
    plans: &mut Vec<ShapePlan<'a>>,
    text: &[u32],
) -> String {
    let mut buffer = UnicodeBuffer::new();
    buffer.set_flags(
        BufferFlags::BOT
            .union(BufferFlags::EOT)
            .union(BufferFlags::PRODUCE_UNSAFE_TO_CONCAT),
    );
    buffer.push_codepoints(text, 0, None);
    let props = SegmentProperties::default().guess(&buffer, None);
    let plan = match plans.iter().position(|p| p.key().0 == props) {
        Some(i) => &plans[i],
        None => {
            plans.push(ShapePlan::new(font.default_instance(), props, &[]));
            plans.last().unwrap()
        }
    };
    let glyphs = shape_with_extents(
        font.default_instance(),
        &Scale::unscaled(font),
        plan,
        buffer,
        &|g: GlyphId| extents.glyph_extents(g),
    );
    let mut out = Vec::new();
    serialize_glyphs(
        glyphs.glyph_infos(),
        Some(glyphs.glyph_positions()),
        &EmptyFont,
        SerializeFormat::Text,
        SerializeFlags::NO_GLYPH_NAMES.union(SerializeFlags::GLYPH_FLAGS),
        &mut out,
    );
    String::from_utf8(out).unwrap()
}

#[test]
fn shape_random_agrees_with_harfbuzz() {
    let testdata = paths::testdata();
    let started = Instant::now();
    let fonts = fonts::harfbuzz_fonts(&testdata);
    let lines: Vec<Line> = parallel(fonts, |path| {
        let id = fonts::id(&path, &testdata);
        let dump = hb_dump(&[
            &"--shape-random".into(),
            &path.clone().into(),
            &RANDOM_COUNT.to_string().into(),
            &RANDOM_SEED.to_string().into(),
        ]);
        let data = std::fs::read(&path).unwrap();
        let mut lines = Vec::new();
        for (face, scripts) in parse_random(&dump) {
            // Faces sofel does not open (no `head`, ADR 0010) are not
            // compared.
            let Ok(font) = Font::new(&data, face) else {
                continue;
            };
            let ours = random_strings(&font);
            let hb_tags: Vec<u32> = scripts.iter().map(|s| s.0).collect();
            let our_tags: Vec<u32> = ours.iter().map(|s| s.0).collect();
            assert_eq!(
                hb_tags, our_tags,
                "{id}#{face}: the scripts of the cmap differ"
            );
            let extents = HbExtents::new(&font);
            let mut plans = Vec::new();
            for ((tag, strings), (_, our_strings)) in scripts.iter().zip(&ours) {
                let script = String::from_utf8_lossy(&tag.to_be_bytes()).into_owned();
                let mut first_failure = None;
                for ((text, want), our_text) in strings.iter().zip(our_strings) {
                    assert_eq!(text, our_text, "{id}#{face}:{script}: the strings differ");
                    let got = sofel_random(&font, &extents, &mut plans, text);
                    if &got != want && first_failure.is_none() {
                        first_failure = Some((text.clone(), got, want.clone()));
                    }
                }
                let (text, got, want) = first_failure.clone().unwrap_or_default();
                lines.push(Line {
                    id: format!("{id}#{face}:{script}"),
                    line: 0,
                    passed: first_failure.is_none(),
                    font: path.clone(),
                    face,
                    text,
                    direction: None,
                    got,
                    want,
                });
            }
        }
        lines
    })
    .into_iter()
    .flatten()
    .collect();
    judge(
        &lines,
        lines.len(),
        RANDOM_LIST,
        "--shape-random",
        started.elapsed(),
    );
}

#[test]
fn layout_map_agrees_with_harfbuzz() {
    let testdata = paths::testdata();
    let started = Instant::now();
    let fonts = fonts::harfbuzz_fonts(&testdata);
    let lines: Vec<Line> = parallel(fonts, |path| {
        let id = fonts::id(&path, &testdata);
        let dump = hb_dump(&[&"--layout-map".into(), &path.clone().into()]);
        let data = std::fs::read(&path).unwrap();
        let mut lines: Vec<Line> = Vec::new();
        let mut font = None;
        for line in dump.lines() {
            let w: Vec<&str> = line.split(' ').collect();
            match w[0] {
                "face" => {
                    let face = w[1].parse().unwrap();
                    font = Font::new(&data, face).ok();
                    // Faces sofel does not open (no `head`, ADR 0010) have
                    // no layout to compare.
                    if font.is_none() {
                        continue;
                    }
                    lines.push(Line {
                        id: format!("{id}#{face}"),
                        line: 0,
                        passed: true,
                        font: path.clone(),
                        face,
                        text: Vec::new(),
                        direction: None,
                        got: String::new(),
                        want: String::new(),
                    });
                }
                "end" => {}
                "sel" => {
                    let Some(font) = &font else { continue };
                    let item = lines.last_mut().unwrap();
                    let tag = u32::from_str_radix(w[1], 16).unwrap();
                    let script = (tag != 0)
                        .then(|| Script::from_iso15924(&tag.to_be_bytes()))
                        .flatten();
                    let language = (w[2] != "*")
                        .then(|| Language::new(w[2].as_bytes()))
                        .flatten();
                    let (scripts, languages) =
                        ot_tags_from_script_and_language(script, language.as_ref());
                    let s = layout_selection(font, &scripts, &languages);
                    let ours: Vec<String> = (0..2)
                        .flat_map(|t| {
                            [
                                u8::from(s.found_script[t]).to_string(),
                                s.script_index[t].map_or(0xFFFF, |i| i as u32).to_string(),
                                format!("{:08x}", s.chosen_script[t].map_or(0, Tag::to_u32)),
                                s.language_index[t].map_or(0xFFFF, |i| i as u32).to_string(),
                            ]
                        })
                        .collect();
                    if item.passed && ours[..] != w[3..] {
                        item.passed = false;
                        item.got = format!("{} {} {}", w[1], w[2], ours.join(" "));
                        item.want = line.to_string();
                    }
                }
                other => panic!("{id}: unknown line {other}"),
            }
        }
        lines
    })
    .into_iter()
    .flatten()
    .collect();
    judge(
        &lines,
        lines.len(),
        LAYOUT_LIST,
        "--layout-map",
        started.elapsed(),
    );
}

#[test]
fn adr0041_i1_i2_compacts_passes_and_keeps_each_failure() {
    let font = Path::new("example.ttf");
    let compress = |input, got, want| pair_cases("pair", font, None, input, got, want).unwrap();
    let empty = compress("", "", "");
    assert_eq!(empty.0, 0);
    assert!(empty.1.is_empty());
    let (count, all_ok) = compress("\na\n", "[]\n[a]\n", "[]\n[a]\n");
    assert_eq!(count, 2);
    assert_eq!(all_ok.len(), 1);
    assert_eq!((all_ok[0].id.as_str(), all_ok[0].passed), ("pair", true));
    assert!(all_ok[0].got.is_empty() && all_ok[0].want.is_empty() && all_ok[0].text.is_empty());
    let (count, failures) = compress("一\n𠮷\n二\n三", "a\nb\nc\nd\n", "a\nX\nc\nY\n");
    assert_eq!(count, 4);
    let details: Vec<_> = failures
        .iter()
        .map(|l| {
            (
                l.line,
                l.got.as_str(),
                l.want.as_str(),
                l.text.clone(),
                l.passed,
            )
        })
        .collect();
    assert_eq!(
        details,
        [
            (2, "b", "X", vec![0x20bb7], false),
            (4, "d", "Y", vec![0x4e09], false)
        ]
    );
    let (count, failures) = compress("x\ny", "A\nB", "C\nD");
    assert_eq!((count, failures.len()), (2, 2));
}

#[test]
fn adr0041_i3_requires_one_result_for_each_source_line() {
    let check = |input, got, want| pair_cases("pair", Path::new("font"), None, input, got, want);
    assert!(check("\n", "[]\n", "[]\n").is_ok());
    assert!(check("x", "x", "x\n").is_ok());
    for (input, got, want) in [
        ("x\ny", "x", "x\ny"),
        ("x\ny", "x\ny", "x"),
        ("x", "x\ny", "x"),
        ("x", "x", "x\ny"),
        ("", "x", "x"),
        ("x", "", ""),
    ] {
        assert!(
            check(input, got, want).is_err(),
            "{input:?} {got:?} {want:?}"
        );
    }
}

#[test]
fn adr0041_i4_reads_each_failing_font_once() {
    let mut all = Vec::new();
    for (name, got) in [
        ("pass", "x"),
        ("bad", "z"),
        ("bad", "q"),
        ("absent", "n"),
        ("absent", "m"),
    ] {
        all.extend(
            pair_cases(name, Path::new(name), None, "x", got, "x")
                .unwrap()
                .1,
        );
    }
    let mut calls = Vec::new();
    let data = failure_fonts(&all, |path| {
        calls.push(path.to_owned());
        if path == Path::new("absent") {
            Err(std::io::ErrorKind::NotFound.into())
        } else {
            Ok(vec![7])
        }
    });
    assert_eq!(calls, [PathBuf::from("absent"), PathBuf::from("bad")]);
    assert_eq!(data.len(), 1);
    assert_eq!(data[Path::new("bad")], [7]);
}
