//! sofel ADR 0063 I1/I2: the pinned HarfBuzz 15.1.0 reference on Roboto.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::excessive_precision
)] // Printed HarfBuzz reference coordinates keep three decimals.

use sofel_conformance::paths;
use sofel_font::read::GlyphId;
use sofel_font::{Font, GlyphExtents, HbScale};
use sofel_shape::{
    Direction, GlyphOutline, ResolvedProperties, Scale, Script, ShapePlan, SubFont, UnicodeBuffer,
    shape_with_subfont,
};

fn font_outline(font: &Font<'_>, glyph: u32, scale: Scale) -> Option<GlyphOutline> {
    let x = (scale.scale.x_scale() as f32) / f32::from(font.hb_units_per_em());
    let y = (scale.scale.y_scale() as f32) / f32::from(font.hb_units_per_em());
    let mut outline = GlyphOutline::new();
    if let Some(path) = sofel_outline::HbExtents::new(font.default_instance())
        .cff_glyph_path(GlyphId(glyph), &scale.scale)
    {
        for command in path {
            match command {
                sofel_outline::HbOutlineCommand::MoveTo(p) => outline.move_to(p),
                sofel_outline::HbOutlineCommand::LineTo(p) => outline.line_to(p),
                sofel_outline::HbOutlineCommand::QuadTo(c, p) => outline.quad_to(c, p),
                sofel_outline::HbOutlineCommand::CubicTo(c1, c2, p) => outline.cubic_to(c1, c2, p),
                sofel_outline::HbOutlineCommand::Close => outline.close(),
            }
        }
        return Some(outline);
    }
    let mut scratch = sofel_outline::Scratch::default();
    sofel_outline::load(
        font.default_instance(),
        GlyphId(glyph),
        sofel_outline::Scale::Unscaled,
        &mut scratch,
    )
    .ok()?;
    scratch
        .hb_decompose(x, y, |command| match command {
            sofel_outline::HbOutlineCommand::MoveTo(p) => outline.move_to(p),
            sofel_outline::HbOutlineCommand::LineTo(p) => outline.line_to(p),
            sofel_outline::HbOutlineCommand::QuadTo(c, p) => outline.quad_to(c, p),
            sofel_outline::HbOutlineCommand::CubicTo(c1, c2, p) => outline.cubic_to(c1, c2, p),
            sofel_outline::HbOutlineCommand::Close => outline.close(),
        })
        .ok()?;
    Some(outline)
}

#[test]
fn adr0063_i2_cff_synthetic_outline_matches_harfbuzz() {
    let path =
        paths::testdata().join("harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf");
    let bytes = std::fs::read(path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let scale = Scale::new(HbScale::new(4096, 2048, font.hb_units_per_em()), 0, 0);
    let mut subfont = SubFont::new(scale);
    subfont.set_outline_func(|glyph| font_outline(&font, glyph, scale));
    subfont.set_synthetic_bold(0.1, 0.1, false);
    let outline = subfont.glyph_outline(71).unwrap();
    assert_eq!(
        outline.extents(),
        GlyphExtents {
            x_bearing: 135,
            y_bearing: 1862,
            width: 1847,
            height: -1862,
        }
    );
    let first = outline.points()[0];
    assert!((first.x - 1982.449).abs() < 0.01);
    assert!((first.y - 1793.467).abs() < 0.01);
    let last = *outline.points().last().unwrap();
    assert!((last.x - 1982.449).abs() < 0.01);
    assert!((last.y - 1793.467).abs() < 0.01);
    assert_eq!(outline.contours().len(), 1);
}

fn shape_a(font: &Font<'_>, subfont: &SubFont<'_>) -> (u32, i32) {
    let instance = font.default_instance();
    let plan = ShapePlan::new(
        instance,
        ResolvedProperties {
            direction: Direction::Ltr,
            script: Script::from_iso15924(b"Latn"),
            language: None,
        },
        &[],
    );
    let mut buffer = UnicodeBuffer::new();
    buffer.push_utf8(b"a", 0, None);
    let shaped = shape_with_subfont(instance, subfont, &plan, buffer);
    (
        shaped.glyph_infos()[0].glyph,
        shaped.glyph_positions()[0].x_advance,
    )
}

#[test]
fn adr0063_i2_roboto_synthetic_outline_extents_match_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf");
    let bytes = std::fs::read(path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let scale = Scale::new(HbScale::new(4096, 2048, font.hb_units_per_em()), 0, 0);
    let mut base = SubFont::new(scale);
    base.set_outline_func(|glyph| font_outline(&font, glyph, scale));
    base.set_synthetic_slant(0.2);
    let outline = base.glyph_outline(71).unwrap();
    let expected = [
        (2325.600, 529.000),
        (2230.800, 292.000),
        (1948.400, 136.000),
        (1666.000, -20.000),
        (1264.000, -20.000),
        (854.000, -20.000),
        (690.000, 125.000),
        (620.000, 0.000),
        (280.000, 0.000),
        (894.400, 1536.000),
        (1266.400, 1536.000),
        (1038.000, 965.000),
        (1312.800, 1102.000),
        (1708.800, 1102.000),
        (2116.800, 1102.000),
        (2275.000, 950.000),
        (2433.200, 798.000),
        (2334.000, 550.000),
        (2325.600, 529.000),
        (1554.400, 946.000),
        (1352.400, 946.000),
        (1207.000, 897.500),
        (1061.600, 849.000),
        (963.200, 778.000),
        (774.800, 307.000),
        (818.000, 235.000),
        (926.400, 186.000),
        (1034.800, 137.000),
        (1234.800, 137.000),
        (1534.800, 137.000),
        (1712.000, 252.500),
        (1889.200, 368.000),
        (1953.600, 529.000),
        (1962.000, 550.000),
        (2026.400, 711.000),
        (1948.400, 828.500),
        (1870.400, 946.000),
        (1554.400, 946.000),
    ];
    assert_eq!(outline.contours(), &[19, 38]);
    assert_eq!(outline.points().len(), expected.len());
    for (index, (point, (x, y))) in outline.points().iter().zip(expected).enumerate() {
        assert!(
            (point.x - x).abs() < 0.002 && (point.y - y).abs() < 0.002,
            "point {index}: {point:?} != ({x}, {y})"
        );
    }
    assert_eq!(
        outline.extents(),
        GlyphExtents {
            x_bearing: 280,
            y_bearing: 1536,
            width: 2153,
            height: -1556
        }
    );
    assert_eq!(outline.contours().len(), 2);
    let first = outline.points()[0];
    assert!((first.x - 2325.6).abs() < 0.001 && (first.y - 529.0).abs() < 0.001);
}

#[test]
fn adr0063_i2_roboto_synthetic_bold_outline_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf");
    let bytes = std::fs::read(path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let scale = Scale::new(HbScale::new(4096, 2048, font.hb_units_per_em()), 0, 0);
    let mut subfont = SubFont::new(scale);
    subfont.set_outline_func(|glyph| font_outline(&font, glyph, scale));
    subfont.set_synthetic_bold(0.1, 0.1, false);
    let outline = subfont.glyph_outline(71).unwrap();
    let expected = [
        (2524.000, 631.500),
        (2524.000, 341.528),
        (2217.578, 154.887),
        (1944.306, -20.000),
        (1477.000, -20.000),
        (1005.983, -20.000),
        (906.082, 89.527),
        (999.807, 0.000),
        (280.000, 0.000),
        (280.000, 1741.000),
        (1062.000, 1741.000),
        (1000.922, 1252.079),
        (1018.388, 1307.000),
        (1473.000, 1307.000),
        (1945.171, 1307.000),
        (2216.888, 1136.705),
        (2524.000, 954.128),
        (2524.000, 652.500),
        (2524.000, 631.500),
        (1381.000, 946.000),
        (1217.092, 946.000),
        (1126.642, 904.342),
        (1041.925, 863.787),
        (1062.000, 838.468),
        (1062.000, 451.957),
        (1043.076, 425.419),
        (1130.290, 384.226),
        (1222.897, 342.000),
        (1385.000, 342.000),
        (1607.533, 342.000),
        (1680.426, 431.884),
        (1742.000, 516.778),
        (1742.000, 631.500),
        (1742.000, 652.500),
        (1742.000, 769.175),
        (1681.593, 856.316),
        (1615.776, 946.000),
        (1381.000, 946.000),
    ];
    assert_eq!(outline.contours(), &[19, 38]);
    assert_eq!(outline.points().len(), expected.len());
    for (index, (point, (x, y))) in outline.points().iter().zip(expected).enumerate() {
        assert!(
            (point.x - x).abs() < 0.002 && (point.y - y).abs() < 0.002,
            "point {index}: {point:?} != ({x}, {y})"
        );
    }
    assert_eq!(
        outline.extents(),
        GlyphExtents {
            x_bearing: 280,
            y_bearing: 1741,
            width: 2244,
            height: -1761
        }
    );
    let first = outline.points()[0];
    assert!((first.x - 2524.0).abs() < 0.001 && (first.y - 631.5).abs() < 0.001);
}

#[test]
fn adr0063_i1_i2_roboto_subfont_and_synthetic_advance_match_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf");
    let bytes = std::fs::read(path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let base = SubFont::new(Scale::unscaled(&font));
    assert_eq!(shape_a(&font, &base), (70, 1114));

    let mut parent = base.child();
    parent.set_h_advance_func(|_| 777);
    assert_eq!(shape_a(&font, &parent), (70, 777));

    let mut child = parent.child();
    child.set_scale(Scale::new(
        HbScale::new(4096, 2048, font.hb_units_per_em()),
        0,
        0,
    ));
    child.set_nominal_glyph_func(|cp| (cp == u32::from(b'a')).then_some(71));
    assert_eq!(shape_a(&font, &child), (71, 1554));

    child.set_synthetic_bold(0.1, 0.0, false);
    assert_eq!(shape_a(&font, &child), (71, 1964));
    child.set_synthetic_bold(0.1, 0.0, true);
    assert_eq!(shape_a(&font, &child), (71, 1554));
}
