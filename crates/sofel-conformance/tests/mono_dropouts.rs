//! Dropout control of the monochrome rasterizer against FreeType's
//! `ftraster.c` (sofel ADR 0064 I2). Each case is a small polygon outline
//! in 26.6 pixels drawn with one combination of `FT_OUTLINE_*` flags and
//! compared with `ft-dump --mono-outline`.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::Write;
use std::process::{Command, Stdio};

use sofel_conformance::paths;
use sofel_font::geom::{Pen, Point};
use sofel_raster::{MonoOptions, Rasterizer};

const IGNORE_DROPOUTS: u32 = 0x8;
const SMART_DROPOUTS: u32 = 0x10;
const INCLUDE_STUBS: u32 = 0x20;
const HIGH_PRECISION: u32 = 0x100;
const SINGLE_PASS: u32 = 0x200;

type Contour = &'static [(i32, i32)];

/// A diamond whose right corner touches the centre of its last column on
/// a horizontal pixel edge: the horizontal sweep sees an upper stub there.
const DIAMOND: Contour = &[(-96, -128), (-208, -192), (-320, -128), (-208, -64)];

/// A contour collapsed onto the same pixel edge, as the automatic hinter
/// leaves the counters of Roboto's infinity sign (glyph 523) at 10 ppem.
/// Its profiles have the same x as the diamond's on that column.
const COLLAPSED: Contour = &[(-162, -128), (-90, -128)];

/// A vertical sliver between two pixel centres, nearer the right one.
const SLIVER: Contour = &[(70, 0), (70, 256), (90, 256), (90, 0)];

/// A horizontal sliver between two pixel centres.
const BAR: Contour = &[(0, 70), (0, 90), (256, 90), (256, 70)];

/// A spike whose tip ends between scanlines more than half a pixel above
/// the last one, on a span at least half a pixel wide: an overshoot.
const SPIKE: Contour = &[(0, 0), (40, 120), (80, 0)];

/// A spike whose tip ends less than half a pixel above the last scanline.
const SHORT_SPIKE: Contour = &[(0, 0), (40, 90), (80, 0)];

const CASES: &[(&str, &[Contour], u32)] = &[
    ("collapsed contour ties", &[DIAMOND, COLLAPSED], 0),
    (
        "collapsed contour ties, high precision",
        &[DIAMOND, COLLAPSED],
        HIGH_PRECISION,
    ),
    (
        "collapsed contour ties, smart",
        &[DIAMOND, COLLAPSED],
        SMART_DROPOUTS,
    ),
    ("upper stub excluded", &[DIAMOND], 0),
    ("upper stub included", &[DIAMOND], INCLUDE_STUBS),
    ("simple dropout", &[SLIVER], 0),
    ("smart dropout", &[SLIVER], SMART_DROPOUTS),
    ("ignored dropout", &[SLIVER], IGNORE_DROPOUTS),
    ("horizontal sweep dropout", &[BAR], 0),
    ("single pass", &[BAR], SINGLE_PASS),
    ("overshoot stub", &[SPIKE], 0),
    ("stub without overshoot", &[SHORT_SPIKE], 0),
    (
        "stub without overshoot included",
        &[SHORT_SPIKE],
        INCLUDE_STUBS,
    ),
];

fn reference(contours: &[Contour], flags: u32) -> Vec<String> {
    let mut child = Command::new(paths::ft_dump())
        .args(["--mono-outline", &flags.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = String::new();
    for contour in contours {
        for (x, y) in *contour {
            input.push_str(&format!("{x} {y} 1\n"));
        }
        input.push_str("end\n");
    }
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

fn render(contours: &[Contour], flags: u32) -> Vec<String> {
    let mut raster = Rasterizer::new();
    for contour in contours {
        let mut points = contour.iter().map(|&(x, y)| Point { x, y });
        raster.move_to(points.next().unwrap());
        for p in points {
            raster.line_to(p);
        }
        raster.close();
    }
    let options = MonoOptions {
        high_precision: flags & HIGH_PRECISION != 0,
        single_pass: flags & SINGLE_PASS != 0,
        ignore_dropouts: flags & IGNORE_DROPOUTS != 0,
        smart_dropouts: flags & SMART_DROPOUTS != 0,
        include_stubs: flags & INCLUDE_STUBS != 0,
    };
    let bitmap = raster.render_mono(options, None).unwrap();
    let b = bitmap.bounds;
    let mut lines = vec![format!(
        "bitmap {} {} {} {}",
        b.left, b.top, b.width, b.rows
    )];
    lines.extend(
        bitmap
            .pixels
            .chunks_exact(bitmap.stride)
            .map(|row| row.iter().map(|v| format!("{v:02x}")).collect::<String>()),
    );
    lines
}

#[test]
fn adr0064_i2_mono_dropout_rules_match_freetype() {
    paths::require(&paths::ft_dump(), "ft-dump").unwrap();
    let mismatches: Vec<String> = CASES
        .iter()
        .filter_map(|&(name, contours, flags)| {
            let (actual, expected) = (render(contours, flags), reference(contours, flags));
            (actual != expected).then(|| format!("{name}: sofel {actual:?} freetype {expected:?}"))
        })
        .collect();
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
