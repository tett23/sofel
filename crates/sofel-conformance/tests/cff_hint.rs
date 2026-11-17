//! FreeType's CFF and CFF2 hinter (the Adobe engine) against the pinned
//! FreeType (sofel ADR 0064 I1 and I2): every CFF and CFF2 face of the
//! test data (each distinct file once, every face of a collection), at
//! many sizes, for the five load targets. For each glyph the slot before
//! rendering (advance, points, tags and contour ends) and the bitmap of
//! the render mode of the same name are compared with `ft-dump
//! --native-hint`. A variable CFF2 face is also compared halfway between
//! the default and the maximum of every axis. Of a face with more than
//! [`GLYPH_CAP`] glyphs, every nth glyph is compared so that about that
//! many remain.
//!
//! `SOFEL_CFF_HINT_FONTS` (paths under the test data, separated by `:`)
//! and `SOFEL_CFF_HINT_PPEMS` (`FIRST-LAST`) narrow the run while
//! debugging; `SOFEL_CFF_HINT_GLYPH_CAP` changes the cap (0 compares every
//! glyph); `SOFEL_CFF_HINT_FAILURES` names a file that receives every
//! mismatch.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::geom::Point;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::{Location, Variation};
use sofel_outline::{HintMode, HintTarget, OutlineInfo, Outlines, Scratch};
use sofel_raster::{FillRule, LcdMode, MonoOptions, Rasterizer, RenderOptions};

const TARGETS: [(&str, HintTarget); 5] = [
    ("normal", HintTarget::Normal),
    ("light", HintTarget::Light),
    ("mono", HintTarget::Mono),
    ("lcd", HintTarget::Lcd),
    ("lcd-v", HintTarget::VerticalLcd),
];

/// The most glyphs compared per face by default.
const GLYPH_CAP: u32 = 100;

fn glyph_cap() -> u32 {
    std::env::var("SOFEL_CFF_HINT_GLYPH_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(GLYPH_CAP)
}

/// The sizes of the comparison: every size from 5 to 20 ppem, where the
/// hinter moves points the most, and 24 (the first size without
/// FT_OUTLINE_HIGH_PRECISION) and 36.
fn ppems() -> Vec<u16> {
    std::env::var("SOFEL_CFF_HINT_PPEMS")
        .ok()
        .and_then(|v| {
            let (a, b) = v.split_once('-')?;
            let (a, b): (u16, u16) = (a.parse().ok()?, b.parse().ok()?);
            Some((a..=b).collect())
        })
        .unwrap_or_else(|| (5..=20).chain([24, 36]).collect())
}

/// One face of the comparison.
#[derive(Clone)]
struct Case {
    path: String,
    face: u32,
    variations: Option<String>,
}

impl Case {
    fn id(&self) -> String {
        let mut id = self.path.clone();
        if self.face != 0 {
            id.push_str(&format!("#{}", self.face));
        }
        if let Some(v) = &self.variations {
            id.push_str(&format!("@{v}"));
        }
        id
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&path, out);
        } else if kind.is_file() {
            out.push(path);
        }
    }
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The faces of a file that FreeType's CFF driver loads: sfnt or
/// collection faces whose version is `OTTO` and that have `CFF ` or
/// `CFF2`.
fn cff_faces(data: &[u8]) -> Vec<u32> {
    let count = if data.starts_with(b"ttcf") {
        be32(data, 8).unwrap_or(0)
    } else {
        1
    };
    (0..count)
        .filter(|&i| {
            let Ok(font) = Font::new(data, i) else {
                return false;
            };
            font.sfnt().version() == u32::from_be_bytes(*b"OTTO")
                && (font.table(Tag::new(b"CFF ")).is_some()
                    || font.table(Tag::new(b"CFF2")).is_some())
        })
        .collect()
}

/// Halfway between the default and the maximum of every axis, in whole
/// units, if that differs from the default.
fn midway(font: &Font<'_>) -> Option<String> {
    let fvar = sofel_font::read::fvar::Fvar::new(font.table(Tag::new(b"fvar"))?).ok()?;
    let settings: Vec<String> = fvar
        .axes()
        .filter_map(|axis| {
            let (default, max) = (axis.default.0 >> 16, axis.max.0 >> 16);
            let value = default + (max - default) / 2;
            (value != default && axis.min.0 <= axis.default.0 && axis.default.0 <= axis.max.0)
                .then(|| format!("{}={value}", axis.tag))
        })
        .collect();
    (!settings.is_empty()).then(|| settings.join(","))
}

/// Every CFF and CFF2 face of the test data, once per distinct file.
fn cases() -> Vec<Case> {
    let root = paths::testdata();
    let files: Vec<PathBuf> = match std::env::var("SOFEL_CFF_HINT_FONTS") {
        Ok(list) => list.split(':').map(|f| root.join(f)).collect(),
        Err(_) => {
            let mut files = Vec::new();
            walk(&root, &mut files);
            files.sort();
            files
        }
    };
    let mut seen = HashSet::new();
    let mut cases = Vec::new();
    for file in files {
        let Ok(data) = std::fs::read(&file) else {
            continue;
        };
        let faces = cff_faces(&data);
        if faces.is_empty() {
            continue;
        }
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        if !seen.insert(hasher.finish()) {
            continue;
        }
        let path = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for face in faces {
            cases.push(Case {
                path: path.clone(),
                face,
                variations: None,
            });
            let font = Font::new(&data, face).unwrap();
            if font.table(Tag::new(b"CFF2")).is_some()
                && let Some(v) = midway(&font)
            {
                cases.push(Case {
                    path: path.clone(),
                    face,
                    variations: Some(v),
                });
            }
        }
    }
    cases
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
/// the load target's name. The CFF loader sets FT_OUTLINE_HIGH_PRECISION
/// below 24 ppem.
fn render(
    info: OutlineInfo,
    scratch: &Scratch,
    ppem: u16,
    target: HintTarget,
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
            if scratch.decompose(b.shift(), 1, raster).is_err() {
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
            let bounds = sofel_raster::mono_pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
            let high_precision = ppem < 24 && bounds.width.saturating_add(bounds.rows) < 258;
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

#[derive(Default)]
struct Stats {
    /// Faces FreeType does not open or size (`ft-dump` fails before the
    /// first glyph).
    unopened: Vec<String>,
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

/// The settings of `--variations=`.
fn settings(variations: Option<&str>) -> Vec<Variation> {
    variations
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
        .collect()
}

/// Compares one glyph's reference lines with sofel's.
#[allow(clippy::too_many_arguments)]
fn compare_glyph(
    case: &str,
    target: (&str, HintTarget),
    ppem: u16,
    gid: u32,
    expected: &[String],
    outlines: &Outlines<'_, '_, '_>,
    scratch: &mut Scratch,
    raster: &mut Rasterizer,
    stats: &mut Stats,
) {
    stats.compared += 1;
    let loaded =
        outlines.load_with_hint(GlyphId(gid), ppem, HintMode::NativeV40, target.1, scratch);
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
            actual.extend(render(info, scratch, ppem, target.1, raster));
        }
    }
    let both_errors = expected.first().is_some_and(|l| l.starts_with("error"))
        && actual.first().is_some_and(|l| l.starts_with("error"));
    if both_errors {
        return;
    }
    stats.loaded += 1;
    if expected.iter().any(|l| l.starts_with("bitmap")) {
        stats.rendered += 1;
    }
    if actual != expected {
        let at = actual
            .iter()
            .zip(expected)
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()));
        // Only a difference in the rows of a bitmap whose slot and box
        // agree may be listed as expected.
        let bitmap_rows_only =
            actual.len() == expected.len() && actual[..at].iter().any(|l| l.starts_with("bitmap "));
        stats.failures.push(Failure {
            id: format!("{case}:{}:{ppem}:{gid}", target.0),
            bitmap_rows_only,
            detail: format!(
                "line {at}: sofel {:?} freetype {:?}",
                actual.get(at),
                expected.get(at)
            ),
        });
    }
}

/// Runs `ft-dump --native-hint` over the sizes and compares each glyph
/// as its lines arrive.
fn compare_case(case: &Case, sizes: &[u16], stats: &mut Stats) {
    let path = paths::testdata().join(&case.path);
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, case.face).unwrap();
    let settings = settings(case.variations.as_deref());
    let location = Location::new(&font, None, &settings);
    let outlines = Outlines::new(font.instance(&location));
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let id = case.id();
    let (first, last) = (*sizes.iter().min().unwrap(), *sizes.iter().max().unwrap());
    let wanted: BTreeSet<u16> = sizes.iter().copied().collect();
    for target in TARGETS {
        let mut command = Command::new(paths::ft_dump());
        command.args([
            "--native-hint",
            &first.to_string(),
            &last.to_string(),
            target.0,
        ]);
        if let Some(v) = &case.variations {
            command.arg(format!("--variations={v}"));
        }
        if case.face != 0 {
            command.arg(format!("--face={}", case.face));
        }
        let cap = glyph_cap();
        if cap > 0 && font.num_glyphs() > cap {
            command.arg(format!("--glyph-step={}", font.num_glyphs().div_ceil(cap)));
        }
        let mut child = command
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut any = false;
        let mut size = 0u16;
        let mut glyph: Option<u32> = None;
        let mut lines: Vec<String> = Vec::new();
        let mut flush = |size: u16, glyph: Option<u32>, lines: &mut Vec<String>| {
            if let Some(g) = glyph
                && wanted.contains(&size)
            {
                compare_glyph(
                    &id,
                    target,
                    size,
                    g,
                    lines,
                    &outlines,
                    &mut scratch,
                    &mut raster,
                    stats,
                );
            }
            lines.clear();
        };
        for line in reader.lines() {
            let line = line.unwrap();
            if let Some(rest) = line.strip_prefix("size ") {
                flush(size, glyph.take(), &mut lines);
                size = rest.parse().unwrap();
            } else if let Some(rest) = line.strip_prefix("glyph ") {
                any = true;
                flush(size, glyph.replace(rest.parse().unwrap()), &mut lines);
            } else {
                lines.push(line);
            }
        }
        flush(size, glyph, &mut lines);
        let status = child.wait().unwrap();
        if !status.success() && !any {
            stats.unopened.push(id);
            return;
        }
        assert!(status.success(), "{id}: ft-dump --native-hint failed");
    }
}

/// Whether the expected identifier `pattern` (fields may be `*`) names
/// the mismatch `id`.
fn matches(pattern: &str, id: &str) -> bool {
    let (p, i): (Vec<&str>, Vec<&str>) = (pattern.split(':').collect(), id.split(':').collect());
    p.len() == i.len() && p.iter().zip(&i).all(|(p, i)| *p == "*" || p == i)
}

/// `conformance/expected-failures-cff-hint.txt`: `<id>  # <reason>`.
fn expected_failures() -> BTreeMap<String, String> {
    let text =
        std::fs::read_to_string(paths::conformance_file("expected-failures-cff-hint.txt")).unwrap();
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
fn adr0064_i1_i2_cff_hinted_slots_and_bitmaps_match_freetype() {
    paths::require(&paths::ft_dump(), "ft-dump").unwrap();
    let cases = cases();
    assert!(!cases.is_empty());
    let sizes = ppems();
    let queue = Mutex::new(cases.iter().collect::<Vec<_>>());
    let total = Mutex::new(Stats::default());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some(case) = queue.lock().unwrap().pop() else {
                        break;
                    };
                    let mut stats = Stats::default();
                    compare_case(case, &sizes, &mut stats);
                    let mut t = total.lock().unwrap();
                    t.unopened.extend(stats.unopened);
                    t.compared += stats.compared;
                    t.loaded += stats.loaded;
                    t.rendered += stats.rendered;
                    t.failures.extend(stats.failures);
                }
            });
        }
    });
    let mut stats = total.into_inner().unwrap();
    stats.failures.sort_by(|a, b| a.id.cmp(&b.id));
    let expected = expected_failures();
    let mut used = BTreeSet::new();
    let unexpected: Vec<&Failure> = stats
        .failures
        .iter()
        .filter(|f| {
            let hit = expected.iter().find(|(id, reason)| {
                matches(id, &f.id) && (f.bitmap_rows_only || reason.starts_with("[slot]"))
            });
            if let Some((id, _)) = hit {
                used.insert(id.clone());
            }
            hit.is_none()
        })
        .collect();
    let checked: BTreeSet<String> = cases.iter().map(Case::id).collect();
    let full_run = std::env::var_os("SOFEL_CFF_HINT_PPEMS").is_none()
        && std::env::var_os("SOFEL_CFF_HINT_GLYPH_CAP").is_none();
    let stale: Vec<&String> = expected
        .keys()
        .filter(|id| {
            let case = id.split(':').next().unwrap_or("");
            full_run && checked.contains(case) && !used.contains(*id)
        })
        .collect();
    eprintln!(
        "cff hint: {} faces ({} FreeType does not open), {} slots compared, {} loaded, {} rendered, {} mismatched ({} expected)",
        cases.len(),
        stats.unopened.len(),
        stats.compared,
        stats.loaded,
        stats.rendered,
        stats.failures.len(),
        stats.failures.len() - unexpected.len()
    );
    for failure in unexpected.iter().take(40) {
        eprintln!("{} {}", failure.id, failure.detail);
    }
    if let Some(out) = std::env::var_os("SOFEL_CFF_HINT_FAILURES") {
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
