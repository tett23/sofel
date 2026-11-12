//! Bounded VARC drawing with component-only records (sofel ADR 0067).

#![allow(clippy::all)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::indexing_slicing)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::var::Location;
use sofel_geom::{Command, FnPen};
use sofel_outline::{CBox, Outlines, Scale, Scratch};
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
    let mut varc = vec![0u8; 24];
    varc[..4].copy_from_slice(&[0, 1, 0, 0]);
    varc[20..24].copy_from_slice(&24u32.to_be_bytes());
    varc.extend_from_slice(&u32::try_from(records.len()).unwrap().to_be_bytes());
    varc.push(1);
    let mut offset = 1u8;
    for (_, record) in records {
        varc.push(offset);
        offset = offset
            .checked_add(u8::try_from(record.len()).unwrap())
            .unwrap();
    }
    varc.push(offset);
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
