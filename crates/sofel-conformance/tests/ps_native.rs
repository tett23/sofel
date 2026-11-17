//! PostScript native loading compared with pinned FreeType (sofel ADR 0064 I1).
#![allow(clippy::unwrap_used)]

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod triangle;

use sofel_conformance::paths;
use sofel_font::{CidFace, Type1Face, Type1Source};
use sofel_geom::{Command, Fixed, FnPen, div_fix};
use sofel_outline::{HintTarget, OutlineError, Scale, draw_cid_native, draw_type1_native};

fn cid_sample(program: &[u8]) -> Vec<u8> {
    let mut source = b"%!PS-Adobe-3.0 Resource-CIDFont\n\
/CIDFontName /TestCID def\n/CIDFontType 0 def\n\
/Registry (Adobe) def\n/Ordering (Identity) def\n/Supplement 0 def\n\
/CIDCount 1 def\n/FDBytes 1 def\n/GDBytes 2 def\n/CIDMapOffset 0 def\n\
/FDArray 1 array\n%ADOBeginFontDict\n\
dup 0 10 dict begin\n/FontName /TestCID def\n/FontType 1 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] def\n\
/Private 1 dict begin /lenIV -1 def end\n\
end put\n%ADOEndFontDict\ndef\n"
        .to_vec();
    let end = u16::try_from(6usize.saturating_add(program.len())).unwrap();
    source.extend_from_slice(format!("(Binary) {end} StartData\n").as_bytes());
    source.extend_from_slice(&[0, 0, 6, 0]);
    source.extend_from_slice(&end.to_be_bytes());
    source.extend_from_slice(program);
    source
}

#[test]
fn adr0064_i1_native_cid_without_stems_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!("sofel-adr0064-cid-{}.cid", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    for ppem in [8, 16, 32] {
        for (target_name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let factor = Fixed(div_fix(ppem * 64, i32::from(face.units_per_em())));
            let scale = Scale::Scaled {
                x: factor,
                y: factor,
            };
            let mut commands = Vec::new();
            let (_, advance) =
                draw_cid_native(&face, 0, scale, target, &mut FnPen(|c| commands.push(c))).unwrap();
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
                .args(["--hint-slot", &ppem.to_string(), "0", "native", target_name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "{target_name} {ppem}"
            );
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_stems_are_unsupported() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    assert_eq!(
        draw_cid_native(
            &face,
            0,
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
fn adr0064_i1_native_cid_matrix_matches_freetype() {
    let original = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let old = b"[0.001 0 0 0.001 0 0]";
    for (index, matrix) in [
        b"[0.001 0.0005 0 0.001 0 0]".as_slice(),
        b"[0.001 0 0 0.001 0.01 0.02]".as_slice(),
        b"[0.002 0 0 0.001 0 0]".as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bytes = original.clone();
        let at = bytes
            .windows(old.len())
            .position(|part| part == old)
            .unwrap();
        bytes.splice(at..at + old.len(), matrix.iter().copied());
        let path = std::env::temp_dir().join(format!(
            "sofel-adr0064-cid-matrix-{}-{index}.cid",
            std::process::id()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let face = CidFace::parse(&bytes, 1024).unwrap();
        for ppem in [8, 16] {
            let factor = Fixed(div_fix(ppem * 64, i32::from(face.units_per_em())));
            let scale = Scale::Scaled {
                x: factor,
                y: factor,
            };
            let mut commands = Vec::new();
            let (_, advance) = draw_cid_native(
                &face,
                0,
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
                .args(["--hint-slot", &ppem.to_string(), "0", "native", "light"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "matrix {index} ppem {ppem}"
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn adr0064_i1_native_type1_without_stems_matches_freetype() {
    let (pfa, _) = triangle::sample();
    let path = std::env::temp_dir().join(format!("sofel-adr0064-type1-{}.pfa", std::process::id()));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    for ppem in [8, 16, 32] {
        for (target_name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let factor = Fixed(div_fix(ppem * 64, 1000));
            let scale = Scale::Scaled {
                x: factor,
                y: factor,
            };
            let mut commands = Vec::new();
            let (_, advance) =
                draw_type1_native(&face, 1, scale, target, &mut FnPen(|c| commands.push(c)))
                    .unwrap();
            let mut points = Vec::new();
            for command in commands {
                match command {
                    Command::MoveTo(p) | Command::LineTo(p) => {
                        points.push(p);
                    }
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
                .args(["--hint-slot", &ppem.to_string(), "1", "native", target_name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(reference.status.success());
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "{target_name} {ppem}"
            );
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_stems_are_unsupported() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
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
fn adr0064_i1_native_type1_fractional_coordinate_matches_freetype() {
    // 101 2 div produces an x coordinate of 50.5 font units.
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 240, 141, 12, 12, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-fraction-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    for ppem in [8, 16, 32] {
        let factor = Fixed(div_fix(ppem * 64, 1000));
        let mut commands = Vec::new();
        let (_, advance) = draw_type1_native(
            &face,
            1,
            Scale::Scaled {
                x: factor,
                y: factor,
            },
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
            "ppem {ppem}"
        );
    }
    std::fs::remove_file(path).unwrap();
}
