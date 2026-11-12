//! sofel ADR 0063 I2: CFF/CFF2 drawing against HarfBuzz 15.1.0.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use sofel_font::read::GlyphId;
use sofel_font::var::Location;
use sofel_font::{Font, HbScale};
use sofel_outline::{HbExtents, HbOutlineCommand};

fn lines(commands: &[HbOutlineCommand]) -> Vec<String> {
    commands
        .iter()
        .map(|command| match command {
            HbOutlineCommand::MoveTo(p) => format!("M {:.3} {:.3}", p.x, p.y),
            HbOutlineCommand::LineTo(p) => format!("L {:.3} {:.3}", p.x, p.y),
            HbOutlineCommand::QuadTo(c, p) => {
                format!("Q {:.3} {:.3} {:.3} {:.3}", c.x, c.y, p.x, p.y)
            }
            HbOutlineCommand::CubicTo(a, b, p) => format!(
                "C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3}",
                a.x, a.y, b.x, b.y, p.x, p.y
            ),
            HbOutlineCommand::Close => "Z".to_string(),
        })
        .collect()
}

fn compare(font_path: &str, glyph: u32, design: Option<&[f32]>, expected: &[&str]) {
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"));
    let path = dir.join(font_path);
    let Ok(bytes) = std::fs::read(&path) else {
        assert!(
            std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
            "missing {}",
            path.display()
        );
        return;
    };
    let font = Font::new(&bytes, 0).expect("reference font");
    let scale = HbScale::new(4096, 2048, font.hb_units_per_em());
    let location = design.map(|coordinates| Location::from_design(&font, coordinates));
    let instance = location
        .as_ref()
        .map_or_else(|| font.default_instance(), |loc| font.instance(loc));
    let path = HbExtents::new(instance)
        .cff_glyph_path(GlyphId(glyph), &scale)
        .expect("HarfBuzz path");
    assert_eq!(lines(&path), expected);
}

#[test]
fn adr0063_i2_cff1_path_matches_harfbuzz() {
    compare(
        "harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf",
        71,
        None,
        &[
            "M 1458.176 1619.968",
            "C 1351.680 1642.496 1220.608 1656.832 1085.440 1656.832",
            "C 643.072 1656.832 438.272 1515.520 438.272 1290.240",
            "L 438.272 1112.064",
            "L 135.168 1101.824",
            "L 135.168 960.512",
            "L 438.272 960.512",
            "L 438.272 0.000",
            "L 811.008 0.000",
            "L 811.008 960.512",
            "L 1282.048 960.512",
            "L 1282.048 1112.064",
            "L 811.008 1112.064",
            "L 811.008 1288.192",
            "C 811.008 1431.552 913.408 1507.328 1126.400 1507.328",
            "C 1204.224 1507.328 1294.336 1497.088 1376.256 1478.656",
            "L 1458.176 1619.968",
            "Z",
        ],
    );
}

#[test]
fn adr0063_i2_cff2_path_matches_harfbuzz() {
    compare(
        "harfbuzz/test/subset/data/fonts/SourceSerif4Variable-Roman-HelloWorld.otf",
        7,
        None,
        &[
            "M 1122.304 -26.624",
            "C 1597.440 -26.624 2064.384 143.360 2064.384 481.280",
            "C 2064.384 821.248 1593.344 999.424 1122.304 999.424",
            "C 647.168 999.424 184.320 821.248 184.320 481.280",
            "C 184.320 143.360 643.072 -26.624 1122.304 -26.624",
            "Z",
            "M 1122.304 65.536",
            "C 765.952 65.536 573.440 206.848 573.440 481.280",
            "C 573.440 757.760 765.952 905.216 1122.304 905.216",
            "C 1474.560 905.216 1675.264 757.760 1675.264 481.280",
            "C 1675.264 206.848 1474.560 65.536 1122.304 65.536",
            "Z",
        ],
    );
}

#[test]
fn adr0063_i2_cff2_variable_path_matches_harfbuzz() {
    compare(
        "harfbuzz/test/subset/data/fonts/SourceSerif4Variable-Roman-HelloWorld.otf",
        7,
        Some(&[900.0, 20.0]),
        &[
            "M 1171.456 -32.768",
            "C 1871.872 -32.768 2252.800 194.560 2252.800 505.856",
            "C 2252.800 817.152 1802.240 1030.144 1171.456 1030.144",
            "C 540.672 1030.144 90.112 811.008 90.112 505.856",
            "C 90.112 200.704 466.944 -32.768 1171.456 -32.768",
            "Z",
            "M 1171.456 79.872",
            "C 991.232 79.872 909.312 200.704 909.312 499.712",
            "C 909.312 796.672 991.232 917.504 1171.456 917.504",
            "C 1351.680 917.504 1433.600 796.672 1433.600 499.712",
            "C 1433.600 200.704 1351.680 79.872 1171.456 79.872",
            "Z",
        ],
    );
}
