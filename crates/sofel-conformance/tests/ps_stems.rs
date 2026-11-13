//! PostScript stem maps against pinned FreeType (sofel ADR 0064 I1).
#![allow(clippy::unwrap_used)]

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod fixture;

use sofel_conformance::paths;
use sofel_font::{Type1Face, Type1Source};
use sofel_geom::{Command, Fixed, FnPen, div_fix};
use sofel_outline::{HintTarget, OutlineError, Scale, draw_type1_native};

#[test]
fn adr0064_i1_type1_multiple_hstems_leave_scratch_untouched() {
    let (pfa, _) = fixture::with_glyph_program(&[
        139, 248, 136, 13, 189, 179, 189, 179, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139,
        5, 139, 39, 5, 9, 14,
    ]);
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    assert_eq!(
        draw_type1_native(
            &face,
            1,
            Scale::Scaled {
                x: factor,
                y: factor
            },
            HintTarget::Light,
            &mut FnPen(|c| commands.push(c)),
        ),
        Err(OutlineError::UnsupportedFormat),
    );
    assert!(commands.is_empty());
}

#[test]
fn adr0064_i1_type1_blue_zone_stem_is_unsupported() {
    let (pfa, _) = fixture::with_glyph_name_and_blues(
        &[
            139, 248, 136, 13, 239, 239, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5,
            139, 39, 5, 9, 14,
        ],
        "A",
        "[0 10 200 210]",
    );
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-blue-stem-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let ppem = 16;
    let factor = Fixed(div_fix(ppem * 64, 1000));
    let mut commands = Vec::new();
    assert_eq!(
        draw_type1_native(
            &face,
            1,
            Scale::Scaled {
                x: factor,
                y: factor
            },
            HintTarget::Light,
            &mut FnPen(|c| commands.push(c))
        ),
        Err(OutlineError::UnsupportedFormat),
    );
    assert!(commands.is_empty());
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "light"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert!(
        String::from_utf8(reference.stdout)
            .unwrap()
            .contains("point 102 153 1")
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_type1_single_hstem_matches_freetype() {
    let mut cases = Vec::new();
    for bottom in [-75, -50, -25, 0, 10, 20, 30, 50, 75, 100] {
        for width in [1, 5, 10, 15, 20, 30, 40, 50, 60, 70, 100] {
            cases.push((bottom, width, bottom, width));
        }
    }
    cases.push((50, 40, 0, 100));
    for (bottom, width, contour_bottom, contour_width) in cases {
        let mut program = vec![139, 248, 136, 13]; // hsbw
        program.extend(number(bottom));
        program.extend(number(width));
        program.push(1); // hstem
        program.extend([239, 239, 3, 239]); // vstem x=100, width=100
        program.extend(number(contour_bottom));
        program.push(21); // rmoveto x=100, y=bottom
        program.extend([239, 139, 5, 139]); // rlineto right 100
        program.extend(number(contour_width));
        program.extend([5, 39, 139, 5, 139]); // up width, left 100
        program.extend(number(-contour_width));
        program.extend([5, 9, 14]); // down width, closepath, endchar
        let (pfa, _) = fixture::with_glyph_program(&program);
        let path =
            std::env::temp_dir().join(format!("sofel-adr0064-ps-stem-{}.pfa", std::process::id()));
        std::fs::write(&path, &pfa).unwrap();
        let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
        let face = Type1Face::new(&source).unwrap();
        for ppem in [8, 16, 32] {
            let factor = Fixed(div_fix(ppem * 64, 1000));
            let scale = Scale::Scaled {
                x: factor,
                y: factor,
            };
            let mut commands = Vec::new();
            let (_, advance) = draw_type1_native(
                &face,
                1,
                scale,
                HintTarget::Light,
                &mut FnPen(|c| commands.push(c)),
            )
            .unwrap();
            let mut points = Vec::new();
            for command in commands {
                match command {
                    Command::MoveTo(p) | Command::LineTo(p) => points.push(p),
                    Command::Close => {
                        if points.last() == points.first() {
                            points.pop();
                        }
                    }
                    other => assert_eq!(other, Command::Close),
                }
            }
            let mut actual = format!("advance {advance} 0\n");
            for p in &points {
                actual.push_str(&format!("point {} {} 1\n", p.x, p.y));
            }
            actual.push_str(&format!("end {}\n", points.len() - 1));
            let reference = std::process::Command::new(paths::ft_dump())
                .args(["--hint-slot", &ppem.to_string(), "1", "native", "light"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(reference.status.success());
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "ppem {ppem}, bottom {bottom}, width {width}"
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}

fn number(value: i32) -> Vec<u8> {
    assert!((-107..=107).contains(&value));
    vec![u8::try_from(value.saturating_add(139)).unwrap()]
}
