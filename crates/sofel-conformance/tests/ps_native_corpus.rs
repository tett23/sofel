//! Real PostScript corpus comparison for root ADR 0064 I1 and I2.
//! Type 1 runs in CI; CID needs an explicitly licensed source before CI can fetch it.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use std::path::Path;
use std::process::Command as Process;

use sofel_font::{CidFace, Type1Face, Type1Source};
use sofel_geom::{div_fix, Command, Fixed, FnPen, Point};
use sofel_outline::{draw_cid_native, draw_type1_native, HintTarget, OutlineInfo, Scale};
use sofel_raster::{FillRule, LcdMode, MonoOptions, Rasterizer, RenderOptions};

fn shifted(command: Command, offset: Point, multiplier: i32) -> Command {
    let point = |p: Point| Point {
        x: p.x.saturating_add(offset.x).saturating_mul(multiplier),
        y: p.y.saturating_add(offset.y).saturating_mul(multiplier),
    };
    match command {
        Command::MoveTo(p) => Command::MoveTo(point(p)),
        Command::LineTo(p) => Command::LineTo(point(p)),
        Command::QuadTo(c, p) => Command::QuadTo(point(c), point(p)),
        Command::CubicTo(c, d, p) => Command::CubicTo(point(c), point(d), point(p)),
        Command::Close => Command::Close,
    }
}

fn render(commands: &[Command], info: OutlineInfo, target: HintTarget) -> Vec<String> {
    let c = info.cbox;
    let mut raster = Rasterizer::new();
    let options = RenderOptions {
        fill: FillRule::NonZero,
        overlap: info.overlap,
        ppem: Some((16, 16)),
    };
    let (left, top, width, rows, stride, pixels) = match target {
        HintTarget::Normal | HintTarget::Light => {
            let bounds = sofel_raster::pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
            for &command in commands {
                shifted(command, bounds.shift(), 1).apply(&mut raster);
            }
            let bitmap = raster.render_placed(bounds, options).unwrap();
            (
                bitmap.left,
                bitmap.top,
                bitmap.width,
                bitmap.rows,
                bitmap.width as usize,
                bitmap.pixels.to_vec(),
            )
        }
        HintTarget::Mono => {
            let bounds = sofel_raster::mono_pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
            let high_precision = bounds.width.saturating_add(bounds.rows) < 258;
            let multiplier = if high_precision { 64 } else { 1 };
            let shift = bounds.shift();
            let offset = Point {
                x: shift.x.saturating_sub(32),
                y: shift.y.saturating_sub(32),
            };
            for &command in commands {
                shifted(command, offset, multiplier).apply(&mut raster);
            }
            let bitmap = raster
                .render_mono_placed(
                    bounds,
                    MonoOptions {
                        high_precision,
                        ..Default::default()
                    },
                    Some((16, 16)),
                )
                .unwrap();
            (
                bounds.left,
                bounds.top,
                bounds.width,
                bounds.rows,
                bitmap.stride,
                bitmap.pixels.to_vec(),
            )
        }
        HintTarget::Lcd | HintTarget::VerticalLcd => {
            let mode = if target == HintTarget::Lcd {
                LcdMode::Horizontal
            } else {
                LcdMode::Vertical
            };
            let bitmap = raster
                .render_harmony_placed(
                    mode,
                    (c.x_min, c.y_min, c.x_max, c.y_max),
                    options,
                    &mut |offset, raster| {
                        for &command in commands {
                            shifted(command, offset, 1).apply(raster);
                        }
                    },
                )
                .unwrap();
            (
                bitmap.left,
                bitmap.top,
                bitmap.width,
                bitmap.rows,
                bitmap.width as usize,
                bitmap.pixels.to_vec(),
            )
        }
    };
    let mut lines = vec![format!("bitmap {left} {top} {width} {rows}")];
    if stride > 0 {
        lines.extend(
            pixels
                .chunks_exact(stride)
                .take(rows as usize)
                .map(|row| row.iter().map(|b| format!("{b:02x}")).collect::<String>()),
        );
    }
    lines
}

fn reference(path: &Path, target: &str) -> Vec<(usize, Vec<String>)> {
    let output = Process::new(sofel_conformance::paths::ft_dump())
        .args(["--native-hint", "16", "16", target])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut records = Vec::new();
    for line in String::from_utf8(output.stdout).unwrap().lines() {
        if let Some(index) = line.strip_prefix("glyph ") {
            records.push((index.parse().unwrap(), Vec::new()));
        } else if !line.starts_with("size ") {
            records
                .last_mut()
                .expect("glyph before data")
                .1
                .push(line.to_owned());
        }
    }
    records
}

fn slot(commands: &[Command], advance: i32) -> Vec<String> {
    let mut result = vec![format!("advance {advance} 0")];
    let mut ends = Vec::new();
    let mut contour: Vec<(Point, u8)> = Vec::new();
    let flush =
        |contour: &mut Vec<(Point, u8)>, result: &mut Vec<String>, ends: &mut Vec<String>| {
            if contour.is_empty() {
                return;
            }
            if contour.len() > 1 && contour.last().unwrap().0 == contour[0].0 {
                contour.pop();
            }
            for (p, tag) in contour.iter() {
                result.push(format!("point {} {} {tag}", p.x, p.y));
            }
            ends.push(format!(
                "end {}",
                result
                    .iter()
                    .filter(|line| line.starts_with("point "))
                    .count()
                    - 1
            ));
            contour.clear();
        };
    for &command in commands {
        match command {
            Command::MoveTo(p) => {
                flush(&mut contour, &mut result, &mut ends);
                contour.push((p, 1));
            }
            Command::LineTo(p) => contour.push((p, 1)),
            Command::QuadTo(c, p) => {
                contour.push((c, 0));
                contour.push((p, 1));
            }
            Command::CubicTo(c, d, p) => {
                contour.push((c, 2));
                contour.push((d, 2));
                contour.push((p, 1));
            }
            Command::Close => flush(&mut contour, &mut result, &mut ends),
        }
    }
    flush(&mut contour, &mut result, &mut ends);
    result.extend(ends);
    result
}

fn compare(path: &Path, kind: &str, bytes: &[u8]) -> (usize, usize, usize) {
    let type1_source =
        (kind == "type1").then(|| Type1Source::parse_pfb(bytes, bytes.len() * 32).unwrap());
    let type1 = type1_source
        .as_ref()
        .map(|source| Type1Face::new(source).unwrap());
    let cid = (kind == "cid").then(|| CidFace::parse(bytes, bytes.len() * 32).unwrap());
    let count = type1
        .as_ref()
        .map_or_else(|| cid.as_ref().unwrap().num_glyphs(), Type1Face::num_glyphs);
    let upem = cid
        .as_ref()
        .map_or(1000, |face| i32::from(face.units_per_em()));
    let factor = Fixed(div_fix(16 * 64, upem));
    let scale = Scale::Scaled {
        x: factor,
        y: factor,
    };
    let mut checks = 0;
    let mut slot_failures = 0;
    let mut bitmap_failures = 0;
    let mut shown_different_slot = false;
    let mut unsupported = 0;
    let mut invalid_index = 0;
    let mut other_error = 0;
    let mut value_difference = 0;
    for (name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
        ("lcd", HintTarget::Lcd),
        ("lcd-v", HintTarget::VerticalLcd),
    ] {
        let expected = reference(path, name);
        assert_eq!(expected.len(), count, "{} {name}", path.display());
        for (glyph, record) in expected {
            checks += 1;
            let mut commands = Vec::new();
            let loaded = if let Some(face) = &type1 {
                draw_type1_native(face, glyph, scale, target, &mut FnPen(|c| commands.push(c)))
            } else {
                draw_cid_native(
                    cid.as_ref().unwrap(),
                    glyph,
                    scale,
                    target,
                    &mut FnPen(|c| commands.push(c)),
                )
            };
            let Ok((info, advance)) = loaded else {
                slot_failures += 1;
                match loaded.err().unwrap() {
                    sofel_outline::OutlineError::UnsupportedFormat => unsupported += 1,
                    sofel_outline::OutlineError::InvalidGlyphIndex => invalid_index += 1,
                    _ => other_error += 1,
                }
                if slot_failures <= 5 {
                    eprintln!(
                        "{} {name} glyph {glyph}: sofel load {:?}, FT {:?}",
                        path.display(),
                        loaded.err(),
                        record.first()
                    );
                }
                continue;
            };
            let actual = slot(&commands, advance);
            let split = record
                .iter()
                .position(|line| line.starts_with("bitmap "))
                .unwrap_or(record.len());
            if actual != record[..split] {
                slot_failures += 1;
                value_difference += 1;
                if !shown_different_slot {
                    let first = actual
                        .iter()
                        .zip(&record[..split])
                        .position(|(a, b)| a != b);
                    eprintln!(
                        "{} {name} glyph {glyph}: first slot difference {first:?}: FT {:?}, sofel {:?}",
                        path.display(),
                        first.and_then(|i| record.get(i)),
                        first.and_then(|i| actual.get(i))
                    );
                    shown_different_slot = true;
                }
            }
            let rendered = render(&commands, info, target);
            if rendered != record[split..] {
                bitmap_failures += 1;
                if bitmap_failures <= 5 {
                    eprintln!(
                        "{} {name} glyph {glyph}: bitmap FT {:?}, sofel {:?}",
                        path.display(),
                        record.get(split),
                        rendered.first()
                    );
                }
            }
        }
    }
    eprintln!(
        "{kind} breakdown: UnsupportedFormat={unsupported}, InvalidGlyphIndex={invalid_index}, other_error={other_error}, value_difference={value_difference}"
    );
    (checks, slot_failures, bitmap_failures)
}

fn check_font(path: &Path, kind: &str) {
    let bytes = std::fs::read(path).unwrap();
    let counts = compare(path, kind, &bytes);
    eprintln!(
        "{}: {} slots, {} slot mismatches, {} bitmap mismatches",
        path.display(),
        counts.0,
        counts.1,
        counts.2
    );
    assert_eq!(
        (counts.1, counts.2),
        (0, 0),
        "{} slots; {} slot mismatches; {} bitmap mismatches",
        counts.0,
        counts.1,
        counts.2
    );
}

#[test]
fn adr0064_i1_real_type1_corpus_matches_freetype() {
    let path = sofel_conformance::paths::testdata().join("extra-fonts/fonts/cmr10.pfb");
    check_font(&path, "type1");
}

#[test]
fn adr0064_i1_real_cid_corpus_matches_freetype_when_available() {
    let Some(root) = std::env::var_os("SOFEL_PS_NATIVE_CORPUS") else {
        eprintln!("skip: SOFEL_PS_NATIVE_CORPUS is unset; 0 real CID comparisons");
        return;
    };
    check_font(&Path::new(&root).join("HangingS.cid"), "cid");
}
