//! Type 1 PFA/PFB outline compared with pinned FreeType 2.15.0.

use sofel_font::{Type1Face, Type1Source};
use sofel_geom::{Command, FnPen, Point};
use sofel_outline::{Scale, draw_type1};

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod fixture;

#[test]
fn adr0065_i1_type1_triangle_matches_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let (pfa, pfb) = fixture::sample();
    for (bytes, binary) in [(&pfa, false), (&pfb, true)] {
        let source = if binary {
            Type1Source::parse_pfb(bytes, 4096)?
        } else {
            Type1Source::parse_pfa(bytes, 4096)?
        };
        let face = Type1Face::new(&source)?;
        assert_eq!(face.nominal_glyph(b'A'), Some(1));
        assert_eq!(face.glyph_metrics(1).ok_or("metrics")?.advance_x, 500);
        let mut commands = Vec::new();
        let info = draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
        assert_eq!(
            commands,
            vec![
                Command::MoveTo(Point { x: 100, y: 100 }),
                Command::LineTo(Point { x: 200, y: 100 }),
                Command::LineTo(Point { x: 200, y: 200 }),
                Command::LineTo(Point { x: 100, y: 100 }),
                Command::Close,
            ]
        );
        assert_eq!(
            (
                info.cbox.x_min,
                info.cbox.y_min,
                info.cbox.x_max,
                info.cbox.y_max
            ),
            (100, 100, 200, 200)
        );
    }
    Ok(())
}

#[test]
fn adr0065_i1_type1_closepath_keeps_current_point() -> Result<(), Box<dyn std::error::Error>> {
    let program = [
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 9, 39, 39, 21, 239, 139, 5, 9,
        14,
    ];
    let (pfa, _) = fixture::with_glyph_program(&program);
    let source = Type1Source::parse_pfa(&pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    let mut commands = Vec::new();
    draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    assert_eq!(
        commands.get(5),
        Some(&Command::MoveTo(Point { x: 100, y: 100 }))
    );
    Ok(())
}
