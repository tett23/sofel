//! CID Type 1 outline compared with pinned FreeType 2.15.0.

use sofel_font::CidFace;
use sofel_geom::{Command, Fixed, FnPen, Point};
use sofel_outline::{Scale, draw_cid};

#[test]
fn adr0065_i1_cid_triangle_matches_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = b"%!PS-Adobe-3.0 Resource-CIDFont\n\
/CIDFontName /TestCID def\n/CIDFontType 0 def\n\
/Registry (Adobe) def\n/Ordering (Identity) def\n/Supplement 0 def\n\
/CIDCount 1 def\n/FDBytes 1 def\n/GDBytes 2 def\n/CIDMapOffset 0 def\n\
/FDArray 1 array\n%ADOBeginFontDict\n\
dup 0 10 dict begin\n/FontName /TestCID def\n/FontType 1 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] def\n\
/Private 1 dict begin /lenIV -1 def end\n\
end put\n%ADOEndFontDict\ndef\n(Binary) 24 StartData\n"
        .to_vec();
    source.extend_from_slice(&[
        0, 0, 6, 0, 0, 24, 139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9,
        14,
    ]);
    let face = CidFace::parse(&source, 1024)?;
    let mut commands = Vec::new();
    let info = draw_cid(&face, 0, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
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
    let mut scaled = Vec::new();
    let scale = Scale::Scaled {
        x: Fixed(4_194_304),
        y: Fixed(4_194_304),
    };
    let scaled_info = draw_cid(&face, 0, scale, &mut FnPen(|c| scaled.push(c)))?;
    assert_eq!(
        scaled.first(),
        Some(&Command::MoveTo(Point { x: 6400, y: 6400 }))
    );
    assert_eq!(
        (
            scaled_info.cbox.x_min,
            scaled_info.cbox.y_min,
            scaled_info.cbox.x_max,
            scaled_info.cbox.y_max
        ),
        (6400, 6400, 12800, 12800)
    );
    Ok(())
}
