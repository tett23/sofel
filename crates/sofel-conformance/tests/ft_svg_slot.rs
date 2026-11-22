//! Pinned FreeType loads a color SVG slot before any outline hinting.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic
)]

use std::process::Command;

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_outline::{HintMode, HintTarget, Outlines, Scratch};
use sofel_read::GlyphId;

fn table(bytes: &[u8], tag: &[u8; 4]) -> (usize, usize, usize) {
    let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    for i in 0..count {
        let record = 12 + i * 16;
        if &bytes[record..record + 4] == tag {
            let offset = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap());
            let len = u32::from_be_bytes(bytes[record + 12..record + 16].try_into().unwrap());
            return (
                record,
                usize::try_from(offset).unwrap(),
                usize::try_from(len).unwrap(),
            );
        }
    }
    panic!("missing table");
}

fn with_glyf(mut bytes: Vec<u8>, glyph: &[u8]) -> Vec<u8> {
    let (glyf_record, glyf_offset, glyf_len) = table(&bytes, b"glyf");
    let (_, loca_offset, _) = table(&bytes, b"loca");
    let (_, head_offset, _) = table(&bytes, b"head");
    assert_eq!(&bytes[head_offset + 50..head_offset + 52], &[0, 0]);
    assert_eq!(glyf_len, 42);
    assert_eq!(&bytes[loca_offset + 38..loca_offset + 42], &[0, 21, 0, 21]);
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let new_offset = bytes.len();
    bytes.extend_from_within(glyf_offset..glyf_offset + glyf_len);
    assert!(glyph.len().is_multiple_of(2));
    bytes.extend_from_slice(glyph);
    let total = glyf_len + glyph.len();
    bytes[glyf_record + 8..glyf_record + 12]
        .copy_from_slice(&u32::try_from(new_offset).unwrap().to_be_bytes());
    bytes[glyf_record + 12..glyf_record + 16]
        .copy_from_slice(&u32::try_from(total).unwrap().to_be_bytes());
    for gid in 20..=28 {
        let at = loca_offset + gid * 2;
        bytes[at..at + 2].copy_from_slice(&u16::try_from(total / 2).unwrap().to_be_bytes());
    }
    bytes
}

fn with_triangle(bytes: Vec<u8>) -> Vec<u8> {
    // Three on-curve points, no instructions, and an even byte length for short loca.
    let triangle = [
        0, 1, 0, 0, 0, 0, 0, 100, 0, 100, 0, 2, 0, 0, 1, 1, 1, 0, 0, 0, 100, 255, 156, 0, 0, 0, 0,
        0, 100, 0,
    ];
    with_glyf(bytes, &triangle)
}

fn reference(font: &std::path::Path, ppem: u16, engine: &str, target: &str, color: bool) -> String {
    let mut command = Command::new(paths::ft_dump());
    command.args(["--hint-slot", &ppem.to_string(), "19", engine, target]);
    if color {
        command.arg("--color");
    }
    let output = command.arg(font).output().unwrap();
    assert!(
        output.status.success(),
        "{engine}/{target}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn adr0064_i1_svg_slot_precedes_auto_and_native_outline() {
    let font = paths::testdata().join("extra-fonts/fonts/color-svg.ttf");
    paths::require(&paths::ft_dump(), "ft-dump").unwrap();
    for (engine, outline_advance) in [("auto", 1280), ("none", 1275), ("native", 1280)] {
        let output = Command::new(paths::ft_dump())
            .args(["--hint-slot", "16", "19", engine, "normal", "--color"])
            .arg(&font)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("format 1398163232\n"), "{engine}: {stdout}");
        assert!(stdout.contains("advance 1275 0\n"), "{engine}: {stdout}");
        assert!(
            stdout.contains("svg-document 784 19 19 1024\n"),
            "{engine}: {stdout}"
        );
        let outline = Command::new(paths::ft_dump())
            .args(["--hint-slot", "16", "19", engine, "normal"])
            .arg(&font)
            .output()
            .unwrap();
        assert!(
            outline.status.success(),
            "{engine}: {}",
            String::from_utf8_lossy(&outline.stderr)
        );
        assert_eq!(
            String::from_utf8(outline.stdout).unwrap(),
            format!("advance {outline_advance} 0\n")
        );
        let no_svg = Command::new(paths::ft_dump())
            .args([
                "--hint-slot",
                "16",
                "19",
                engine,
                "normal",
                "--color",
                "--no-svg",
            ])
            .arg(&font)
            .output()
            .unwrap();
        assert!(no_svg.status.success());
        assert_eq!(
            String::from_utf8(no_svg.stdout).unwrap(),
            format!("format 1869968492\nadvance {outline_advance} 0\n")
        );
    }
    let bytes = std::fs::read(&font).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    for (mode, advance) in [
        (HintMode::Auto, 1280),
        (HintMode::None, 1275),
        (HintMode::NativeV40, 1280),
    ] {
        let mut scratch = Scratch::default();
        let (_, actual) = outlines
            .load_with_hint(GlyphId(19), 16, mode, HintTarget::Normal, &mut scratch)
            .unwrap();
        assert_eq!(actual, advance, "{mode:?}");
        assert_eq!(scratch.points().len(), 0, "{mode:?}");
    }
}

#[test]
fn adr0064_i1_non_color_loads_triangle_instead_of_svg_for_all_targets() {
    let original_path = paths::testdata().join("extra-fonts/fonts/color-svg.ttf");
    let original = std::fs::read(&original_path).unwrap();
    let bytes = with_triangle(original);
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let path = std::env::temp_dir().join(format!("sofel-svg-triangle-{}.ttf", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    for ppem in [12, 16] {
        for (engine, mode) in [
            ("auto", HintMode::Auto),
            ("none", HintMode::None),
            ("v35", HintMode::NativeV35),
            ("v40", HintMode::NativeV40),
        ] {
            for (target, hint_target) in [
                ("normal", HintTarget::Normal),
                ("light", HintTarget::Light),
                ("mono", HintTarget::Mono),
                ("lcd", HintTarget::Lcd),
                ("lcd-v", HintTarget::VerticalLcd),
            ] {
                let mut scratch = Scratch::default();
                let (_, advance) = outlines
                    .load_with_hint(GlyphId(19), ppem, mode, hint_target, &mut scratch)
                    .unwrap();
                assert_eq!(scratch.points().len(), 3, "{ppem}/{engine}/{target}");
                let mut actual = format!("advance {advance} 0\n");
                for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                    actual.push_str(&format!("point {} {} {tag}\n", point.x, point.y));
                }
                for end in scratch.contour_ends() {
                    actual.push_str(&format!("end {end}\n"));
                }
                assert_eq!(
                    actual,
                    reference(&path, ppem, engine, target, false),
                    "{ppem}/{engine}/{target}"
                );
            }
        }
        let svg = reference(&path, ppem, "auto", "normal", true);
        assert_eq!(svg, reference(&original_path, ppem, "auto", "normal", true));
        assert!(svg.contains("format 1398163232\n"));
        assert!(svg.contains("svg-document 784 19 19 1024\n"));
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_color_svg_loads_when_fallback_outline_is_broken() {
    let original_path = paths::testdata().join("extra-fonts/fonts/color-svg.ttf");
    let original = std::fs::read(&original_path).unwrap();
    // One contour claims 1001 points, but this 20-byte glyf record has no flags.
    let broken = [
        0, 1, 0, 0, 0, 0, 0, 100, 0, 100, 3, 232, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let bytes = with_glyf(original, &broken);
    let path = std::env::temp_dir().join(format!("sofel-svg-broken-{}.ttf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let svg = reference(&path, 16, "auto", "normal", true);
    assert_eq!(svg, reference(&original_path, 16, "auto", "normal", true));
    let no_svg = Command::new(paths::ft_dump())
        .args([
            "--hint-slot",
            "16",
            "19",
            "auto",
            "normal",
            "--color",
            "--no-svg",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(no_svg.status.code(), Some(1));
    assert!(
        String::from_utf8(no_svg.stderr)
            .unwrap()
            .contains("hint slot error 20")
    );
    std::fs::remove_file(path).unwrap();
}
