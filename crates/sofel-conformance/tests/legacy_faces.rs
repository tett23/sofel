//! Real legacy fonts compared with pinned FreeType (root ADR 0090 I3).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

use sofel_bitmap::{BitmapError, PixelMode, bitmap_layout, draw_bitmap, strikes};
use sofel_conformance::outline::{parse, sofel_glyph};
use sofel_conformance::paths;
use sofel_conformance::{color, expect};
use sofel_font::read::GlyphId;
use sofel_font::read::{Tag, cmap::Cmap};
use sofel_font::{Font, Type1Face, Type1Source};
use sofel_geom::{Command as PathCommand, FnPen};
use sofel_outline::{Outlines, Scale, Scratch, draw_type1};

fn font_path(name: &str) -> std::path::PathBuf {
    paths::testdata().join("extra-fonts/fonts").join(name)
}

fn reference(args: &[&str], path: &Path) -> String {
    let output = Command::new(paths::ft_dump())
        .args(args)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn sofel_sbit(font: &Font<'_>) -> String {
    let mut out = String::new();
    for (size, strike) in strikes(font).enumerate() {
        writeln!(out, "size {size}").unwrap();
        for glyph in 0..font.num_glyphs() {
            match bitmap_layout(font, &strike, glyph) {
                Err(BitmapError::Missing) => writeln!(out, "glyph {glyph} missing").unwrap(),
                Err(_) => writeln!(out, "glyph {glyph} error").unwrap(),
                Ok(layout) => {
                    let mut pixels = vec![0; layout.len()];
                    match draw_bitmap(font, &strike, glyph, &mut pixels) {
                        Err(_) => writeln!(out, "glyph {glyph} error").unwrap(),
                        Ok(layout) => {
                            let m = layout.metrics;
                            let mode = match layout.pixel_mode {
                                PixelMode::Mono => 1,
                                PixelMode::Gray => 2,
                                PixelMode::Gray2 => 3,
                                PixelMode::Gray4 => 4,
                                PixelMode::Lcd => 5,
                                PixelMode::LcdV => 6,
                                PixelMode::Bgra => 7,
                            };
                            writeln!(
                                out,
                                "glyph {glyph} 0 {} {} {} {} {} {} {} {} {} {}",
                                m.hori_bearing_x,
                                m.hori_bearing_y,
                                layout.width,
                                layout.rows,
                                layout.pitch,
                                mode,
                                i64::from(m.hori_advance) * 64,
                                i64::from(m.vert_bearing_x) * 64,
                                i64::from(m.vert_bearing_y) * 64,
                                i64::from(m.vert_advance) * 64
                            )
                            .unwrap();
                            for row in pixels
                                .chunks(layout.pitch.max(1) as usize)
                                .take(layout.rows as usize)
                            {
                                for byte in row {
                                    write!(out, "{byte:02x}").unwrap();
                                }
                                writeln!(out).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn normalized_sbit(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        match fields.as_slice() {
            ["glyph", id, error] => {
                let kind = if *error == "-1" || *error == "157" {
                    "missing"
                } else {
                    "error"
                };
                writeln!(out, "glyph {id} {kind}").unwrap();
            }
            ["glyph", id, _, _, "0", "0", _, _, _, _, _, _, _] => {
                writeln!(out, "glyph {id} missing").unwrap()
            }
            ["strike", ..] | ["face", ..] | ["end", ..] => {}
            _ => writeln!(out, "{line}").unwrap(),
        }
    }
    out
}

#[test]
fn adr0090_i3_bitmap_face_counts_and_full_cmaps() {
    let mut results = Vec::new();
    for name in ["spleen-8x16.bdf", "spleen-8x16.pcf", "spleen-8x16.fon"] {
        let path = font_path(name);
        let bytes = std::fs::read(&path).unwrap();
        let Ok(face) = Font::from_bitmap(&bytes, 0) else {
            results.push(("spleen-fon/face-open", false));
            continue;
        };
        if name.ends_with(".fon") {
            results.push(("spleen-fon/face-open", true));
        }
        let dump = reference(&[], &path);
        let counts = dump
            .lines()
            .filter(|line| line.starts_with("face "))
            .count();
        assert_eq!(face.face_count() as usize, counts, "{name}");
        assert_eq!(
            dump.lines()
                .find(|line| line.starts_with("num_glyphs "))
                .unwrap(),
            format!("num_glyphs {}", face.glyph_count()),
            "{name}"
        );
        let strike = reference(&["--sbit"], &path);
        let strike = strike
            .lines()
            .find(|line| line.starts_with("strike 0 "))
            .unwrap();
        let fields = strike.split_ascii_whitespace().collect::<Vec<_>>();
        assert_eq!(
            face.ppem_26_6(),
            (fields[5].parse().unwrap(), fields[6].parse().unwrap()),
            "{name}"
        );
        let mut reference_map = BTreeMap::new();
        for line in dump.lines().filter(|line| line.starts_with("map ")) {
            let mut fields = line.split_ascii_whitespace();
            fields.next();
            let code = fields.next().unwrap().parse::<u32>().unwrap();
            let glyph = fields.next().unwrap().parse::<u32>().unwrap();
            assert_eq!(
                face.nominal_glyph(code),
                Some(GlyphId(glyph)),
                "{name}: U+{code:04X}"
            );
            reference_map.insert(code, glyph);
        }
        for code in 0..=0xffff {
            assert_eq!(
                face.nominal_glyph(code).map(|glyph| glyph.0),
                reference_map.get(&code).copied(),
                "{name}: U+{code:04X}"
            );
        }
        for line in dump.lines().filter(|line| line.starts_with("advance ")) {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            let id = fields[1].parse::<u32>().unwrap();
            let value = fields[2].parse::<i32>().unwrap();
            assert_eq!(
                face.glyph(GlyphId(id)).unwrap().advance * 64,
                value,
                "{name}: advance {id}"
            );
        }
    }
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-legacy.txt"
    ));
    let expected = expected
        .into_iter()
        .filter(|id| id.starts_with("spleen-fon/"))
        .collect();
    let problems = expect::judge(results, &expected);
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-legacy.txt")
    );
}

#[test]
fn adr0090_i3_bitmap_strike_pixels_match_freetype() {
    for name in ["spleen-8x16.bdf", "spleen-8x16.pcf"] {
        let path = font_path(name);
        let bytes = std::fs::read(&path).unwrap();
        let face = Font::from_bitmap(&bytes, 0).unwrap();
        let output = reference(&["--raster", "16", "--mono"], &path);
        let mut lines = output.lines().peekable();
        let mut compared = 0;
        while let Some(line) = lines.next() {
            let Some(glyph) = line.strip_prefix("glyph ") else {
                continue;
            };
            let id = glyph.parse::<u32>().unwrap();
            let header = lines.next().unwrap();
            let ours = face.glyph(GlyphId(id)).unwrap();
            let expected = format!(
                "bitmap {} {} {} {}",
                ours.left, ours.top, ours.width, ours.height
            );
            assert_eq!(header, expected, "{name}: glyph {id}");
            let stride = usize::from(ours.width).div_ceil(8);
            assert_eq!(
                ours.bitmap.len(),
                stride * usize::from(ours.height),
                "{name}: glyph {id}"
            );
            for row in ours.bitmap.chunks(stride) {
                let reference_row = lines.next().unwrap();
                let actual = row
                    .iter()
                    .map(|value| format!("{value:02x}"))
                    .collect::<String>();
                assert_eq!(reference_row, actual, "{name}: glyph {id}");
            }
            compared += 1;
        }
        assert_eq!(compared, face.glyph_count(), "{name}");
    }
}

#[test]
fn adr0090_i3_dfont_strike_pixels_match_freetype() {
    let path = font_path("spleen-8x16.dfont");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let reference = reference(&["--sbit"], &path);
    assert_eq!(normalized_sbit(&reference), sofel_sbit(&font));
}

#[test]
fn adr0090_i3_webfont_unscaled_outlines_match_freetype() {
    for name in ["spleen-8x16.woff", "spleen-8x16.woff2"] {
        if name.ends_with(".woff2") && std::env::var_os("SOFEL_FT_BROTLI_DUMP").is_none() {
            continue;
        }
        let path = font_path(name);
        let bytes = std::fs::read(&path).unwrap();
        let decoded = if name.ends_with(".woff2") {
            sofel_woff::decode_woff2(&bytes, 1 << 20).unwrap()
        } else {
            sofel_woff::decode_woff(&bytes, 1 << 20).unwrap()
        };
        let font = Font::new(&decoded, 0).unwrap();
        let dump = if name.ends_with(".woff2") {
            std::env::var_os("SOFEL_FT_BROTLI_DUMP")
                .map(std::path::PathBuf::from)
                .unwrap()
        } else {
            paths::ft_dump()
        };
        let output = Command::new(dump)
            .args(["--outline", "unscaled"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let faces = parse(&String::from_utf8(output.stdout).unwrap()).unwrap();
        let reference = &faces[&0];
        assert_eq!(reference.num_glyphs, font.num_glyphs(), "{name}");
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        for glyph in 0..font.num_glyphs() {
            assert_eq!(
                sofel_glyph(&outlines, glyph, Scale::Unscaled, &mut scratch),
                reference.glyphs[&glyph],
                "{name}: glyph {glyph}"
            );
        }
    }
}

#[test]
fn adr0090_i3_spleen_webfont_and_dfont_cmaps() {
    for name in ["spleen-8x16.woff", "spleen-8x16.woff2", "spleen-8x16.dfont"] {
        let path = font_path(name);
        let bytes = std::fs::read(&path).unwrap();
        let decoded = match name {
            "spleen-8x16.woff" => sofel_woff::decode_woff(&bytes, 1 << 20).unwrap(),
            "spleen-8x16.woff2" => sofel_woff::decode_woff2(&bytes, 1 << 20).unwrap(),
            _ => bytes.clone(),
        };
        let font = Font::new(&decoded, 0).unwrap();
        let dump = if name.ends_with(".woff2") {
            std::env::var_os("SOFEL_FT_BROTLI_DUMP")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(paths::ft_dump)
        } else {
            paths::ft_dump()
        };
        if name.ends_with(".woff2") && std::env::var_os("SOFEL_FT_BROTLI_DUMP").is_none() {
            // The local default ft-dump has no Brotli; CI supplies the pinned Brotli build.
            continue;
        }
        let reference = Command::new(dump).arg(&path).output().unwrap();
        assert!(
            reference.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let text = String::from_utf8(reference.stdout).unwrap();
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("face "))
                .count(),
            1,
            "{name}"
        );
        assert_eq!(
            text.lines()
                .find(|line| line.starts_with("num_glyphs "))
                .unwrap(),
            format!("num_glyphs {}", font.num_glyphs()),
            "{name}"
        );
        let cmap = Cmap::new(font.table(Tag::new(b"cmap")).unwrap()).unwrap();
        let mut maps = Vec::new();
        let mut compared = 0;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("cmap ") {
                let parts = rest.split_ascii_whitespace().collect::<Vec<_>>();
                let platform = parts[0].parse::<u16>().unwrap();
                let encoding = parts[1].parse::<u16>().unwrap();
                let subtable = cmap.find(platform, encoding).unwrap().unwrap();
                assert_eq!(
                    subtable.format(),
                    parts[2].parse::<u16>().unwrap(),
                    "{name}"
                );
                maps.push((subtable, BTreeMap::<u32, u32>::new()));
            } else if let Some(rest) = line.strip_prefix("map ") {
                let parts = rest.split_ascii_whitespace().collect::<Vec<_>>();
                let code = parts[0].parse::<u32>().unwrap();
                let glyph = parts[1].parse::<u32>().unwrap();
                maps.last_mut().unwrap().1.insert(code, glyph);
                compared += 1;
            }
        }
        for (subtable, reference_map) in maps {
            for code in 0..=0xffff {
                assert_eq!(
                    subtable.glyph(code).map(|glyph| glyph.0),
                    reference_map.get(&code).copied(),
                    "{name}: U+{code:04X}"
                );
            }
        }
        assert!(compared >= 1000, "{name}: only {compared} cmap mappings");
        let sbit = Command::new(if name.ends_with(".woff2") {
            std::env::var_os("SOFEL_FT_BROTLI_DUMP")
                .map(std::path::PathBuf::from)
                .unwrap()
        } else {
            paths::ft_dump()
        })
        .arg("--sbit")
        .arg(&path)
        .output()
        .unwrap();
        assert!(
            sbit.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&sbit.stderr)
        );
        let strikes =
            color::compare_strikes(name, 0, &font, &String::from_utf8(sbit.stdout).unwrap());
        assert!(strikes.ok, "{name}: {}", strikes.detail);
        for line in text
            .lines()
            .filter(|line| line.starts_with("advance ") && !name.ends_with(".dfont"))
        {
            let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
            let id = fields[1].parse::<u32>().unwrap();
            let value = fields[2].parse::<u32>().unwrap();
            assert_eq!(
                font.default_instance().advance(GlyphId(id)),
                value,
                "{name}: advance {id}"
            );
        }
    }
}

#[test]
fn adr0090_i3_webfont_table_bytes_match_freetype() {
    let mut results = Vec::new();
    for name in ["spleen-8x16.woff", "spleen-8x16.woff2"] {
        let path = font_path(name);
        let bytes = std::fs::read(&path).unwrap();
        let decoded = if name.ends_with(".woff2") {
            sofel_woff::decode_woff2(&bytes, 1 << 20).unwrap()
        } else {
            sofel_woff::decode_woff(&bytes, 1 << 20).unwrap()
        };
        if name.ends_with(".woff2") {
            assert_eq!(sofel_write::checksum(&decoded), 0xB1B0_AFBA, "{name}");
        }
        let font = Font::new(&decoded, 0).unwrap();
        if name.ends_with(".woff2") && std::env::var_os("SOFEL_FT_BROTLI_DUMP").is_none() {
            continue;
        }
        let dump = if name.ends_with(".woff2") {
            std::env::var_os("SOFEL_FT_BROTLI_DUMP")
                .map(std::path::PathBuf::from)
                .unwrap()
        } else {
            paths::ft_dump()
        };
        let output = Command::new(dump)
            .arg("--legacy-tables")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut compared = 0;
        for line in String::from_utf8(output.stdout).unwrap().lines() {
            let mut parts = line.split_ascii_whitespace();
            assert_eq!(parts.next(), Some("table"));
            let raw = u32::from_str_radix(parts.next().unwrap(), 16).unwrap();
            let tag = Tag::new(&raw.to_be_bytes());
            let ours = font.table(tag).unwrap();
            let mut actual = ours
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                .into_bytes();
            let mut expected = parts.next().unwrap().as_bytes().to_vec();
            if name.ends_with(".woff2") && raw == u32::from_be_bytes(*b"head") {
                // root ADR 0070 excludes this layout-dependent four-byte adjustment.
                assert!(actual.len() >= 24 && expected.len() >= 24);
                actual[16..24].fill(b'0');
                expected[16..24].fill(b'0');
            }
            results.push((format!("{name}/table/{raw:08x}"), expected == actual));
            compared += 1;
        }
        assert!(compared >= 5, "{name}: only {compared} tables");
    }
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-legacy.txt"
    ));
    let expected = expected
        .into_iter()
        .filter(|id| id.contains("/table/"))
        .collect();
    let problems = expect::judge(
        results.iter().map(|(id, passed)| (id.as_str(), *passed)),
        &expected,
    );
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-legacy.txt")
    );
}

fn kerning(path: &Path, attach: Option<&Path>) -> BTreeMap<(usize, usize), (i32, i32)> {
    let mut command = Command::new(paths::ft_dump());
    command.arg("--legacy-kerning").arg(path);
    if let Some(attach) = attach {
        command.arg(attach);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("kern "))
        .map(|line| {
            let parts: Vec<_> = line
                .split_ascii_whitespace()
                .skip(1)
                .map(|part| part.parse::<i32>().unwrap())
                .collect();
            ((parts[0] as usize, parts[1] as usize), (parts[2], parts[3]))
        })
        .collect()
}

#[test]
fn adr0090_i3_cmr10_all_afm_and_pfm_kerning_pairs() {
    let path = font_path("cmr10.pfb");
    let bytes = std::fs::read(&path).unwrap();
    let source = Type1Source::parse_pfb(&bytes, bytes.len() * 8).unwrap();
    let mut face = Type1Face::new(&source).unwrap();
    let face_dump = reference(&[], &path);
    assert_eq!(
        face_dump
            .lines()
            .find(|line| line.starts_with("num_glyphs "))
            .unwrap(),
        format!("num_glyphs {}", face.num_glyphs())
    );
    let mut maps = Vec::<(u16, u16, BTreeMap<u32, usize>)>::new();
    for line in face_dump.lines() {
        if let Some(rest) = line.strip_prefix("cmap ") {
            let fields = rest.split_ascii_whitespace().collect::<Vec<_>>();
            maps.push((
                fields[0].parse().unwrap(),
                fields[1].parse().unwrap(),
                BTreeMap::new(),
            ));
        } else if let Some(rest) = line.strip_prefix("map ") {
            let fields = rest.split_ascii_whitespace().collect::<Vec<_>>();
            maps.last_mut()
                .unwrap()
                .2
                .insert(fields[0].parse().unwrap(), fields[1].parse().unwrap());
        }
    }
    assert_eq!(maps.len(), 2);
    let mut cmap_results = Vec::new();
    for (platform, encoding, reference_map) in maps {
        let kind = match (platform, encoding) {
            (3, 1) => "unicode",
            (7, 2) => "custom",
            _ => panic!("unexpected cmap {platform}/{encoding}"),
        };
        for code in 0..=0xffff {
            let actual = if kind == "unicode" {
                face.unicode_glyph(code)
            } else {
                u8::try_from(code)
                    .ok()
                    .and_then(|value| face.nominal_glyph(value))
            };
            cmap_results.push((
                format!("cmr10/cmap/{kind}/{code}"),
                actual == reference_map.get(&code).copied(),
            ));
        }
    }
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-legacy.txt"
    ));
    let expected = expected
        .into_iter()
        .filter(|id| id.starts_with("cmr10/cmap/"))
        .collect();
    let problems = expect::judge(
        cmap_results
            .iter()
            .map(|(id, passed)| (id.as_str(), *passed)),
        &expected,
    );
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-legacy.txt")
    );
    for line in face_dump
        .lines()
        .filter(|line| line.starts_with("advance "))
    {
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        let id = fields[1].parse::<usize>().unwrap();
        let value = fields[2].parse::<i32>().unwrap();
        assert_eq!(
            face.glyph_metrics(id).unwrap().advance_x,
            value,
            "cmr10: advance {id}"
        );
    }
    let afm = std::fs::read(font_path("cmr10.afm")).unwrap();
    let pfm = std::fs::read(font_path("cmr10.pfm")).unwrap();
    for attach in [None, Some("cmr10.afm"), Some("cmr10.pfm")] {
        let attachment = attach.map(font_path);
        match attach {
            Some("cmr10.afm") => face.attach_afm(&afm).unwrap(),
            Some("cmr10.pfm") => face.attach_pfm(&pfm).unwrap(),
            _ => {}
        }
        let expected = kerning(&path, attachment.as_deref());
        assert_eq!(expected.len(), if attach.is_none() { 0 } else { 181 });
        for left in 0..face.num_glyphs() {
            for right in 0..face.num_glyphs() {
                assert_eq!(
                    face.kerning(left, right),
                    expected.get(&(left, right)).copied().unwrap_or((0, 0)),
                    "{attach:?}: {left} {right}"
                );
            }
        }
    }
}

#[test]
fn adr0090_i3_cmr10_unhinted_outline_commands() {
    let path = font_path("cmr10.pfb");
    let bytes = std::fs::read(&path).unwrap();
    let source = Type1Source::parse_pfb(&bytes, bytes.len() * 8).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let output = reference(&["--outline", "unscaled"], &path);
    let mut expected = BTreeMap::<usize, Vec<String>>::new();
    let mut current = None;
    for line in output.lines() {
        if let Some(id) = line.strip_prefix("glyph ") {
            current = Some(id.parse::<usize>().unwrap());
            expected.entry(current.unwrap()).or_default();
        } else if line.starts_with("M ")
            || line.starts_with("L ")
            || line.starts_with("Q ")
            || line.starts_with("C ")
            || line == "Z"
        {
            expected
                .get_mut(&current.unwrap())
                .unwrap()
                .push(line.to_string());
        }
    }
    assert_eq!(expected.len(), face.num_glyphs());
    let mut results = Vec::new();
    for glyph in 0..face.num_glyphs() {
        let mut actual = Vec::new();
        let result = draw_type1(
            &face,
            glyph,
            Scale::Unscaled,
            &mut FnPen(|c| {
                let text = match c {
                    PathCommand::MoveTo(p) => format!("M {} {}", p.x, p.y),
                    PathCommand::LineTo(p) => format!("L {} {}", p.x, p.y),
                    PathCommand::QuadTo(c, p) => format!("Q {} {} {} {}", c.x, c.y, p.x, p.y),
                    PathCommand::CubicTo(a, b, p) => {
                        format!("C {} {} {} {} {} {}", a.x, a.y, b.x, b.y, p.x, p.y)
                    }
                    PathCommand::Close => "Z".to_string(),
                };
                actual.push(text);
            }),
        );
        results.push((
            format!("cmr10/outline/{glyph}"),
            result.is_ok() && expected.get(&glyph) == Some(&actual),
        ));
    }
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-legacy.txt"
    ));
    let expected = expected
        .into_iter()
        .filter(|id| id.starts_with("cmr10/outline/"))
        .collect();
    let problems = expect::judge(
        results.iter().map(|(id, passed)| (id.as_str(), *passed)),
        &expected,
    );
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-legacy.txt")
    );
}
