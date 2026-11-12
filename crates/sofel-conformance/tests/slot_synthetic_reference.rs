//! sofel ADR 0063 I2: pinned FreeType 2.15.0 slot synthesis.

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_geom::Point;
use sofel_outline::{CBox, Scale, Scratch, SlotMetrics, load};

#[test]
fn adr0063_i2_oblique_slot_matches_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let path = paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf");
    if !path.exists() {
        return Ok(());
    }
    let data = std::fs::read(path)?;
    let font = Font::new(&data, 0)?;
    let mut scratch = Scratch::default();
    load(
        font.default_instance(),
        GlyphId(71),
        Scale::pixels(&font, 16, 16),
        &mut scratch,
    )?;
    assert_eq!(
        scratch.oblique_slot(),
        CBox {
            x_min: 70,
            y_min: -10,
            x_max: 614,
            y_max: 768
        }
    );
    Ok(())
}

#[test]
fn adr0063_i2_embolden_slot_matches_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let path = paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf");
    if !path.exists() {
        return Ok(());
    }
    let data = std::fs::read(path)?;
    let font = Font::new(&data, 0)?;
    let mut scratch = Scratch::default();
    load(
        font.default_instance(),
        GlyphId(71),
        Scale::pixels(&font, 16, 16),
        &mut scratch,
    )?;
    let mut metrics = SlotMetrics {
        advance_x: 575,
        advance_y: 0,
        width: 459,
        height: 778,
        hori_bearing_x: 70,
        hori_bearing_y: 768,
        hori_advance: 575,
        vert_bearing_x: -217,
        vert_bearing_y: 286,
        vert_advance: 1351,
    };
    assert_eq!(
        scratch.embolden_slot(16, 16, &mut metrics)?,
        CBox {
            x_min: 70,
            y_min: -10,
            x_max: 571,
            y_max: 810
        }
    );
    assert_eq!(
        metrics,
        SlotMetrics {
            advance_x: 617,
            advance_y: 0,
            width: 501,
            height: 820,
            hori_bearing_x: 70,
            hori_bearing_y: 810,
            hori_advance: 617,
            vert_bearing_x: -217,
            vert_bearing_y: 286,
            vert_advance: 1393,
        }
    );
    let expected = [
        (571, 286),
        (571, 160),
        (451, -10),
        (339, -10),
        (227, -10),
        (198, 28),
        (195, 0),
        (70, 0),
        (70, 810),
        (205, 810),
        (205, 563),
        (229, 593),
        (338, 593),
        (451, 593),
        (571, 427),
        (571, 296),
        (315, 473),
        (272, 473),
        (219, 432),
        (205, 405),
        (205, 180),
        (219, 153),
        (273, 111),
        (316, 111),
        (379, 111),
        (436, 211),
        (436, 286),
        (436, 296),
        (436, 372),
        (381, 473),
    ];
    assert_eq!(scratch.points(), expected.map(|(x, y)| Point { x, y }));
    Ok(())
}
