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
                .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
                .with_axis_pinned(Tag::new(b"wght"), value, &font)
                .unwrap();
            let ours = subset(&font, &input).unwrap();
            assert!(
                Command::new(paths::hb_dump())
                    .arg("--subset")
                    .arg(format!("--font-file={}", path.display()))
                    .arg("--unicodes=*")
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

#[test]
fn adr0061_i1_substitute_features_at_pinned_coordinates() {
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/MPLUS1-Variable.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let out = std::env::temp_dir().join(format!("sofel-feature-pin-{}.ttf", std::process::id()));
    for value in [100.0, 400.0, 900.0] {
        let input = Input::new()
            .map_set(SetKind::Unicodes, |s| s.add(0x30dd))
            .with_axis_pinned(Tag::new(b"wght"), value, &font)
            .unwrap();
        let ours = subset(&font, &input).unwrap();
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--unicodes=U+30DD")
                .arg(format!("--variations=wght={value}"))
                .arg(format!("--output-file={}", out.display()))
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(ours, std::fs::read(&out).unwrap(), "{value}");
    }
    std::fs::remove_file(out).unwrap();
}

#[test]
fn adr0061_i1_two_pins_keep_the_remaining_gvar_axis() {
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/Handjet.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let output =
        std::env::temp_dir().join(format!("sofel-gvar-two-pins-{}.ttf", std::process::id()));
    for (weight, grid) in [(400.0, 1.0), (500.0, 1.5), (900.0, 2.0)] {
        let input = Input::new()
            .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
            .map_set(SetKind::DropTables, |s| {
                for tag in [b"GDEF", b"GPOS", b"GSUB", b"HVAR", b"STAT"] {
                    s.add(Tag::new(tag).to_u32());
                }
            })
            .with_axis_pinned(Tag::new(b"wght"), weight, &font)
            .unwrap()
            .with_axis_pinned(Tag::new(b"ELGR"), grid, &font)
            .unwrap();
        let actual = subset(&font, &input).unwrap();
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--unicodes=*")
                .arg("--drop-tables+=GDEF,GPOS,GSUB,HVAR,STAT")
                .arg(format!("--variations=wght={weight},ELGR={grid}"))
                .arg(format!("--output-file={}", output.display()))
                .status()
                .unwrap()
                .success()
        );
        let expected = std::fs::read(&output).unwrap();
        assert_eq!(
            sofel_conformance::subset::diff(&expected, &actual),
            sofel_conformance::subset::diff(&expected, &expected)
        );
        assert_eq!(actual, expected);
    }
    std::fs::remove_file(output).unwrap();
}

#[test]
fn adr0061_i1_restricted_gvar_axis_matches_harfbuzz() {
    for name in [
        "Handjet.ttf",
        "Fraunces.ttf",
        "Roboto-Variable.ABC.ttf",
        "NotoSans-VF.abc.ttf",
    ] {
        let path = paths::testdata()
            .join("harfbuzz/test/subset/data/fonts")
            .join(name);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        let drops = if name == "NotoSans-VF.abc.ttf" {
            ""
        } else {
            "GDEF,GPOS,GSUB,STAT"
        };
        let output =
            std::env::temp_dir().join(format!("sofel-gvar-range-{}.ttf", std::process::id()));
        for (min, default, max) in [(200.0, 400.0, 650.0), (450.0, 500.0, 700.0)] {
            let input = Input::new()
                .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
                .map_set(SetKind::DropTables, |s| {
                    for tag in drops.split_terminator(',') {
                        s.add(Tag::new(tag.as_bytes().try_into().unwrap()).to_u32());
                    }
                })
                .with_axis_range(
                    Tag::new(b"wght"),
                    Some(min),
                    Some(max),
                    Some(default),
                    &font,
                )
                .unwrap();
            let actual = subset(&font, &input).unwrap();
            assert!(
                Command::new(paths::hb_dump())
                    .arg("--subset")
                    .arg(format!("--font-file={}", path.display()))
                    .arg("--unicodes=*")
                    .args((!drops.is_empty()).then(|| format!("--drop-tables+={drops}")))
                    .arg(format!("--variations=wght={min}:{default}:{max}"))
                    .arg(format!("--output-file={}", output.display()))
                    .status()
                    .unwrap()
                    .success()
            );
            let expected = std::fs::read(&output).unwrap();
            assert!(
                actual == expected,
                "{name}: {}",
                sofel_conformance::subset::diff(&expected, &actual)
            );
        }
        std::fs::remove_file(output).unwrap();
    }
}

#[test]
fn adr0061_i1_implicit_advances_survive_a_changed_default_and_gid_gaps() {
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/ImplicitHVAR.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let out = std::env::temp_dir().join(format!(
        "sofel-implicit-advances-{}.ttf",
        std::process::id()
    ));
    for (min, default, max) in [(450.0, 500.0, 700.0), (100.0, 300.0, 300.0)] {
        for retain in [false, true] {
            let input = Input::new()
                .with_flags(if retain {
                    Flags::RETAIN_GIDS
                } else {
                    Flags::DEFAULT
                })
                .map_set(SetKind::Unicodes, |s| {
                    s.add(65);
                    s.add(67);
                })
                .with_axis_range(
                    Tag::new(b"wght"),
                    Some(min),
                    Some(max),
                    Some(default),
                    &font,
                )
                .unwrap();
            let actual = subset(&font, &input).unwrap();
            let mut cmd = Command::new(paths::hb_dump());
            cmd.arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--unicodes=41,43")
                .arg(format!("--variations=wght={min}:{default}:{max}"))
                .arg(format!("--output-file={}", out.display()));
            if retain {
                cmd.arg("--retain-gids");
            }
            assert!(cmd.status().unwrap().success());
            assert_eq!(actual, std::fs::read(&out).unwrap());
        }
    }
    std::fs::remove_file(out).unwrap();
}

#[test]
fn adr0061_i1_iup_optimized_partial_instance_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/NotoSans-VF.abc.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    for ranges in [
        &[(b"wght", 300u16, 600u16)][..],
        &[(b"wght", 500, 600)][..],
        &[(b"wght", 200, 600), (b"wdth", 80, 90), (b"CTGR", 20, 60)][..],
    ] {
        let mut input = Input::new()
            .with_flags(Flags::OPTIMIZE_IUP_DELTAS)
            .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff));
        let mut arguments = Vec::new();
        for &(tag, minimum, maximum) in ranges {
            input = input
                .with_axis_range(
                    Tag::new(tag),
                    Some(f32::from(minimum)),
                    Some(f32::from(maximum)),
                    None,
                    &font,
                )
                .unwrap();
            arguments.push(format!(
                "{}={minimum}:{maximum}",
                std::str::from_utf8(tag).unwrap()
            ));
        }
        let actual = subset(&font, &input).unwrap();
        let output =
            std::env::temp_dir().join(format!("sofel-iup-range-{}.ttf", std::process::id()));
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .args(["--unicodes=*", "--optimize"])
                .arg(format!("--variations={}", arguments.join(",")))
                .arg(format!("--output-file={}", output.display()))
                .status()
                .unwrap()
                .success()
        );
        let expected = std::fs::read(&output).unwrap();
        std::fs::remove_file(output).unwrap();
        assert!(
            actual == expected,
            "{}",
            sofel_conformance::subset::diff(&expected, &actual)
        );
    }
}

#[test]
fn adr0061_i1_partial_layout_matches_harfbuzz() {
    for (name, axis, min, default, max) in [
        ("AnekBangla-subset.ttf", b"wght", 300u16, 300u16, 300u16),
        ("NotoSansOriya-subset.ttf", b"wdth", 75, 75, 75),
        ("NotoSansOriya-subset.ttf", b"wght", 300, 300, 300),
        ("NotoSansOriya-subset.ttf", b"wght", 900, 900, 900),
        ("NotoSansOriya-subset.ttf", b"wght", 200, 400, 850),
    ] {
        let path = paths::testdata()
            .join("harfbuzz/test/subset/data/fonts")
            .join(name);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let input = Input::new()
            .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
            .with_axis_range(
                Tag::new(axis),
                Some(f32::from(min)),
                Some(f32::from(max)),
                Some(f32::from(default)),
                &font,
            )
            .unwrap();
        let actual = subset(&font, &input).unwrap();
        let output =
            std::env::temp_dir().join(format!("sofel-gdef-pin-{}.ttf", std::process::id()));
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--unicodes=*")
                .arg(format!(
                    "--variations={}={min}:{default}:{max}",
                    std::str::from_utf8(axis).unwrap()
                ))
                .arg(format!("--output-file={}", output.display()))
                .status()
                .unwrap()
                .success()
        );
        let expected = std::fs::read(&output).unwrap();
        std::fs::remove_file(output).unwrap();
        assert!(
            expected == actual,
            "{}",
            sofel_conformance::subset::diff(&expected, &actual)
        );
    }
}
