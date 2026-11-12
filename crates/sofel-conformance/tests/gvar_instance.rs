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
fn adr0061_i1_simple_and_composite_gvar_full_pin_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/glyf-default-instance-bounds.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let output =
        std::env::temp_dir().join(format!("sofel-gvar-full-pin-{}.ttf", std::process::id()));
    for retain in [false, true] {
        for gid in [1, 2] {
            for value in [0.25, 0.5, 1.0] {
                let input = Input::new()
                    .with_flags(if retain {
                        Flags::RETAIN_GIDS
                    } else {
                        Flags::DEFAULT
                    })
                    .map_set(SetKind::Glyphs, |set| {
                        set.add(gid);
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
                    .arg(format!("--gids={gid}"))
                    .arg(format!("--variations=wght={value},wdth={value}"))
                    .arg(format!("--output-file={}", output.display()));
                if retain {
                    command.arg("--retain-gids");
                }
                assert!(command.status().unwrap().success());
                assert_eq!(
                    ours,
                    std::fs::read(&output).unwrap(),
                    "gid={gid}, value={value}, retain={retain}"
                );
            }
        }
    }
    let _ = std::fs::remove_file(output);
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
    compare_mixed_tuple_pin(false, false);
}

#[test]
fn adr0061_i1_merge_projected_gvar_tuples_before_rounding() {
    compare_mixed_tuple_pin(true, false);
}

#[test]
fn adr0061_i1_expand_sparse_gvar_deltas_before_serializing() {
    compare_mixed_tuple_pin(false, true);
}

fn table_entry(data: &[u8], tag: &[u8; 4]) -> usize {
    let count = sofel_font::read::be::u16_at(data, 4).unwrap();
    (0..usize::from(count))
        .map(|index| 12 + 16 * index)
        .find(|&at| data.get(at..at + 4) == Some(tag))
        .unwrap()
}

fn table_offset(data: &[u8], entry: usize) -> usize {
    sofel_font::read::be::u32_at(data, entry + 8).unwrap() as usize
}

fn compare_mixed_tuple_pin(duplicate: bool, sparse: bool) {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/glyf-default-instance-bounds.ttf");
    let mut bytes = std::fs::read(path).unwrap();
    // The fixture has one shared peak (1, 0). Give it support on both axes.
    let entry = table_entry(&bytes, b"gvar");
    let table = table_offset(&bytes, entry);
    let shared = sofel_font::read::be::u32_at(&bytes, table + 8).unwrap() as usize;
    bytes
        .get_mut(table + shared + 2..table + shared + 4)
        .unwrap()
        .copy_from_slice(&16384i16.to_be_bytes());
    if sparse {
        // Glyph 1 has one reference at point zero, with X delta 8.
        bytes
            .get_mut(table + 32..table + 46)
            .unwrap()
            .copy_from_slice(&[0x80, 1, 0, 8, 0, 3, 0, 0, 1, 0, 0, 0, 8, 0x80]);
    }
    if duplicate {
        // Replace glyph 1 with two identical mixed tuples sharing all points.
        let pos = table + 32;
        let mut record = vec![0x80, 2, 0, 12];
        record.extend_from_slice(bytes.get(pos + 4..pos + 8).unwrap());
        record.extend_from_slice(bytes.get(pos + 4..pos + 8).unwrap());
        record.push(0);
        record.extend_from_slice(bytes.get(pos + 9..pos + 14).unwrap());
        record.extend_from_slice(bytes.get(pos + 9..pos + 14).unwrap());
        record.push(0);
        bytes.splice(pos..pos + 14, record);
        bytes
            .get_mut(table + 24..table + 28)
            .unwrap()
            .copy_from_slice(&[0, 12, 0, 19]);
        let len = entry + 12;
        bytes
            .get_mut(len..len + 4)
            .unwrap()
            .copy_from_slice(&70u32.to_be_bytes());
    }
    let font = Font::new(&bytes, 0).unwrap();
    let source = std::env::temp_dir().join(format!(
        "sofel-mixed-peak-{duplicate}-{sparse}-{}.ttf",
        std::process::id()
    ));
    let output = source.with_extension("out.ttf");
    std::fs::write(&source, &bytes).unwrap();
    for gid in [1, 2] {
        for value in [0.25, 0.5, 0.75, 1.0] {
            let input = Input::new()
                .map_set(SetKind::Glyphs, |s| s.add(gid))
                .with_axis_pinned(Tag::new(b"wdth"), value, &font)
                .unwrap();
            let ours = subset(&font, &input).unwrap();
            assert!(
                Command::new(paths::hb_dump())
                    .arg("--subset")
                    .arg(format!("--font-file={}", source.display()))
                    .arg(format!("--gids={gid}"))
                    .arg(format!("--variations=wdth={value}"))
                    .arg(format!("--output-file={}", output.display()))
                    .status()
                    .unwrap()
                    .success()
            );
            assert_eq!(ours, std::fs::read(&output).unwrap());
        }
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(output).unwrap();
}

#[test]
fn adr0061_i1_empty_glyph_keeps_instanced_phantom_metrics() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/glyf-default-instance-bounds.ttf");
    let mut data = std::fs::read(path).unwrap();
    let loca = table_offset(&data, table_entry(&data, b"loca"));
    let gvar = table_offset(&data, table_entry(&data, b"gvar"));
    // Give glyph 1 no outline and a +20 X delta on its right phantom point.
    data.copy_within(loca + 2..loca + 4, loca + 4);
    data.get_mut(gvar + 32..gvar + 46)
        .unwrap()
        .copy_from_slice(&[0x80, 1, 0, 8, 0, 5, 0, 0, 0, 0x80, 0, 20, 0x81, 0x83]);
    let font = Font::new(&data, 0).unwrap();
    let src = std::env::temp_dir().join(format!("sofel-empty-phantom-{}.ttf", std::process::id()));
    let out = src.with_extension("out.ttf");
    std::fs::write(&src, &data).unwrap();
    let gid = sofel_font::read::GlyphId(1);
    let advance = font.default_instance().advance(gid);
    for (value, delta) in [(0.25, 5), (0.5, 10), (1.0, 20)] {
        let input = Input::new()
            .map_set(SetKind::Glyphs, |s| s.add(1))
            .with_axis_pinned(Tag::new(b"wght"), value, &font)
            .unwrap()
            .with_axis_pinned(Tag::new(b"wdth"), 0.0, &font)
            .unwrap();
        let ours = subset(&font, &input).unwrap();
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", src.display()))
                .arg("--gids=1")
                .arg(format!("--variations=wght={value},wdth=0"))
                .arg(format!("--output-file={}", out.display()))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(ours, std::fs::read(&out).unwrap());
        assert_eq!(
            Font::new(&ours, 0).unwrap().default_instance().advance(gid),
            advance + delta
        );
    }
    std::fs::remove_file(src).unwrap();
    std::fs::remove_file(out).unwrap();
}

#[test]
fn adr0061_i1_pinning_cvar_updates_control_values() {
    for name in ["Comfortaa-Regular-new.ttf", "Muli-ABC.ttf"] {
        let path = paths::testdata()
            .join("harfbuzz/test/subset/data/fonts")
            .join(name);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let out = std::env::temp_dir().join(format!("sofel-cvar-pin-{}.ttf", std::process::id()));
        for value in [300.0, 700.0] {
            let input = Input::new()
                .map_set(SetKind::Unicodes, |s| s.add(65))
                .map_set(SetKind::DropTables, |s| {
                    for t in [b"GDEF", b"GPOS", b"GSUB"] {
                        s.add(Tag::new(t).to_u32());
                    }
                })
                .with_axis_pinned(Tag::new(b"wght"), value, &font)
                .unwrap();
            let ours = subset(&font, &input).unwrap();
            assert!(
                Command::new(paths::hb_dump())
                    .arg("--subset")
                    .arg(format!("--font-file={}", path.display()))
                    .arg("--unicodes=U+0041")
                    .arg("--drop-tables+=GDEF,GPOS,GSUB")
                    .arg(format!("--variations=wght={value}"))
                    .arg(format!("--output-file={}", out.display()))
                    .status()
                    .unwrap()
                    .success()
            );
            let reference = std::fs::read(&out).unwrap();
            assert_eq!(
                Font::new(&ours, 0).unwrap().table(Tag::new(b"cvt ")),
                Font::new(&reference, 0).unwrap().table(Tag::new(b"cvt "))
            );
            let actual = Font::new(&ours, 0).unwrap();
            let expected = Font::new(&reference, 0).unwrap();
            let gid = actual.nominal_glyph(65).unwrap();
            assert_eq!(
                actual.default_instance().advance(gid),
                expected.default_instance().advance(gid)
            );
            assert_eq!(ours, reference);
        }
        std::fs::remove_file(out).unwrap();
    }
}
