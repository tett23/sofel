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

#[test]
fn adr0065_i1_type1_mm_blends_other_subr_14() -> Result<(), Box<dyn std::error::Error>> {
    // 0 500 hsbw; 100 100 rmoveto; blend 100 + 100*w1; hlineto.
    let program = [
        139, 248, 136, 13, 239, 239, 21, 239, 239, 141, 153, 12, 16, 12, 17, 6, 9, 14,
    ];
    let (pfa, _) = fixture::with_glyph_program(&program);
    let marker = b"currentdict end";
    let at = pfa
        .windows(marker.len())
        .position(|part| part == marker)
        .ok_or("missing marker")?;
    let mut mm_pfa = pfa.get(..at).ok_or("invalid marker")?.to_vec();
    mm_pfa.extend_from_slice(b"/BlendDesignPositions [[0] [1]] def\n/BlendDesignMap [[[50 0] [999 1]]] def\n/BlendAxisTypes [/Weight] def\n/WeightVector [0.631191 0.368809] def\n");
    mm_pfa.extend_from_slice(pfa.get(at..).ok_or("invalid marker")?);
    let source = Type1Source::parse_pfa(&mm_pfa, 4096)?;
    let mut face = Type1Face::new(&source)?;
    let mut commands = Vec::new();
    draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    assert!(commands.contains(&Command::LineTo(Point { x: 236, y: 100 })));
    face.set_design_coordinates(&[50 << 16])?;
    commands.clear();
    draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    assert!(commands.contains(&Command::LineTo(Point { x: 200, y: 100 })));
    face.set_design_coordinates(&[999 << 16])?;
    commands.clear();
    draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    assert!(commands.contains(&Command::LineTo(Point { x: 300, y: 100 })));
    Ok(())
}

#[test]
fn adr0065_i1_type1_mm_blends_other_subr_15_two_points() -> Result<(), Box<dyn std::error::Error>> {
    let program = [
        139, 248, 136, 13, 239, 239, 21, 239, 139, 239, 239, 143, 154, 12, 16, 12, 17, 12, 17, 5,
        9, 14,
    ];
    let (pfa, _) = fixture::with_glyph_program(&program);
    let marker = b"currentdict end";
    let at = pfa
        .windows(marker.len())
        .position(|part| part == marker)
        .ok_or("missing marker")?;
    let mut mm_pfa = pfa.get(..at).ok_or("invalid marker")?.to_vec();
    mm_pfa.extend_from_slice(b"/BlendDesignPositions [[0] [1]] def\n/BlendDesignMap [[[50 0] [999 1]]] def\n/BlendAxisTypes [/Weight] def\n/WeightVector [0.631191 0.368809] def\n");
    mm_pfa.extend_from_slice(pfa.get(at..).ok_or("invalid marker")?);
    let source = Type1Source::parse_pfa(&mm_pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    let mut commands = Vec::new();
    draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    assert!(commands.contains(&Command::LineTo(Point { x: 236, y: 136 })));
    Ok(())
}

#[test]
fn adr0065_i2_type1_mm_rejects_blend_without_master_data() -> Result<(), Box<dyn std::error::Error>>
{
    let program = [139, 248, 136, 13, 239, 239, 141, 153, 12, 16, 12, 17, 6, 14];
    let (pfa, _) = fixture::with_glyph_program(&program);
    let source = Type1Source::parse_pfa(&pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    let mut commands = Vec::new();
    assert!(draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c))).is_err());
    Ok(())
}

#[test]
fn adr0065_i1_type1_other_subr_3_passes_operand() -> Result<(), Box<dyn std::error::Error>> {
    let program = [
        139, 248, 136, 13, 239, 239, 21, 239, 140, 142, 12, 16, 12, 17, 6, 9, 14,
    ];
    let (pfa, _) = fixture::with_glyph_program(&program);
    let source = Type1Source::parse_pfa(&pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    let mut commands = Vec::new();
    draw_type1(&face, 1, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    assert!(commands.contains(&Command::LineTo(Point { x: 200, y: 100 })));
    Ok(())
}

/// Glyphs for `seac`: A, acute (std code 194), B = A + acute, then three
/// composites FreeType 2.15.0 rejects (error 3 from `ft-dump --outline`).
fn seac_face_bytes() -> Vec<u8> {
    let a: &[u8] = &[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ];
    let acute: &[u8] = &[
        169, 248, 136, 13, 159, 247, 42, 21, 179, 139, 5, 119, 169, 5, 9, 14,
    ];
    // 10 500 hsbw; 30 80 20 65 194 seac (B), then the same with the base
    // code 66 (B itself, nested), 67 (no glyph C), and the accent code 300.
    let b: &[u8] = &[149, 248, 136, 13, 169, 219, 159, 204, 247, 86, 12, 6];
    let nest: &[u8] = &[149, 248, 136, 13, 169, 219, 159, 205, 247, 86, 12, 6];
    let missing: &[u8] = &[149, 248, 136, 13, 169, 219, 159, 206, 247, 86, 12, 6];
    let wide: &[u8] = &[149, 248, 136, 13, 169, 219, 159, 204, 247, 192, 12, 6];
    fixture::with_glyphs(
        &[
            ("A", a),
            ("acute", acute),
            ("B", b),
            ("nest", nest),
            ("missing", missing),
            ("wide", wide),
        ],
        &[(65, "A"), (66, "B")],
    )
    .0
}

fn lines(points: &[(i32, i32)]) -> Vec<Command> {
    let mut out: Vec<Command> = points
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let p = Point { x, y };
            if i == 0 {
                Command::MoveTo(p)
            } else {
                Command::LineTo(p)
            }
        })
        .collect();
    out.push(Command::Close);
    out
}

#[test]
fn adr0065_i1_type1_seac_matches_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let pfa = seac_face_bytes();
    let source = Type1Source::parse_pfa(&pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    // ft-dump --outline unscaled: the base at its own side bearing, the
    // accent moved by adx + lsb(seac glyph) - asb = 80 + 10 - 30.
    let mut commands = Vec::new();
    let info = draw_type1(&face, 3, Scale::Unscaled, &mut FnPen(|c| commands.push(c)))?;
    let mut want = lines(&[(100, 100), (200, 100), (200, 200), (100, 100)]);
    want.extend(lines(&[(110, 170), (150, 170), (130, 200), (110, 170)]));
    assert_eq!(commands, want);
    assert_eq!(
        (
            info.cbox.x_min,
            info.cbox.y_min,
            info.cbox.x_max,
            info.cbox.y_max
        ),
        (100, 100, 200, 200)
    );
    // ft-dump --outline 16.
    let fixed = sofel_geom::Fixed(sofel_geom::div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    draw_type1(
        &face,
        3,
        Scale::Scaled { x: fixed, y: fixed },
        &mut FnPen(|c| commands.push(c)),
    )?;
    let mut want = lines(&[(102, 102), (205, 102), (205, 205), (102, 102)]);
    want.extend(lines(&[(113, 174), (154, 174), (133, 205), (113, 174)]));
    assert_eq!(commands, want);
    Ok(())
}

#[test]
fn adr0065_i1_type1_seac_rejects_what_freetype_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let pfa = seac_face_bytes();
    let source = Type1Source::parse_pfa(&pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    // Nested seac, a base code without a glyph, an accent code above 255.
    for glyph in [4, 5, 6] {
        assert_eq!(
            draw_type1(&face, glyph, Scale::Unscaled, &mut FnPen(|_| {})).err(),
            Some(sofel_outline::OutlineError::InvalidCharString),
            "glyph {glyph}"
        );
    }
    Ok(())
}
