//! HarfBuzz byte equality for VARC records without auxiliary data.

#![allow(clippy::all)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_read::{GlyphId, Tag, varc::Varc};
use sofel_subset::{Flags, Input, SetKind, subset};
use std::process::Command;

#[test]
fn adr0067_i2_short_cff2_reassigned_and_retained_gids() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-short-cff2.otf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    for retain in [false, true] {
        let flags = if retain {
            Flags::RETAIN_GIDS
        } else {
            Flags::DEFAULT
        };
        let input = Input::new()
            .with_flags(flags)
            .map_set(SetKind::Glyphs, |glyphs| glyphs.add(3));
        let ours = subset(&font, &input).unwrap();
        let reference =
            std::env::temp_dir().join(format!("sofel-varc-{retain}-{}-hb.otf", std::process::id()));
        let mut command = Command::new(paths::hb_dump());
        command
            .arg("--subset")
            .arg(format!("--font-file={}", path.display()))
            .arg("--gids=3")
            .arg(format!("--output-file={}", reference.display()));
        if retain {
            command.arg("--retain-gids");
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        let expected = std::fs::read(&reference).unwrap();
        let _ = std::fs::remove_file(&reference);
        assert_eq!(ours, expected, "retain={retain}");
    }
}

#[test]
fn adr0067_i2_duplicate_component_reference_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-short-cff2.otf");
    let data = std::fs::read(path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let source = Varc::new(font.table(Tag::new(b"VARC")).unwrap()).unwrap();
    let component = source.record(GlyphId(3)).unwrap();
    let mut varc = vec![0u8; 24];
    varc[..4].copy_from_slice(&[0, 1, 0, 0]);
    varc[20..24].copy_from_slice(&24u32.to_be_bytes());
    varc.extend_from_slice(&1u32.to_be_bytes()); // one CFF2 INDEX record
    varc.push(1); // one-byte offsets
    varc.extend_from_slice(&[1, u8::try_from(component.len() * 2 + 1).unwrap()]);
    varc.extend_from_slice(component);
    varc.extend_from_slice(component);
    let coverage_offset = u32::try_from(varc.len()).unwrap();
    varc[4..8].copy_from_slice(&coverage_offset.to_be_bytes());
    varc.extend_from_slice(&[0, 1, 0, 1, 0, 3]);
    let tables: Vec<_> = font
        .sfnt()
        .records()
        .map(|record| {
            let bytes = if record.tag == Tag::new(b"VARC") {
                varc.as_slice()
            } else {
                font.table(record.tag).unwrap()
            };
            (record.tag, bytes)
        })
        .collect();
    let synthetic = sofel_write::build_sfnt(&tables).unwrap();
    let font = Font::new(&synthetic, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| glyphs.add(3));
    let ours = subset(&font, &input).unwrap();
    let input_file =
        std::env::temp_dir().join(format!("sofel-varc-duplicate-{}.otf", std::process::id()));
    let expected_file = std::env::temp_dir().join(format!(
        "sofel-varc-duplicate-hb-{}.otf",
        std::process::id()
    ));
    std::fs::write(&input_file, &synthetic).unwrap();
    let result = Command::new(paths::hb_dump())
        .arg("--subset")
        .arg(format!("--font-file={}", input_file.display()))
        .arg("--gids=3")
        .arg(format!("--output-file={}", expected_file.display()))
        .output()
        .unwrap();
    assert!(result.status.success());
    let expected = std::fs::read(&expected_file).unwrap();
    let _ = std::fs::remove_file(input_file);
    let _ = std::fs::remove_file(expected_file);
    assert_eq!(ours, expected);
}
