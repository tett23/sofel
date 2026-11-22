//! FreeType's automatic hinter against the pinned FreeType (sofel ADR 0064
//! I1 and I2): every glyph of each font, at many sizes, for the five load
//! targets. For each glyph the slot before rendering (advance, points,
//! tags and contour ends) and the bitmap of the render mode of the same
//! name are compared with `ft-dump --auto-hint`.
//!
//! `SOFEL_AUTOHINT_FONTS` (paths under the test data, separated by `:`)
//! and `SOFEL_AUTOHINT_PPEMS` (`FIRST-LAST`) narrow the run while
//! debugging.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::geom::Point;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::{Location, Variation};
use sofel_outline::{
    AutoHintCache, HintMode, HintTarget, OutlineError, OutlineInfo, Outlines, Scratch,
};
use sofel_raster::{FillRule, LcdMode, MonoOptions, Rasterizer, RenderOptions};

const TARGETS: [(&str, HintTarget); 5] = [
    ("normal", HintTarget::Normal),
    ("light", HintTarget::Light),
    ("mono", HintTarget::Mono),
    ("lcd", HintTarget::Lcd),
    ("lcd-v", HintTarget::VerticalLcd),
];

/// The fonts of the comparison: Latin, CJK, Indic and other scripts, glyf
/// and CFF outlines, a variable font at a non-default location.
const CORPUS: &[(&str, Option<&str>)] = &[
    ("harfbuzz/perf/fonts/Roboto-Regular.ttf", None),
    ("harfbuzz/perf/fonts/Amiri-Regular.ttf", None),
    ("harfbuzz/perf/fonts/NotoSansDuployan-Regular.otf", None),
    ("extra-fonts/fonts/source-serif.otf", None),
    ("extra-fonts/fonts/devanagari.ttf", None),
    ("extra-fonts/fonts/hebrew.ttf", None),
    ("extra-fonts/fonts/khmer.ttf", None),
    ("extra-fonts/fonts/myanmar.ttf", None),
    ("extra-fonts/fonts/arabic.ttf", None),
    ("extra-fonts/fonts/stix-math.otf", None),
    ("extra-fonts/fonts/amstelvar.ttf", Some("wght=700")),
    ("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf", None),
    ("harfbuzz/test/api/fonts/Mplus1p-Regular.660E.ttf", None),
];

/// The sizes of the comparison: small sizes, where the hinter moves points
/// the most, and a few larger ones.
const PPEMS: &[u16] = &[6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 36];

fn ppems() -> Vec<u16> {
    std::env::var("SOFEL_AUTOHINT_PPEMS")
        .ok()
        .and_then(|v| {
            let (a, b) = v.split_once('-')?;
            let (a, b): (u16, u16) = (a.parse().ok()?, b.parse().ok()?);
            Some((a..=b).collect())
        })
        .unwrap_or_else(|| PPEMS.to_vec())
}

fn hex_rows(rows: &mut Vec<String>, pixels: &[u8], stride: usize, count: usize) {
    if stride == 0 {
        return;
    }
    rows.extend(
        pixels
            .chunks_exact(stride)
            .take(count)
            .map(|row| row.iter().map(|v| format!("{v:02x}")).collect::<String>()),
    );
}

/// `FT_Render_Glyph` of the slot in `scratch` with the render mode of
/// the load target's name.
fn render(
    info: OutlineInfo,
    scratch: &Scratch,
    ppem: u16,
    target: HintTarget,
    cff: bool,
    raster: &mut Rasterizer,
) -> Vec<String> {
    let c = info.cbox;
    let options = RenderOptions {
        fill: FillRule::NonZero,
        overlap: info.overlap,
        ppem: Some((ppem, ppem)),
    };
    let mut lines = Vec::new();
    match target {
        HintTarget::Normal | HintTarget::Light => {
            let b = sofel_raster::pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
            raster.clear();
            let multiplier = if info.overlap { 4 } else { 1 };
            if scratch.decompose(b.shift(), multiplier, raster).is_err() {
                return vec!["render_error".into()];
            }
            let Ok(bitmap) = raster.render_placed(b, options) else {
                return vec!["render_error".into()];
            };
            lines.push(format!(
                "bitmap {} {} {} {}",
                bitmap.left, bitmap.top, bitmap.width, bitmap.rows
            ));
            hex_rows(
                &mut lines,
                bitmap.pixels,
                bitmap.width as usize,
                bitmap.rows as usize,
            );
        }
        HintTarget::Mono => {
            // The automatic hinter loads TrueType glyphs with
            // FT_LOAD_NO_SCALE, which leaves FT_OUTLINE_HIGH_PRECISION
            // unset; the CFF loader sets it below 24 ppem regardless.
            let bounds = sofel_raster::mono_pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
            let high_precision = cff && ppem < 24 && bounds.width.saturating_add(bounds.rows) < 258;
            let multiplier = if high_precision { 64 } else { 1 };
            let shift = bounds.shift();
            let offset = Point {
                x: shift.x - 32,
                y: shift.y - 32,
            };
            raster.clear();
            if scratch.decompose(offset, multiplier, raster).is_err() {
                return vec!["render_error".into()];
            }
            let options = MonoOptions {
                high_precision,
                ..Default::default()
            };
            let Ok(bitmap) = raster.render_mono_placed(bounds, options, Some((ppem, ppem))) else {
                return vec!["render_error".into()];
            };
            let b = bitmap.bounds;
            lines.push(format!(
                "bitmap {} {} {} {}",
                b.left, b.top, b.width, b.rows
            ));
            hex_rows(&mut lines, bitmap.pixels, bitmap.stride, b.rows as usize);
        }
        HintTarget::Lcd | HintTarget::VerticalLcd => {
            let mode = if target == HintTarget::Lcd {
                LcdMode::Horizontal
            } else {
                LcdMode::Vertical
            };
            let mut decomposed = true;
            let Ok(bitmap) = raster.render_harmony_placed(
                mode,
                (c.x_min, c.y_min, c.x_max, c.y_max),
                options,
                &mut |offset, raster| decomposed &= scratch.decompose(offset, 1, raster).is_ok(),
            ) else {
                return vec!["render_error".into()];
            };
            if !decomposed {
                return vec!["render_error".into()];
            }
            lines.push(format!(
                "bitmap {} {} {} {}",
                bitmap.left, bitmap.top, bitmap.width, bitmap.rows
            ));
            hex_rows(
                &mut lines,
                bitmap.pixels,
                bitmap.width as usize,
                bitmap.rows as usize,
            );
        }
    }
    lines
}

/// The reference lines of one target, keyed by ppem and glyph.
fn reference(
    path: &Path,
    variations: Option<&str>,
    (first, last): (u16, u16),
    target: &str,
    use_gsub: bool,
) -> BTreeMap<(u16, u32), Vec<String>> {
    let executable = if use_gsub {
        std::env::var_os("SOFEL_FT_DUMP_HB").map_or_else(
            || "target/ft-dump/ft-dump-harfbuzz".into(),
            std::path::PathBuf::from,
        )
    } else {
        paths::ft_dump()
    };
    let mut command = Command::new(executable);
    command.args(["--auto-hint", &first.to_string(), &last.to_string(), target]);
    if let Some(v) = variations {
        command.arg(format!("--variations={v}"));
    }
    let out = command.arg(path).output().unwrap();
    assert!(
        out.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let mut map = BTreeMap::new();
    let mut size = 0u16;
    let mut glyph = None;
    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("size ") {
            if let Some(g) = glyph.take() {
                map.insert((size, g), std::mem::take(&mut lines));
            }
            size = rest.parse().unwrap();
        } else if let Some(rest) = line.strip_prefix("glyph ") {
            if let Some(g) = glyph.replace(rest.parse().unwrap()) {
                map.insert((size, g), std::mem::take(&mut lines));
            }
        } else {
            lines.push(line.to_string());
        }
    }
    if let Some(g) = glyph {
        map.insert((size, g), lines);
    }
    map
}

#[derive(Default)]
struct Stats {
    compared: u64,
    loaded: u64,
    rendered: u64,
    failures: Vec<Failure>,
}

struct Failure {
    id: String,
    bitmap_rows_only: bool,
    detail: String,
}

fn compare_font(relative: &str, variations: Option<&str>, use_gsub: bool, stats: &mut Stats) {
    let path = paths::testdata().join(relative);
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let settings: Vec<Variation> = variations
        .into_iter()
        .flat_map(|v| v.split(','))
        .map(|s| {
            let (tag, value) = s.split_once('=').unwrap();
            let tag: [u8; 4] = tag.as_bytes().try_into().unwrap();
            Variation {
                tag: Tag::new(&tag),
                value: value.parse().unwrap(),
            }
        })
        .collect();
    let location = Location::new(&font, None, &settings);
    let outlines = Outlines::new(font.instance(&location));
    let sizes = ppems();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    for (name, target) in TARGETS {
        let mut cache = if use_gsub {
            sofel_cli::hint::gsub_cache(&outlines).unwrap()
        } else {
            AutoHintCache::new(&outlines)
        };
        for &size in &sizes {
            let expected = reference(&path, variations, (size, size), name, use_gsub);
            for (&(ppem, gid), expected) in &expected {
                stats.compared += 1;
                let loaded =
                    outlines.load_auto_hinted(GlyphId(gid), ppem, target, &mut cache, &mut scratch);
                let mut actual = Vec::new();
                match loaded {
                    Err(e) => actual.push(format!("error {e:?}")),
                    Ok((info, advance)) => {
                        actual.push(format!("advance {advance} 0"));
                        for (p, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                            actual.push(format!("point {} {} {tag}", p.x, p.y));
                        }
                        for end in scratch.contour_ends() {
                            actual.push(format!("end {end}"));
                        }
                        actual.extend(render(
                            info,
                            &scratch,
                            ppem,
                            target,
                            outlines.cff().is_some(),
                            &mut raster,
                        ));
                    }
                }
                let both_errors = expected.first().is_some_and(|l| l.starts_with("error"))
                    && actual.first().is_some_and(|l| l.starts_with("error"));
                if !both_errors {
                    stats.loaded += 1;
                    if expected.iter().any(|l| l.starts_with("bitmap")) {
                        stats.rendered += 1;
                    }
                }
                if !both_errors && &actual != expected {
                    let at = actual
                        .iter()
                        .zip(expected)
                        .position(|(a, b)| a != b)
                        .unwrap_or(actual.len().min(expected.len()));
                    // Only a difference in the rows of a bitmap whose slot
                    // and box agree may be listed as expected.
                    let bitmap_rows_only = actual.len() == expected.len()
                        && actual[..at].iter().any(|l| l.starts_with("bitmap "));
                    stats.failures.push(Failure {
                        id: format!("{relative}:{name}:{ppem}:{gid}"),
                        bitmap_rows_only,
                        detail: format!(
                            "line {at}: sofel {:?} freetype {:?}",
                            actual.get(at),
                            expected.get(at)
                        ),
                    });
                }
            }
        }
    }
    eprintln!(
        "autohint: {relative} done, {} mismatches so far",
        stats.failures.len()
    );
}

#[test]
fn adr0064_i1_i2_auto_hinted_slots_and_bitmaps_match_freetype() {
    paths::require(&paths::ft_dump(), "ft-dump").unwrap();
    let fonts: Vec<(String, Option<String>)> = match std::env::var("SOFEL_AUTOHINT_FONTS") {
        Ok(list) => list.split(':').map(|f| (f.to_string(), None)).collect(),
        Err(_) => CORPUS
            .iter()
            .map(|(f, v)| ((*f).to_string(), v.map(str::to_string)))
            .collect(),
    };
    let mut stats = Stats::default();
    for (font, variations) in &fonts {
        compare_font(font, variations.as_deref(), false, &mut stats);
    }
    let expected = expected_failures();
    let unexpected: Vec<&Failure> = stats
        .failures
        .iter()
        .filter(|f| !(f.bitmap_rows_only && expected.contains_key(&f.id)))
        .collect();
    let failed: std::collections::BTreeSet<&str> =
        stats.failures.iter().map(|f| f.id.as_str()).collect();
    let checked_fonts: std::collections::BTreeSet<&str> =
        fonts.iter().map(|(f, _)| f.as_str()).collect();
    let full_run = std::env::var_os("SOFEL_AUTOHINT_PPEMS").is_none();
    let stale: Vec<&String> = expected
        .keys()
        .filter(|id| {
            full_run
                && checked_fonts.contains(id.split(':').next().unwrap_or(""))
                && !failed.contains(id.as_str())
        })
        .collect();
    eprintln!(
        "autohint: {} slots compared, {} loaded, {} rendered, {} mismatched ({} expected)",
        stats.compared,
        stats.loaded,
        stats.rendered,
        stats.failures.len(),
        stats.failures.len() - unexpected.len()
    );
    for failure in unexpected.iter().take(40) {
        eprintln!("{} {}", failure.id, failure.detail);
    }
    if let Some(out) = std::env::var_os("SOFEL_AUTOHINT_FAILURES") {
        let lines: Vec<String> = stats
            .failures
            .iter()
            .map(|f| format!("{} {} {}", f.id, f.bitmap_rows_only, f.detail))
            .collect();
        std::fs::write(out, lines.join("\n")).unwrap();
    }
    assert!(stats.loaded > 0);
    assert!(
        unexpected.is_empty(),
        "{} unexpected mismatches",
        unexpected.len()
    );
    assert!(stale.is_empty(), "expected failures that pass: {stale:?}");
}

#[test]
fn adr0085_a1_gsub_hinted_slots_and_bitmaps_match_harfbuzz_freetype() {
    let fonts: Vec<(String, Option<String>)> = match std::env::var("SOFEL_AUTOHINT_FONTS") {
        Ok(list) => list
            .split(':')
            .map(|font| (font.to_owned(), None))
            .collect(),
        Err(_) => CORPUS
            .iter()
            .map(|(font, variation)| (font.to_string(), variation.map(str::to_string)))
            .collect(),
    };
    let mut stats = Stats::default();
    for (font, variation) in &fonts {
        compare_font(font, variation.as_deref(), true, &mut stats);
    }
    eprintln!(
        "adr0085: {} slots, {} loaded, {} rendered, {} mismatched",
        stats.compared,
        stats.loaded,
        stats.rendered,
        stats.failures.len()
    );
    for failure in stats.failures.iter().take(40) {
        eprintln!("{} {}", failure.id, failure.detail);
    }
    assert!(stats.loaded > 0);
    assert_eq!(stats.failures.len(), 0);
}

/// `conformance/expected-failures-autohint.txt`: `<id>  # <reason>`.
fn expected_failures() -> BTreeMap<String, String> {
    let text =
        std::fs::read_to_string(paths::conformance_file("expected-failures-autohint.txt")).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (id, reason) = l.split_once('#').expect("a reason after #");
            assert!(!reason.trim().is_empty(), "{l}");
            (id.trim().to_string(), reason.trim().to_string())
        })
        .collect()
}

#[test]
fn adr0064_i1_load_with_hint_auto_uses_the_automatic_hinter() {
    let path = paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf");
    let data = std::fs::read(path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut cache = AutoHintCache::new(&outlines);
    let (mut a, mut b) = (Scratch::default(), Scratch::default());
    let glyph = font.nominal_glyph(u32::from('g')).unwrap();
    for (_, target) in TARGETS {
        let cached = outlines
            .load_auto_hinted(glyph, 13, target, &mut cache, &mut a)
            .unwrap();
        let fresh = outlines
            .load_with_hint(glyph, 13, HintMode::Auto, target, &mut b)
            .unwrap();
        assert_eq!(cached, fresh);
        assert_eq!(a.points(), b.points());
        assert_eq!(a.point_tags(), b.point_tags());
    }
    assert_eq!(
        outlines.load_auto_hinted(
            GlyphId(u32::MAX),
            13,
            HintTarget::Normal,
            &mut cache,
            &mut a
        ),
        Err(OutlineError::InvalidGlyphIndex)
    );
    assert!(a.points().is_empty());
}
