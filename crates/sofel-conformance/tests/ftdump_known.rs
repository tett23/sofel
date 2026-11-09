//! `ft-dump` on fonts whose contents are known by construction must
//! print exactly what the fonts hold (ADR 0008).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_conformance::paths;
use std::fmt::Write;
use std::path::PathBuf;
use std::process::Command;

fn ft_dump(name: &str, font: &[u8]) -> String {
    let exe = paths::ft_dump();
    if let Err(e) = paths::require(&exe, "ft-dump") {
        panic!("{e}");
    }
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!("sofel-ftdump-{name}-{}.ttf", std::process::id()));
    std::fs::write(&path, font).unwrap();
    let out = Command::new(exe).arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ft-dump failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// What `sample_font()` holds, written out by hand.
fn sample_expected(face: u32) -> String {
    let mut s = format!(
        "face {face}
num_glyphs 28
head version 65536
head font_revision 163840
head checksum_adjustment 0
head magic_number 1594834165
head flags 11
head units_per_em 1000
head created 0 3000000000
head modified 0 3100000000
head x_min -50
head y_min -200
head x_max 950
head y_max 800
head mac_style 1
head lowest_rec_ppem 9
head font_direction_hint 2
head index_to_loc_format 0
head glyph_data_format 0
hhea version 65536
hhea ascender 800
hhea descender -200
hhea line_gap 90
hhea advance_width_max 1200
hhea min_left_side_bearing -30
hhea min_right_side_bearing -40
hhea x_max_extent 1100
hhea caret_slope_rise 1
hhea caret_slope_run 0
hhea caret_offset 5
hhea metric_data_format 0
hhea number_of_h_metrics 20
maxp version 20480
maxp num_glyphs 28
cmap 3 1 4
"
    );
    // 'A'..='Z' map to glyphs 1..=26.
    for (i, c) in (0x41u32..=0x5A).enumerate() {
        writeln!(s, "map {c} {}", i + 1).unwrap();
    }
    s.push_str("cmap 3 10 12\n");
    for (i, c) in (0x41u32..=0x5A).enumerate() {
        writeln!(s, "map {c} {}", i + 1).unwrap();
    }
    s.push_str("map 128512 27\n");
    // Glyphs 0..20 have long metrics 500 + g; the rest repeat the last.
    for g in 0..28u32 {
        writeln!(s, "advance {g} {}", 500 + g.min(19)).unwrap();
    }
    s.push_str("end\n");
    s
}

#[test]
fn sample_font_is_dumped_exactly() {
    assert_eq!(
        ft_dump("sample", &sfnt(&sample_tables_with_glyf())),
        sample_expected(0)
    );
}

#[test]
fn collections_list_each_face() {
    let file = ttc(
        true,
        &[sample_tables_with_glyf(), sample_tables_with_glyf()],
    );
    assert_eq!(
        ft_dump("ttc", &file),
        sample_expected(0) + &sample_expected(1)
    );
}

#[test]
fn broken_subtables_are_left_out() {
    // A format 4 subtable with start > end, which FreeType rejects, and a
    // valid format 12 subtable.
    let mut tables = sample_tables_with_glyf();
    for (t, d) in tables.iter_mut() {
        if *t == b"cmap" {
            *d = cmap(&[
                (3, 1, cmap4(&[seg(0x45, 0x41, 0), seg(0xFFFF, 0xFFFF, 1)])),
                (3, 10, cmap12_13(12, &[(0x41, 0x41, 5)])),
            ]);
        }
    }
    let out = ft_dump("broken", &sfnt(&tables));
    let cmaps: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with("cmap ") || l.starts_with("map "))
        .collect();
    assert_eq!(cmaps, vec!["cmap 3 10 12", "map 65 5"]);
}
