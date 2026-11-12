//! ADR 0061 I1: gvar sources with no glyph deltas still support axis instancing.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{sample_tables, sfnt};
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_subset::{Flags, Input, SetKind, subset};
use std::process::Command;

fn inert_gvar_font() -> Vec<u8> {
    let mut tables = sample_tables();
    let glyph_count = Font::new(&sfnt(&tables), 0).unwrap().num_glyphs();
    let mut fvar = vec![0, 1, 0, 0];
    fvar.extend(16u16.to_be_bytes());
    fvar.extend(2u16.to_be_bytes());
    fvar.extend(1u16.to_be_bytes());
    fvar.extend(20u16.to_be_bytes());
    fvar.extend(0u16.to_be_bytes());
    fvar.extend(8u16.to_be_bytes());
    fvar.extend(b"wght");
    for value in [100i32, 400, 900] {
        fvar.extend((value << 16).to_be_bytes());
    }
    fvar.extend(0u16.to_be_bytes());
    fvar.extend(256u16.to_be_bytes());
    let mut gvar = vec![0, 1, 0, 0];
    gvar.extend(1u16.to_be_bytes());
    gvar.extend(0u16.to_be_bytes());
    gvar.extend(0u32.to_be_bytes());
    gvar.extend(u16::try_from(glyph_count).unwrap().to_be_bytes());
    gvar.extend(0u16.to_be_bytes());
    gvar.extend((20 + (glyph_count + 1) * 2).to_be_bytes());
    gvar.extend(vec![0; usize::try_from((glyph_count + 1) * 2).unwrap()]);
    tables.push((b"fvar", fvar));
    tables.push((b"gvar", gvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn adr0061_i1_inert_gvar_nondefault_pin_and_range_match_harfbuzz() {
    let source = std::env::temp_dir().join(format!("sofel-inert-gvar-{}.ttf", std::process::id()));
    let output =
        std::env::temp_dir().join(format!("sofel-inert-gvar-out-{}.ttf", std::process::id()));
    let data = inert_gvar_font();
    std::fs::write(&source, &data).unwrap();
    let font = Font::new(&data, 0).unwrap();
    for retain in [false, true] {
        let base = Input::new()
            .with_flags(if retain {
                Flags::RETAIN_GIDS
            } else {
                Flags::DEFAULT
            })
            .map_set(SetKind::Glyphs, |set| set.add(1));
        for (variation, input) in [
            (
                "wght=650",
                base.with_axis_pinned(Tag::new(b"wght"), 650.0, &font)
                    .unwrap(),
            ),
            (
                "wght=200:650:800",
                base.with_axis_range(
                    Tag::new(b"wght"),
                    Some(200.0),
                    Some(800.0),
                    Some(650.0),
                    &font,
                )
                .unwrap(),
            ),
        ] {
            let ours = subset(&font, &input).unwrap();
            let mut reference = Command::new(paths::hb_dump());
            reference
                .arg("--subset")
                .arg(format!("--font-file={}", source.display()))
                .arg("--gids=1")
                .arg(format!("--variations={variation}"))
                .arg(format!("--output-file={}", output.display()));
            if retain {
                reference.arg("--retain-gids");
            }
            assert!(reference.status().unwrap().success());
            assert_eq!(
                ours,
                std::fs::read(&output).unwrap(),
                "{variation}, retain={retain}"
            );
        }
    }
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(output);
}

#[test]
fn adr0061_i1_simple_gvar_full_pin_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/glyf-default-instance-bounds.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let output =
        std::env::temp_dir().join(format!("sofel-gvar-full-pin-{}.ttf", std::process::id()));
    for retain in [false, true] {
        for value in [0.25, 0.5, 1.0] {
            let input = Input::new()
                .with_flags(if retain {
                    Flags::RETAIN_GIDS
                } else {
                    Flags::DEFAULT
                })
                .map_set(SetKind::Glyphs, |set| {
                    set.add(1);
                })
                .with_axis_pinned(Tag::new(b"wght"), value, &font)
                .unwrap()
                .with_axis_pinned(Tag::new(b"wdth"), value, &font)
                .unwrap();
            let ours = subset(&font, &input).unwrap();
            let mut command = Command::new(paths::hb_dump());
            command
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--gids=1")
                .arg(format!("--variations=wght={value},wdth={value}"))
                .arg(format!("--output-file={}", output.display()));
            if retain {
                command.arg("--retain-gids");
            }
            assert!(command.status().unwrap().success());
            assert_eq!(
                ours,
                std::fs::read(&output).unwrap(),
                "value={value}, retain={retain}"
            );
        }
    }
    let _ = std::fs::remove_file(output);
}

#[test]
fn adr0061_i1_nondefault_gvar_composite_remains_unsupported() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/glyf-default-instance-bounds.ttf");
    let data = std::fs::read(path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |set| set.add(2))
        .with_axis_pinned(Tag::new(b"wght"), 0.5, &font)
        .unwrap()
        .with_axis_pinned(Tag::new(b"wdth"), 0.5, &font)
        .unwrap();
    assert!(matches!(
        subset(&font, &input),
        Err(sofel_subset::SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"gvar")
    ));
}

#[test]
fn adr0061_i1_axis_pin_projects_roboto_mixed_tuples() {
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/Roboto-Variable.ABC.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let dest =
        std::env::temp_dir().join(format!("sofel-mixed-gvar-pin-{}.ttf", std::process::id()));
    let base = Input::new()
        .map_set(SetKind::Unicodes, |s| s.add(0x41))
        .map_set(SetKind::DropTables, |s| {
            for tag in [b"HVAR", b"STAT", b"GDEF", b"GSUB", b"GPOS", b"avar"] {
                s.add(Tag::new(tag).to_u32());
            }
        });
    for (axis, value, variation) in [
        (b"wght", 400.0, "wght=400"),
        (b"wdth", 100.0, "wdth=100"),
        (b"wght", 900.0, "wght=900"),
        (b"wght", 650.0, "wght=650"),
        (b"wght", 250.0, "wght=250"),
        (b"wdth", 75.0, "wdth=75"),
    ] {
        let input = base.with_axis_pinned(Tag::new(axis), value, &font).unwrap();
        let ours = subset(&font, &input).unwrap();
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--unicodes=U+0041")
                .arg("--drop-tables+=HVAR,STAT,GDEF,GSUB,GPOS,avar")
                .arg(format!("--variations={variation}"))
                .arg(format!("--output-file={}", dest.display()))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(ours, std::fs::read(&dest).unwrap(), "{variation}");
    }
    std::fs::remove_file(dest).unwrap();
}

#[test]
fn adr0061_i1_pinning_a_mixed_tuple_scales_the_retained_axis() {
    compare_mixed_tuple_pin(false);
}

#[test]
fn adr0061_i1_merge_projected_gvar_tuples_before_rounding() {
    compare_mixed_tuple_pin(true);
}

fn compare_mixed_tuple_pin(duplicate: bool) {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/glyf-default-instance-bounds.ttf");
    let mut bytes = std::fs::read(path).unwrap();
    // The fixture has one shared peak (1, 0). Give it support on both axes.
    let record = bytes[12..]
        .chunks_exact(16)
        .find(|r| &r[..4] == b"gvar")
        .unwrap();
    let table = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
    let shared = u32::from_be_bytes(bytes[table + 8..table + 12].try_into().unwrap()) as usize;
    bytes[table + shared + 2..table + shared + 4].copy_from_slice(&16384i16.to_be_bytes());
    if duplicate {
        // Replace glyph 1 with two identical mixed tuples sharing all points.
        let pos = table + 32;
        let mut record = vec![0x80, 2, 0, 12];
        record.extend_from_slice(&bytes[pos + 4..pos + 8]);
        record.extend_from_slice(&bytes[pos + 4..pos + 8]);
        record.push(0);
        record.extend_from_slice(&bytes[pos + 9..pos + 14]);
        record.extend_from_slice(&bytes[pos + 9..pos + 14]);
        record.push(0);
        bytes.splice(pos..pos + 14, record);
        bytes[table + 24..table + 28].copy_from_slice(&[0, 12, 0, 19]);
        let index = bytes[12..]
            .chunks_exact(16)
            .position(|r| &r[..4] == b"gvar")
            .unwrap();
        let len = 12 + index * 16 + 12;
        bytes[len..len + 4].copy_from_slice(&70u32.to_be_bytes());
    }
    let font = Font::new(&bytes, 0).unwrap();
    let source = std::env::temp_dir().join(format!(
        "sofel-mixed-peak-{duplicate}-{}.ttf",
        std::process::id()
    ));
    let output = source.with_extension("out.ttf");
    std::fs::write(&source, &bytes).unwrap();
    for value in [0.25, 0.5, 0.75, 1.0] {
        let input = Input::new()
            .map_set(SetKind::Glyphs, |s| s.add(1))
            .with_axis_pinned(Tag::new(b"wdth"), value, &font)
            .unwrap();
        let ours = subset(&font, &input).unwrap();
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", source.display()))
                .arg("--gids=1")
                .arg(format!("--variations=wdth={value}"))
                .arg(format!("--output-file={}", output.display()))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(ours, std::fs::read(&output).unwrap());
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(output).unwrap();
}
