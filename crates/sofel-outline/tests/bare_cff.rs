use sofel_font::Cff;
use sofel_font::read::GlyphId;
use sofel_geom::{Command, FnPen, Point};
use sofel_outline::{CBox, Scale, Scratch, draw_bare_cff};

const BARE_CFF: &[u8] = &[
    1, 0, 4, 4, // Header
    0, 1, 1, 1, 2, b'A', // Name INDEX
    0, 1, 1, 1, 3, 160, 17, // Top DICT INDEX
    0, 0, // String INDEX
    0, 0, // Global Subrs INDEX
    0, 1, 1, 1, 11, // CharStrings INDEX
    239, 239, 21, // 100 100 rmoveto
    239, 139, 5, // 100 0 rlineto
    139, 239, 5,  // 0 100 rlineto
    14, // endchar
];

#[test]
fn adr0065_i1_bare_cff_draws_unscaled_outline() {
    let face = Cff::from_bare(BARE_CFF, 0);
    assert!(face.is_ok());
    let Ok(face) = face else {
        return;
    };
    let mut scratch = Scratch::default();
    let mut commands = Vec::new();
    let result = draw_bare_cff(
        &face,
        GlyphId(0),
        Scale::Unscaled,
        &mut scratch,
        &mut FnPen(|command| commands.push(command)),
    );
    assert_eq!(
        result.map(|info| info.cbox),
        Ok(CBox {
            x_min: 100,
            y_min: 100,
            x_max: 200,
            y_max: 200
        })
    );
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
}
