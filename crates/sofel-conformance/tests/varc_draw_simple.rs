//! Bounded VARC drawing with component-only records (sofel ADR 0067).

#![allow(clippy::all)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::indexing_slicing)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::var::Location;
use sofel_geom::{Command, FnPen};
use sofel_outline::{CBox, HbOutlineCommand, Outlines, Scale, Scratch};
use sofel_read::{GlyphId, Tag, varc::Varc};
use sofel_subset::{Flags, Input, SetKind, subset};
use std::process::Command as ProcessCommand;

fn source() -> Vec<u8> {
    std::fs::read(paths::testdata().join("harfbuzz/test/api/fonts/varc-short-cff2.otf")).unwrap()
}

fn synthetic(records: &[(u16, Vec<u8>)]) -> Vec<u8> {
    synthetic_with_condition(records, None)
}

fn synthetic_with_condition(records: &[(u16, Vec<u8>)], condition: Option<(i16, i16)>) -> Vec<u8> {
    let data = source();
    let font = Font::new(&data, 0).unwrap();
    let mut maxp = font.table(Tag::new(b"maxp")).unwrap().to_vec();
    let highest = records.iter().map(|(glyph, _)| *glyph).max().unwrap_or(0);
    let glyph_count = u16::from_be_bytes([maxp[4], maxp[5]]).max(highest.checked_add(1).unwrap());
    maxp[4..6].copy_from_slice(&glyph_count.to_be_bytes());
    let mut varc = vec![0u8; 24];
    varc[..4].copy_from_slice(&[0, 1, 0, 0]);
    varc[20..24].copy_from_slice(&24u32.to_be_bytes());
    varc.extend_from_slice(&u32::try_from(records.len()).unwrap().to_be_bytes());
    let end_offset = records
        .iter()
        .try_fold(1usize, |offset, (_, record)| {
            offset.checked_add(record.len())
        })
        .unwrap();
    let offset_size = if end_offset <= u8::MAX as usize {
        1
    } else if end_offset <= u16::MAX as usize {
        2
    } else {
        3
    };
    varc.push(offset_size);
    let offset_start = 4usize.checked_sub(usize::from(offset_size)).unwrap();
    let mut offset = 1usize;
    varc.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes()[offset_start..]);
    for (_, record) in records {
        offset = offset.checked_add(record.len()).unwrap();
        varc.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes()[offset_start..]);
    }
    for (_, record) in records {
        varc.extend_from_slice(record);
    }
    let coverage = u32::try_from(varc.len()).unwrap();
    varc[4..8].copy_from_slice(&coverage.to_be_bytes());
    varc.extend_from_slice(&[0, 1]);
    varc.extend_from_slice(&u16::try_from(records.len()).unwrap().to_be_bytes());
    for (glyph, _) in records {
        varc.extend_from_slice(&glyph.to_be_bytes());
    }
    if let Some((min, max)) = condition {
        let offset = u32::try_from(varc.len()).unwrap();
        varc[12..16].copy_from_slice(&offset.to_be_bytes());
        varc.extend_from_slice(&1u32.to_be_bytes()); // one condition
        varc.extend_from_slice(&8u32.to_be_bytes()); // offset from the list
        varc.extend_from_slice(&1u16.to_be_bytes()); // axis-range format
        varc.extend_from_slice(&0u16.to_be_bytes()); // axis 0
        varc.extend_from_slice(&min.to_be_bytes());
        varc.extend_from_slice(&max.to_be_bytes());
    }
    let tables: Vec<_> = font
        .sfnt()
        .records()
        .map(|record| {
            let bytes = if record.tag == Tag::new(b"VARC") {
                varc.as_slice()
            } else if record.tag == Tag::new(b"maxp") {
                maxp.as_slice()
            } else {
                font.table(record.tag).unwrap()
            };
            (record.tag, bytes)
        })
        .collect();
    sofel_write::build_sfnt(&tables).unwrap()
}

#[test]
fn adr0067_i1_component_condition_at_boundary() {
    let data = source();
    let font = Font::new(&data, 0).unwrap();
    let mut record = Varc::new(font.table(Tag::new(b"VARC")).unwrap())
        .unwrap()
        .record(GlyphId(3))
        .unwrap()
        .to_vec();
    record[1] |= 0x80; // HAVE_CONDITION in the two-byte flags
    record.insert(4, 0); // condition-list entry 0
    let shown = synthetic_with_condition(&[(3, record.clone())], Some((0, 0)));
    let hidden = synthetic_with_condition(&[(3, record)], Some((1, 1)));
    assert!(
        !draw(&Font::new(&shown, 0).unwrap(), 3, &mut Scratch::default())
            .1
            .is_empty()
    );
    assert!(
        draw(&Font::new(&hidden, 0).unwrap(), 3, &mut Scratch::default())
            .1
            .is_empty()
    );
}

#[test]
fn adr0067_i1_component_axes_and_variations_draw_at_two_locations() {
    let data = std::fs::read(paths::testdata().join("harfbuzz/test/api/fonts/varc-ac00-ac01.ttf"))
        .unwrap();
    let font = Font::new(&data, 0).unwrap();
    let mut scratch = Scratch::default();
    let default = draw(&font, 3, &mut scratch).1;
    let location = Location::from_normalized_2_14(&font, &[16_384]);
    let outlines = Outlines::new(font.instance(&location));
    let mut heavy = Vec::new();
    outlines
        .draw(
            GlyphId(3),
            Scale::Unscaled,
            &mut scratch,
            &mut FnPen(|command| heavy.push(command)),
        )
        .unwrap();
    assert!(!default.is_empty());
    assert!(!heavy.is_empty());
    assert_ne!(default, heavy);
}

#[test]
fn adr0067_i2_condition_only_subset_matches_harfbuzz() {
    let data = source();
    let font = Font::new(&data, 0).unwrap();
    let mut record = Varc::new(font.table(Tag::new(b"VARC")).unwrap())
        .unwrap()
        .record(GlyphId(3))
        .unwrap()
        .to_vec();
    record[1] |= 0x80;
    record.insert(4, 0);
    let data = synthetic_with_condition(&[(3, record)], Some((0, 0)));
    let source_path = std::env::temp_dir().join(format!(
        "sofel-varc-condition-source-{}.otf",
        std::process::id()
    ));
    std::fs::write(&source_path, &data).unwrap();
    for retain in [false, true] {
        let reference = std::env::temp_dir().join(format!(
            "sofel-varc-condition-{retain}-hb-{}.otf",
            std::process::id()
        ));
        let mut command = ProcessCommand::new(paths::hb_dump());
        command
            .arg("--subset")
            .arg(format!("--font-file={}", source_path.display()))
            .arg("--gids=3")
            .arg(format!("--output-file={}", reference.display()));
        if retain {
            command.arg("--retain-gids");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let flags = if retain {
            Flags::RETAIN_GIDS
        } else {
            Flags::DEFAULT
        };
        let input = Input::new()
            .with_flags(flags)
            .map_set(SetKind::Glyphs, |glyphs| glyphs.add(3));
        let ours = subset(&Font::new(&data, 0).unwrap(), &input).unwrap();
        let expected = std::fs::read(&reference).unwrap();
        assert_eq!(ours, expected, "retain={retain}");
        let _ = std::fs::remove_file(reference);
    }
    let _ = std::fs::remove_file(source_path);
}

#[test]
fn adr0067_i2_real_varc_subset_matches_harfbuzz() {
    for (name, unicode) in [
        ("varc-ac00-ac01.ttf", 0xAC00),
        ("varc-ac00-ac01.ttf", 0xAC01),
        ("varc-ac01-conditional.ttf", 0xAC01),
        ("varc-no-variation-index.ttf", 0xAC00),
        ("varc-condition-no-variation-index.ttf", 0xAC01),
        ("varc-coverage-longer.ttf", 0xAC00),
        ("varc-records-longer.ttf", 0xAC00),
        ("varc-unrelated-axis.ttf", 0xAC01),
        ("varc-unrelated-axis-avar2.ttf", 0xAC01),
        ("varc-6868.ttf", 0x6868),
        ("varc-delta-precision.ttf", 0x0041),
        ("varc-delta-precision.ttf", 0x0042),
        ("varc-delta-precision.ttf", 0x0043),
        ("varc-static-deltas.ttf", 0x0041),
        ("varc-static-deltas.ttf", 0x0042),
        ("varc-static-deltas.ttf", 0x0043),
        ("varc-static-gvar.ttf", 0x0061),
    ] {
        let path = paths::testdata().join("harfbuzz/test/api/fonts").join(name);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        for retain in [false, true] {
            let reference = std::env::temp_dir().join(format!(
                "sofel-varc-real-{name}-{retain}-hb-{}.ttf",
                std::process::id()
            ));
            let mut command = ProcessCommand::new(paths::hb_dump());
            command
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg(format!("--unicodes=U+{unicode:04X}"))
                .arg(format!("--output-file={}", reference.display()));
            if retain {
                command.arg("--retain-gids");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{name} {retain}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let flags = if retain {
                Flags::RETAIN_GIDS
            } else {
                Flags::DEFAULT
            };
            let input = Input::new()
                .with_flags(flags)
                .map_set(SetKind::Unicodes, |unicodes| unicodes.add(unicode));
            let ours = subset(&font, &input);
            assert!(ours.is_ok(), "{name} retain={retain}: {ours:?}");
            let ours = ours.unwrap();
            let expected = std::fs::read(&reference).unwrap();
            assert!(
                ours == expected,
                "{name} retain={retain}: ours {} bytes, HarfBuzz {} bytes",
                ours.len(),
                expected.len()
            );
            let _ = std::fs::remove_file(reference);
        }
    }
}

fn draw(font: &Font<'_>, glyph: u32, scratch: &mut Scratch) -> (CBox, Vec<Command>) {
    let outlines = Outlines::new(font.default_instance());
    let mut commands = Vec::new();
    let info = outlines
        .draw(
            GlyphId(glyph),
            Scale::Unscaled,
            scratch,
            &mut FnPen(|command| commands.push(command)),
        )
        .unwrap();
    (info.cbox, commands)
}

#[test]
fn adr0067_i1_short_cff2_transform_matches_harfbuzz() {
    let data = source();
    let font = Font::new(&data, 0).unwrap();
    let (box_, commands) = draw(&font, 3, &mut Scratch::default());
    let moves: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            Command::MoveTo(point) => Some(*point),
            _ => None,
        })
        .collect();
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0].x, 600); // HarfBuzz test-draw-varc.c
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, Command::LineTo(_)))
            .count(),
        3
    );
    assert_eq!(
        box_,
        CBox {
            x_min: 600,
            y_min: 0,
            x_max: 800,
            y_max: 200
        }
    );
    assert_eq!(
        Outlines::new(font.default_instance()).glyph_extents(GlyphId(3)),
        Some(sofel_font::GlyphExtents {
            x_bearing: 600,
            y_bearing: 200,
            width: 200,
            height: -200,
        })
    );
}

#[test]
fn adr0067_i3_self_and_two_glyph_cycles_leave_next_glyph_clean() {
    let data = source();
    let font = Font::new(&data, 0).unwrap();
    let original = Varc::new(font.table(Tag::new(b"VARC")).unwrap())
        .unwrap()
        .record(GlyphId(3))
        .unwrap()
        .to_vec();
    let mut self_record = original.clone();
    self_record[2..4].copy_from_slice(&3u16.to_be_bytes());
    let self_data = synthetic(&[(3, self_record)]);
    let self_font = Font::new(&self_data, 0).unwrap();
    let mut scratch = Scratch::default();
    assert!(draw(&self_font, 3, &mut scratch).1.is_empty());
    let normal = draw(&self_font, 1, &mut scratch);
    assert_eq!(normal, draw(&self_font, 1, &mut Scratch::default()));

    let mut from_two = original.clone();
    from_two[2..4].copy_from_slice(&3u16.to_be_bytes());
    let mut from_three = original;
    from_three[2..4].copy_from_slice(&2u16.to_be_bytes());
    let cycle_data = synthetic(&[(2, from_two), (3, from_three)]);
    let cycle_font = Font::new(&cycle_data, 0).unwrap();
    assert!(draw(&cycle_font, 2, &mut scratch).1.is_empty());
    assert_eq!(
        draw(&cycle_font, 1, &mut scratch),
        draw(&cycle_font, 1, &mut Scratch::default())
    );
}

#[test]
fn adr0067_i3_shared_edge_budget_at_boundary() {
    let leaf = vec![0, 0, 1];
    let mut previous = 0;
    for count in [16_382usize, 16_383, 16_384] {
        let parent = [0, 0, 2].repeat(count);
        let data = synthetic(&[(2, leaf.clone()), (3, parent)]);
        let font = Font::new(&data, 0).unwrap();
        let mut scratch = Scratch::default();
        let moves = draw(&font, 3, &mut scratch)
            .1
            .iter()
            .filter(|command| matches!(command, Command::MoveTo(_)))
            .count();
        assert_eq!(moves, count.min(16_383), "{count} covered components");
        assert!(moves >= previous);
        previous = moves;
        assert_eq!(
            draw(&font, 1, &mut scratch),
            draw(&font, 1, &mut Scratch::default()),
            "normal glyph after {count} covered components"
        );
    }
}

#[test]
fn adr0067_i3_depth_budget_at_boundary() {
    for covered_count in [63u16, 64, 65] {
        let records: Vec<_> = (2..covered_count + 2)
            .map(|glyph| {
                (
                    glyph,
                    [0, 0, u8::try_from(glyph.checked_sub(1).unwrap()).unwrap()].to_vec(),
                )
            })
            .collect();
        let data = synthetic(&records);
        let font = Font::new(&data, 0).unwrap();
        let mut scratch = Scratch::default();
        let commands = draw(&font, u32::from(covered_count + 1), &mut scratch).1;
        assert_eq!(
            !commands.is_empty(),
            covered_count <= 64,
            "{covered_count} covered glyphs"
        );
        assert_eq!(
            draw(&font, 1, &mut scratch),
            draw(&font, 1, &mut Scratch::default()),
            "normal glyph after {covered_count} covered glyphs"
        );
    }
}

fn hb_path_line(command: HbOutlineCommand) -> String {
    let point = |p: sofel_geom::PointF| format!("{:08x} {:08x}", p.x.to_bits(), p.y.to_bits());
    match command {
        HbOutlineCommand::MoveTo(p) => format!("M {}", point(p)),
        HbOutlineCommand::LineTo(p) => format!("L {}", point(p)),
        HbOutlineCommand::QuadTo(c, p) => format!("Q {} {}", point(c), point(p)),
        HbOutlineCommand::CubicTo(c1, c2, p) => {
            format!("C {} {} {}", point(c1), point(c2), point(p))
        }
        HbOutlineCommand::Close => "Z".to_string(),
    }
}

#[test]
fn adr0067_i1_varc_float_paths_match_harfbuzz() {
    let cases = [
        (
            "varc-ac00-ac01.ttf",
            0xAC00,
            false,
            include_str!("../../../conformance/varc-paths/ac00-default.txt"),
        ),
        (
            "varc-ac00-ac01.ttf",
            0xAC00,
            true,
            include_str!("../../../conformance/varc-paths/ac00-wght840p3.txt"),
        ),
        (
            "varc-ac01-conditional.ttf",
            0xAC01,
            false,
            include_str!("../../../conformance/varc-paths/ac01-conditional-default.txt"),
        ),
        (
            "varc-ac01-conditional.ttf",
            0xAC01,
            true,
            include_str!("../../../conformance/varc-paths/ac01-conditional-wght840p3.txt"),
        ),
    ];
    for (name, unicode, heavy, expected) in cases {
        let bytes =
            std::fs::read(paths::testdata().join("harfbuzz/test/api/fonts").join(name)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let glyph = font.nominal_glyph_raw(unicode).unwrap();
        let location = if heavy {
            Location::from_design(&font, &[840.3])
        } else {
            Location::from_design(&font, &[])
        };
        let outlines = Outlines::new(font.instance(&location));
        let path = outlines
            .varc_glyph_path(glyph, sofel_font::HbScale::unscaled(&font))
            .unwrap();
        let actual: Vec<_> = path.into_iter().map(hb_path_line).collect();
        let expected: Vec<_> = expected.lines().collect();
        assert_eq!(
            actual.len(),
            expected.len(),
            "{name} U+{unicode:04X} heavy={heavy}"
        );
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                actual, expected,
                "{name} U+{unicode:04X} heavy={heavy} command {index}"
            );
        }
    }
}

#[test]
fn adr0067_i1_varc_float_cff2_matches_harfbuzz() {
    let bytes = source();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let path = outlines
        .varc_glyph_path(sofel_read::GlyphId(3), sofel_font::HbScale::unscaled(&font))
        .unwrap();
    let actual: Vec<_> = path.into_iter().map(hb_path_line).collect();
    let expected: Vec<_> = include_str!("../../../conformance/varc-paths/short-cff2.txt")
        .lines()
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn adr0067_i2_unrelated_axis_pin_with_gvar_dropped_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-unrelated-axis.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Unicodes, |set| set.add(0xAC01))
        .map_set(SetKind::DropTables, |set| {
            set.add(Tag::new(b"gvar").to_u32())
        })
        .with_axis_pinned(Tag::new(b"DUMY"), 0.0, &font)
        .unwrap();
    let ours = subset(&font, &input).unwrap();
    let reference = std::env::temp_dir().join(format!(
        "sofel-varc-unrelated-pin-{}.ttf",
        std::process::id()
    ));
    let output = ProcessCommand::new(paths::hb_dump())
        .arg("--subset")
        .arg(format!("--font-file={}", path.display()))
        .arg("--unicodes=U+AC01")
        .arg("--variations=DUMY=0")
        .arg("--drop-tables+=gvar")
        .arg(format!("--output-file={}", reference.display()))
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected = std::fs::read(&reference).unwrap();
    assert_eq!(ours, expected);
    let _ = std::fs::remove_file(reference);
}

#[test]
fn adr0067_i2_unrelated_nondefault_axis_changes_without_gvar_match_harfbuzz() {
    let original = paths::testdata().join("harfbuzz/test/api/fonts/varc-unrelated-axis.ttf");
    let source =
        std::env::temp_dir().join(format!("sofel-varc-no-gvar-{}.ttf", std::process::id()));
    let reference =
        std::env::temp_dir().join(format!("sofel-varc-no-gvar-pin-{}.ttf", std::process::id()));
    let strip = ProcessCommand::new(paths::hb_dump())
        .arg("--subset")
        .arg(format!("--font-file={}", original.display()))
        .arg("--unicodes=U+AC01")
        .arg("--drop-tables+=gvar")
        .arg(format!("--output-file={}", source.display()))
        .output()
        .unwrap();
    assert!(strip.status.success());
    let bytes = std::fs::read(&source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    assert!(font.table(Tag::new(b"VARC")).is_some());
    assert!(font.table(Tag::new(b"gvar")).is_none());
    for retain in [false, true] {
        let base = Input::new()
            .with_flags(if retain {
                Flags::RETAIN_GIDS
            } else {
                Flags::DEFAULT
            })
            .map_set(SetKind::Unicodes, |set| set.add(0xAC01));
        for (variation, input) in [
            (
                "DUMY=0.5",
                base.with_axis_pinned(Tag::new(b"DUMY"), 0.5, &font)
                    .unwrap(),
            ),
            (
                "DUMY=0:0.5:1",
                base.with_axis_range(Tag::new(b"DUMY"), Some(0.0), Some(1.0), Some(0.5), &font)
                    .unwrap(),
            ),
        ] {
            let ours = subset(&font, &input).unwrap();
            let mut command = ProcessCommand::new(paths::hb_dump());
            command
                .arg("--subset")
                .arg(format!("--font-file={}", source.display()))
                .arg("--unicodes=U+AC01")
                .arg(format!("--variations={variation}"))
                .arg(format!("--output-file={}", reference.display()));
            if retain {
                command.arg("--retain-gids");
            }
            let output = command.output().unwrap();
            assert!(output.status.success());
            let expected = std::fs::read(&reference).unwrap();
            assert_eq!(ours, expected, "{variation}, retain={retain}");
        }
    }
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(reference);
}

#[test]
fn adr0067_i2_referenced_axis_pin_remains_unsupported() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-unrelated-axis.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Unicodes, |set| set.add(0xAC01))
        .map_set(SetKind::DropTables, |set| {
            set.add(Tag::new(b"gvar").to_u32())
        })
        .with_axis_pinned_to_default(Tag::new(b"wght"), &font)
        .unwrap();
    assert!(subset(&font, &input).is_err());
    let destination = std::env::temp_dir().join(format!(
        "sofel-varc-referenced-pin-{}.ttf",
        std::process::id()
    ));
    let output = ProcessCommand::new(paths::hb_dump())
        .arg("--subset")
        .arg(format!("--font-file={}", path.display()))
        .arg(format!("--output-file={}", destination.display()))
        .arg("--unicodes=U+AC01")
        .arg("--variations=wght=356.5")
        .arg("--drop-tables+=gvar")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stderr, b"hb-dump --subset: subsetting failed\n");
}

#[test]
fn adr0067_i2_varc_passthrough_requires_retained_gids() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-ac00-ac01.ttf");
    let bytes = std::fs::read(path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let base = Input::new()
        .map_set(SetKind::Unicodes, |set| set.add(0xAC00))
        .map_set(SetKind::NoSubsetTables, |set| {
            set.add(Tag::new(b"VARC").to_u32())
        });
    assert!(subset(&font, &base).is_err());
    let retained = base.with_flags(Flags::RETAIN_GIDS);
    let output = subset(&font, &retained).unwrap();
    let output = Font::new(&output, 0).unwrap();
    assert!(output.table(Tag::new(b"VARC")).is_some());
    let pinned = retained
        .with_axis_pinned_to_default(Tag::new(b"0005"), &font)
        .unwrap();
    assert!(subset(&font, &pinned).is_err());
}
