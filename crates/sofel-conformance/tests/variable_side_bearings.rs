//! sofel ADR 0078: FreeType metric callbacks for HVAR and VVAR.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{Buf, sample_tables_with_glyf, sfnt};
use sofel_conformance::paths;
use sofel_font::{Font, read::GlyphId, var::Location};
use std::process::Command;

fn fixture(vertical: bool, entries: u16) -> Vec<u8> {
    let fvar = Buf::new()
        .u32(0x10000)
        .u16(16)
        .u16(2)
        .u16(1)
        .u16(20)
        .u16(0)
        .u16(8)
        .bytes(b"wght")
        .u32(0)
        .u32(0)
        .u32(65536)
        .u16(0)
        .u16(256)
        .done();
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(2)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .i16(-31)
        .done();
    let header = if vertical { 24 } else { 20 };
    let map_offset = header + u32::try_from(store.len()).unwrap();
    let mut table = Buf::new()
        .u32(65536)
        .u32(header)
        .u32(0)
        .u32(map_offset)
        .u32(0);
    if vertical {
        table = table.u32(0);
    }
    let mut table = table.bytes(&store).u16(0).u16(entries).done();
    table.extend(std::iter::repeat_n(1, usize::from(entries)));
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    if vertical {
        tables.push((b"vhea", build::hhea(1)));
        tables.push((b"vmtx", build::hmtx(&[(500, 7)], &[11; 27])));
        tables.push((b"VVAR", table));
    } else {
        tables.push((b"HVAR", table));
    }
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn check(vertical: bool) {
    for entries in [0, 1, 28] {
        let bytes = fixture(vertical, entries);
        let font = Font::new(&bytes, 0).unwrap();
        let count = font.num_glyphs();
        assert_eq!(count, 28);
        let path = std::env::temp_dir().join(format!(
            "sofel-bearing-{}-{vertical}-{entries}.ttf",
            std::process::id()
        ));
        std::fs::write(&path, &bytes).unwrap();
        for coordinate in [0u8, 1] {
            let location = Location::from_design(&font, &[f32::from(coordinate)]);
            let instance = font.instance(&location);
            let output = Command::new(paths::ft_dump())
                .arg("--metrics")
                .arg(format!("--variations=wght={coordinate}"))
                .arg(&path)
                .output()
                .unwrap();
            assert!(output.status.success());
            let dump = String::from_utf8(output.stdout).unwrap();
            for id in [0, 1, count - 1] {
                let glyph = GlyphId(id);
                let (field, advance, bearing) = if vertical {
                    (
                        "v_advance",
                        instance.ft_v_advance(glyph),
                        instance.ft_top_side_bearing(glyph, 0).unwrap(),
                    )
                } else {
                    (
                        "advance",
                        instance.ft_h_advance(glyph),
                        instance.ft_left_side_bearing(glyph),
                    )
                };
                assert!(
                    dump.lines()
                        .any(|line| line == format!("{field} {id} {}", advance.unwrap()))
                );
                let prefix = format!("bearing {id} ");
                let row = dump
                    .lines()
                    .find_map(|line| line.strip_prefix(&prefix))
                    .unwrap();
                assert_eq!(
                    row.split_whitespace().nth(usize::from(vertical)).unwrap(),
                    bearing.to_string()
                );
                if id == 0 {
                    assert_eq!(advance, Some(500 + 21 * u16::from(coordinate)));
                    assert_eq!(bearing, if vertical { 7 } else { 0 });
                }
            }
            let outside = GlyphId(count);
            assert_eq!(instance.ft_top_side_bearing(outside, 0), None);
            assert_eq!(
                instance.ft_left_side_bearing(outside),
                font.default_instance().ft_left_side_bearing(outside)
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn adr0078_i01_hvar_lsb_map_keeps_freetype_bearing() {
    check(false);
}

#[test]
fn adr0078_i02_vvar_tsb_map_keeps_freetype_bearing() {
    check(true);
}
