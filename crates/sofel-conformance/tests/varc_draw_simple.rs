//! Bounded VARC drawing with component-only records (sofel ADR 0067).

#![allow(clippy::all)]
#![allow(clippy::unwrap_used)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_geom::{Command, FnPen};
use sofel_outline::{CBox, Outlines, Scale, Scratch};
use sofel_read::{GlyphId, Tag, varc::Varc};

fn source() -> Vec<u8> {
    std::fs::read(paths::testdata().join("harfbuzz/test/api/fonts/varc-short-cff2.otf")).unwrap()
}

fn synthetic(records: &[(u16, Vec<u8>)]) -> Vec<u8> {
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
