//! PostScript native loading compared with pinned FreeType (sofel ADR 0064 I1).
#![allow(clippy::unwrap_used)]

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod triangle;

use sofel_conformance::paths;
use sofel_font::{CidFace, Font, HbScale, Type1Face, Type1Source};
use sofel_geom::{Command, Fixed, FnPen, div_fix};
use sofel_outline::{HintTarget, Scale, draw_cid_native, draw_type1_native};
use sofel_shape::{
    Scale as ShapeScale, SegmentProperties, ShapePlan, SubFont, UnicodeBuffer, shape_with_subfont,
};

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
fn adr0064_i1_type1_nondefault_font_matrix_matches_freetype() {
    let program = [
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
    ];
    let (source, _) = triangle::with_glyph_program(&program);
    let old = b"/FontMatrix [0.001 0 0 0.001 0 0]";
    for (case, new) in [
        b"/FontMatrix [0.002 0 0 0.002 0 0]".as_slice(),
        b"/FontMatrix [0.002 0 0 0.001 0 0]".as_slice(),
        b"/FontMatrix [0.001 0.0005 0 0.001 0 0]".as_slice(),
        b"/FontMatrix [0.001 0 0.0005 0.001 0 0]".as_slice(),
        b"/FontMatrix [0.001 0 0 0.001 0.05 0.02]".as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bytes = source.clone();
        let at = bytes
            .windows(old.len())
            .position(|window| window == old)
            .unwrap();
        bytes.splice(at..at + old.len(), new.iter().copied());
        let path = std::env::temp_dir().join(format!(
            "sofel-adr0064-type1-matrix-{case}-{}.pfa",
            std::process::id()
        ));
        std::fs::write(&path, &bytes).unwrap();
        for ppem in [8, 16, 32] {
            for engine in ["native", "none"] {
                for target in ["normal", "light", "mono", "lcd", "lcd-v"] {
                    let size = ppem.to_string();
                    let reference = std::process::Command::new(paths::ft_dump())
                        .args(["--hint-slot", &size, "1", engine, target])
                        .arg(&path)
                        .output()
                        .unwrap();
                    assert!(
                        reference.status.success(),
                        "FT case {case}, ppem {ppem}, engine {engine}, target {target}: {}",
                        String::from_utf8_lossy(&reference.stderr)
                    );
                    let args = [
                        "--engine",
                        engine,
                        "--ppem",
                        &size,
                        "--glyph",
                        "1",
                        "--target",
                        target,
                        path.to_str().unwrap(),
                    ]
                    .map(std::ffi::OsString::from);
                    let (mut actual, mut error) = (Vec::new(), Vec::new());
                    assert_eq!(
                        sofel_cli::hint::run(&args, &mut actual, &mut error),
                        0,
                        "case {case}: {}",
                        String::from_utf8_lossy(&error)
                    );
                    assert_eq!(
                        actual, reference.stdout,
                        "case {case}, ppem {ppem}, engine {engine}, target {target}"
                    );
                }
            }
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn adr0065_i1_type1_nondefault_matrix_shaping_advance_matches_freetype() {
    let (mut bytes, _) = triangle::sample();
    let old = b"/FontMatrix [0.001 0 0 0.001 0 0]";
    let new = b"/FontMatrix [0.002 0 0 0.002 0 0]";
    let at = bytes
        .windows(old.len())
        .position(|window| window == old)
        .unwrap();
    bytes.splice(at..at + old.len(), new.iter().copied());
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-type1-shape-matrix-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let source = Type1Source::parse_pfa(&bytes, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let sfnt = Font::for_shaping(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0).unwrap();
    let instance = sfnt.default_instance();
    let mut input = UnicodeBuffer::new();
    input.push_utf8(b"A", 0, None);
    input.number_clusters();
    let plan = ShapePlan::new(
        instance,
        SegmentProperties::default().guess(&input, None),
        &[],
    );
    let subfont = SubFont::from_type1(
        ShapeScale::new(HbScale::new(20 * 64, 20 * 64, 1000), 20, 20),
        &face,
    );
    let shaped = shape_with_subfont(instance, &subfont, &plan, input);
    let actual = shaped.glyph_positions().first().unwrap().x_advance;
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "20", "1", "none", "normal"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let expected = String::from_utf8(reference.stdout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_ascii_whitespace()
        .nth(1)
        .unwrap()
        .parse::<i32>()
        .unwrap();
    assert_eq!(actual, expected);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0065_i1_type1_shear_extents_use_transformed_points() {
    let program = [
        139, 248, 136, 13, 239, 239, 21, 247, 92, 139, 5, 39, 247, 92, 5, 9, 14,
    ];
    let (mut bytes, _) = triangle::with_glyph_program(&program);
    let old = b"/FontMatrix [0.001 0 0 0.001 0 0]";
    let new = b"/FontMatrix [0.001 0 0.0005 0.001 0 0]";
    let at = bytes
        .windows(old.len())
        .position(|window| window == old)
        .unwrap();
    bytes.splice(at..at + old.len(), new.iter().copied());
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-type1-shear-extents-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let source = Type1Source::parse_pfa(&bytes, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let scale = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let info = sofel_outline::draw_type1(
        &face,
        1,
        Scale::Scaled { x: scale, y: scale },
        &mut FnPen(|_| {}),
    )
    .unwrap();
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "none", "normal"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let points = String::from_utf8(reference.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_ascii_whitespace();
            (parts.next() == Some("point")).then(|| {
                (
                    parts.next().unwrap().parse::<i32>().unwrap(),
                    parts.next().unwrap().parse::<i32>().unwrap(),
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(points.len(), 3);
    assert_eq!(
        (
            info.cbox.x_min,
            info.cbox.y_min,
            info.cbox.x_max,
            info.cbox.y_max
        ),
        (
            points.iter().map(|point| point.0).min().unwrap(),
            points.iter().map(|point| point.1).min().unwrap(),
            points.iter().map(|point| point.0).max().unwrap(),
            points.iter().map(|point| point.1).max().unwrap(),
        )
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0065_i1_type1_nondefault_matrix_attached_kerning_matches_freetype() {
    let root = paths::testdata().join("extra-fonts/fonts");
    let mut bytes = std::fs::read(root.join("cmr10.pfb")).unwrap();
    let old = b"/FontMatrix [0.001 0 0 0.001 0 0 ";
    let new = b"/FontMatrix [0.002 0 0 0.002 0 0 ";
    let at = bytes
        .windows(old.len())
        .position(|window| window == old)
        .unwrap();
    bytes.splice(at..at + old.len(), new.iter().copied());
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-type1-kerning-matrix-{}.pfb",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let source = Type1Source::parse_pfb(&bytes, bytes.len().saturating_mul(8)).unwrap();
    let mut face = Type1Face::new(&source).unwrap();
    assert_eq!(face.units_per_em(), 500);
    let afm_data = std::fs::read(root.join("cmr10.afm")).unwrap();
    let pfm_data = std::fs::read(root.join("cmr10.pfm")).unwrap();
    for suffix in ["afm", "pfm"] {
        let attached = root.join(format!("cmr10.{suffix}"));
        if suffix == "afm" {
            face.attach_afm(&afm_data).unwrap();
        } else {
            face.attach_pfm(&pfm_data).unwrap();
        }
        let reference = std::process::Command::new(paths::ft_dump())
            .args(["--legacy-kerning", path.to_str().unwrap()])
            .arg(&attached)
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let pairs = String::from_utf8(reference.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_ascii_whitespace();
                (fields.next() == Some("kern")).then(|| {
                    (
                        fields.next().unwrap().parse::<usize>().unwrap(),
                        fields.next().unwrap().parse::<usize>().unwrap(),
                        fields.next().unwrap().parse::<i32>().unwrap(),
                        fields.next().unwrap().parse::<i32>().unwrap(),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(pairs.len(), 181);
        for (left, right, x, y) in pairs {
            assert_eq!(face.kerning(left, right), (x, y), "{suffix} {left} {right}");
        }
    }
    std::fs::remove_file(path).unwrap();
}

fn cid_sample_with_blue_values(program: &[u8]) -> Vec<u8> {
    let mut source = cid_sample(program);
    let old = b"/Private 1 dict begin /lenIV -1 def end";
    let new = b"/Private 2 dict begin /lenIV -1 def /BlueValues [0 40] def end";
    let at = source
        .windows(old.len())
        .position(|window| window == old)
        .unwrap();
    source.splice(at..at.saturating_add(old.len()), new.iter().copied());
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
fn adr0064_i1_native_cid_duplicate_stems_match_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 1, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39,
        39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-duplicate-stems-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_vertical_stem_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 3, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-vstem-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_two_horizontal_stems_match_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 1, 247, 92, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5,
        39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-two-hstems-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_nonunit_font_matrix_with_stem_matches_freetype() {
    let program = [
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ];
    let old = b"/FontMatrix [0.001 0 0 0.001 0 0]";
    for (case, new) in [
        b"/FontMatrix [0.002 0 0 0.001 0 0]".as_slice(),
        b"/FontMatrix [0.001 0 0 0.002 0 0]".as_slice(),
        b"/FontMatrix [0.001 0.0005 0 0.001 0 0]".as_slice(),
        b"/FontMatrix [0.001 0 0.0005 0.001 0 0]".as_slice(),
        b"/FontMatrix [0.001 0 0 0.001 0.05 0.02]".as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bytes = cid_sample(&program);
        let at = bytes
            .windows(old.len())
            .position(|window| window == old)
            .unwrap();
        bytes.splice(at..at + old.len(), new.iter().copied());
        let path = std::env::temp_dir().join(format!(
            "sofel-adr0064-cid-matrix-hstem-{case}-{}.cid",
            std::process::id()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let face = CidFace::parse(&bytes, 1024).unwrap();
        for ppem in [8, 16, 32] {
            for (name, target) in [
                ("mono", HintTarget::Mono),
                ("normal", HintTarget::Normal),
                ("light", HintTarget::Light),
                ("lcd", HintTarget::Lcd),
                ("lcd-v", HintTarget::VerticalLcd),
            ] {
                let factor = Fixed(div_fix(ppem * 64, i32::from(face.units_per_em())));
                let mut commands = Vec::new();
                let loaded = draw_cid_native(
                    &face,
                    0,
                    Scale::Scaled {
                        x: factor,
                        y: factor,
                    },
                    target,
                    &mut FnPen(|c| commands.push(c)),
                );
                let reference = std::process::Command::new(paths::ft_dump())
                    .args(["--hint-slot", &ppem.to_string(), "0", "native", name])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                let (_, advance) = loaded.unwrap();
                let mut points = Vec::new();
                for command in commands {
                    match command {
                        Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
                        Command::Close => {
                            if points.last() == points.first() {
                                points.pop();
                            }
                        }
                        other => assert_eq!(other, Command::Close),
                    }
                }
                let mut actual = format!("advance {advance} 0\n");
                for point in &points {
                    actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
                }
                actual.push_str(&format!("end {}\n", points.len() - 1));
                assert_eq!(
                    actual,
                    String::from_utf8(reference.stdout).unwrap(),
                    "case {case}, ppem {ppem}, target {name}"
                );
            }
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn adr0064_i1_native_cid_multiple_pairs_in_one_hstem_match_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39,
        5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-paired-hstems-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let loaded = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_late_hstem_without_new_contour_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 239, 189, 1, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-late-hstem-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let loaded = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_mid_hstem3_without_new_contour_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 239, 189, 247, 92, 189, 247, 192, 189, 12, 2,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-mid-hstem3-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let loaded = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_mid_vstem3_without_new_contour_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 239, 189, 247, 92, 189, 247, 192, 189, 12, 1,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-mid-vstem3-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let loaded = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_late_hstem_before_next_contour_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 247, 192, 189, 1,
        139, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-contour-hstem-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let loaded = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut actual = format!("advance {advance} 0\n");
    let (mut contour, mut ends, mut count) = (Vec::new(), Vec::new(), 0usize);
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => contour.push(point),
            Command::Close => {
                if contour.last() == contour.first() {
                    contour.pop();
                }
                for point in &contour {
                    actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
                }
                count += contour.len();
                ends.push(count - 1);
                contour.clear();
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    for end in ends {
        actual.push_str(&format!("end {end}\n"));
    }
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_blue_values_capture_stem() {
    let program = &[
        139, 248, 136, 13, 169, 149, 1, 139, 169, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
    ];
    let bytes = cid_sample_with_blue_values(program);
    let path =
        std::env::temp_dir().join(format!("sofel-adr0064-cid-blue-{}.cid", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let plain_path = path.with_file_name(format!(
        "sofel-adr0064-cid-no-blue-{}.cid",
        std::process::id()
    ));
    std::fs::write(&plain_path, cid_sample(program)).unwrap();
    let without_blue = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&plain_path)
        .output()
        .unwrap();
    assert!(without_blue.status.success());
    assert_ne!(reference.stdout, without_blue.stdout);
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(plain_path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_hstem3_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 247, 92, 189, 247, 192, 189, 12, 2, 239, 239, 21, 239, 139, 5,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-hstem3-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_vstem3_matches_freetype() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 247, 92, 189, 247, 192, 189, 12, 1, 239, 239, 21, 239, 139, 5,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-vstem3-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_vstem3_with_blue_values_matches_freetype() {
    let bytes = cid_sample_with_blue_values(&[
        139, 248, 136, 13, 239, 189, 247, 92, 189, 247, 192, 189, 12, 1, 239, 239, 21, 239, 139, 5,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-vstem3-blues-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "0", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let face = CidFace::parse(&bytes, 1024).unwrap();
    let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
    let mut commands = Vec::new();
    let (_, advance) = draw_cid_native(
        &face,
        0,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_other_subr3_replaces_hints() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 140, 142, 12, 16, 12, 17,
        12, 18, 247, 92, 189, 1, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-replacement-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    for ppem in [8, 16, 32] {
        for (name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let reference = std::process::Command::new(paths::ft_dump())
                .args(["--hint-slot", &ppem.to_string(), "0", "native", name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            let factor = Fixed(div_fix(ppem * 64, i32::from(face.units_per_em())));
            let mut commands = Vec::new();
            let (_, advance) = draw_cid_native(
                &face,
                0,
                Scale::Scaled {
                    x: factor,
                    y: factor,
                },
                target,
                &mut FnPen(|command| commands.push(command)),
            )
            .unwrap();
            let mut points = Vec::new();
            for command in commands {
                match command {
                    Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
                    Command::Close => {
                        if points.last() == points.first() {
                            points.pop();
                        }
                    }
                    other => assert_eq!(other, Command::Close),
                }
            }
            let mut actual = format!("advance {advance} 0\n");
            for point in &points {
                actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
            }
            actual.push_str(&format!("end {}\n", points.len() - 1));
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "{name} {ppem}"
            );
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_other_subr3_starts_with_empty_map() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 140, 142, 12, 16, 12, 17, 12, 18, 247,
        92, 189, 1, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-empty-replacement-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    for ppem in [8, 16, 32] {
        for (name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let reference = std::process::Command::new(paths::ft_dump())
                .args(["--hint-slot", &ppem.to_string(), "0", "native", name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            let factor = Fixed(div_fix(ppem * 64, i32::from(face.units_per_em())));
            let mut commands = Vec::new();
            let (_, advance) = draw_cid_native(
                &face,
                0,
                Scale::Scaled {
                    x: factor,
                    y: factor,
                },
                target,
                &mut FnPen(|command| commands.push(command)),
            )
            .unwrap();
            let mut points = Vec::new();
            for command in commands {
                match command {
                    Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
                    Command::Close => {
                        if points.last() == points.first() {
                            points.pop();
                        }
                    }
                    other => assert_eq!(other, Command::Close),
                }
            }
            let mut actual = format!("advance {advance} 0\n");
            for point in &points {
                actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
            }
            actual.push_str(&format!("end {}\n", points.len() - 1));
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "{name} {ppem}"
            );
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_stems_match_freetype_all_targets() {
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-cid-stems-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    for (name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
        ("lcd", HintTarget::Lcd),
        ("lcd-v", HintTarget::VerticalLcd),
    ] {
        let factor = Fixed(div_fix(16 * 64, i32::from(face.units_per_em())));
        let mut commands = Vec::new();
        let (_, advance) = draw_cid_native(
            &face,
            0,
            Scale::Scaled {
                x: factor,
                y: factor,
            },
            target,
            &mut FnPen(|c| commands.push(c)),
        )
        .unwrap();
        let mut points = Vec::new();
        for command in commands {
            match command {
                Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
                Command::Close => {
                    if points.last() == points.first() {
                        points.pop();
                    }
                }
                other => assert_eq!(other, Command::Close),
            }
        }
        let mut actual = format!("advance {advance} 0\n");
        for point in &points {
            actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
        }
        actual.push_str(&format!("end {}\n", points.len() - 1));
        let reference = std::process::Command::new(paths::ft_dump())
            .args(["--hint-slot", "16", "0", "native", name])
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        assert_eq!(
            actual,
            String::from_utf8(reference.stdout).unwrap(),
            "{name}"
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_cid_single_hstem_matches_freetype() {
    // The contour's bottom and top coincide with the single hstem.
    let bytes = cid_sample(&[
        139, 248, 136, 13, 239, 239, 1, 239, 239, 3, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39,
        139, 5, 139, 39, 5, 9, 14,
    ]);
    let path =
        std::env::temp_dir().join(format!("sofel-adr0064-cid-stem-{}.cid", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    for ppem in [8, 16, 32] {
        let factor = Fixed(div_fix(ppem * 64, i32::from(face.units_per_em())));
        let mut commands = Vec::new();
        let (_, advance) = draw_cid_native(
            &face,
            0,
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
            .args(["--hint-slot", &ppem.to_string(), "0", "native", "light"])
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
fn adr0064_i1_native_type1_hstem3_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 247, 92, 189, 247, 192, 189, 12, 2, 239, 239, 21, 239, 139, 5,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-hstem3-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_vstem3_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 247, 92, 189, 247, 192, 189, 12, 1, 239, 239, 21, 239, 139, 5,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-vstem3-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_vstem3_with_blue_values_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_name_and_blues(
        &[
            139, 248, 136, 13, 239, 189, 247, 92, 189, 247, 192, 189, 12, 1, 239, 239, 21, 239,
            139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
        ],
        "A",
        "[0 40]",
    );
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-vstem3-blues-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_other_subr3_replaces_hints() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 140, 142, 12, 16, 12, 17,
        12, 18, 247, 92, 189, 1, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-replacement-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    for ppem in [8, 16, 32] {
        for (name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let reference = std::process::Command::new(paths::ft_dump())
                .args(["--hint-slot", &ppem.to_string(), "1", "native", name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            let factor = Fixed(div_fix(ppem * 64, 1000));
            let mut commands = Vec::new();
            let (_, advance) = draw_type1_native(
                &face,
                1,
                Scale::Scaled {
                    x: factor,
                    y: factor,
                },
                target,
                &mut FnPen(|command| commands.push(command)),
            )
            .unwrap();
            let mut points = Vec::new();
            for command in commands {
                match command {
                    Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
                    Command::Close => {
                        if points.last() == points.first() {
                            points.pop();
                        }
                    }
                    other => assert_eq!(other, Command::Close),
                }
            }
            let mut actual = format!("advance {advance} 0\n");
            for point in &points {
                actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
            }
            actual.push_str(&format!("end {}\n", points.len() - 1));
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "{name} {ppem}"
            );
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_replacement_with_blues_matches_freetype() {
    let program = &[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 140, 142, 12, 16, 12, 17,
        12, 18, 247, 92, 189, 1, 139, 239, 5, 39, 139, 5, 9, 14,
    ];
    let (pfa, _) = triangle::with_glyph_name_and_blues(program, "A", "[100 150]");
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-replacement-blue-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_duplicate_stems_match_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 1, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39,
        39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-duplicate-stems-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_vertical_stem_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 3, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-vstem-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_two_horizontal_stems_match_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 1, 247, 92, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5,
        39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-two-hstems-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_multiple_pairs_in_one_hstem_match_freetype() {
    // Type 1's Adobe engine accepts delta-encoded stem pairs in one operator.
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39,
        5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-paired-hstems-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let loaded = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_late_hstem_without_new_contour_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 239, 189, 1, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-late-hstem-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let loaded = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_mid_hstem3_without_new_contour_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 239, 189, 247, 92, 189, 247, 192, 189, 12, 2,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-mid-hstem3-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let loaded = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_mid_vstem3_without_new_contour_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 239, 189, 247, 92, 189, 247, 192, 189, 12, 1,
        139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-mid-vstem3-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let loaded = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_late_hstem_before_next_contour_matches_freetype() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 247, 192, 189, 1,
        139, 239, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-contour-hstem-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let loaded = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    );
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (_, advance) = loaded.unwrap();
    let mut actual = format!("advance {advance} 0\n");
    let mut contour = Vec::new();
    let mut count = 0usize;
    let mut ends = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => contour.push(point),
            Command::Close => {
                if contour.last() == contour.first() {
                    contour.pop();
                }
                for point in &contour {
                    actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
                }
                count += contour.len();
                ends.push(count - 1);
                contour.clear();
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    for end in ends {
        actual.push_str(&format!("end {end}\n"));
    }
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_blue_values_capture_stem() {
    let program = &[
        139, 248, 136, 13, 169, 149, 1, 139, 169, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
    ];
    let (pfa, _) = triangle::with_glyph_name_and_blues(program, "A", "[0 40]");
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-blue-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let (plain, _) = triangle::with_glyph_program(program);
    let plain_path = path.with_file_name(format!(
        "sofel-adr0064-type1-no-blue-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&plain_path, plain).unwrap();
    let without_blue = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&plain_path)
        .output()
        .unwrap();
    assert!(without_blue.status.success());
    assert_ne!(reference.stdout, without_blue.stdout);
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(plain_path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_other_blues_capture_stem() {
    let program = &[
        139, 248, 136, 13, 109, 149, 1, 139, 109, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5, 9, 14,
    ];
    let (pfa, _) =
        triangle::with_glyph_name_and_blues(program, "A", "[] def /OtherBlues [-40 -20]");
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-other-blue-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(16 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|c| commands.push(c)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let (plain, _) = triangle::with_glyph_program(program);
    let plain_path = path.with_file_name(format!(
        "sofel-adr0064-type1-no-other-blue-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&plain_path, plain).unwrap();
    let without_blue = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", "1", "native", "mono"])
        .arg(&plain_path)
        .output()
        .unwrap();
    assert!(without_blue.status.success());
    assert_ne!(reference.stdout, without_blue.stdout);
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(plain_path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_explicit_blue_scale_matches_freetype() {
    let program = &[
        139, 248, 136, 13, 247, 1, 144, 1, 139, 247, 1, 21, 239, 139, 5, 139, 239, 5, 39, 139, 5,
        9, 14,
    ];
    let (pfa, _) =
        triangle::with_glyph_name_and_blues(program, "A", "[109 114] def /BlueScale 0.08");
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-blue-scale-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    let factor = Fixed(div_fix(50 * 64, 1000));
    let mut commands = Vec::new();
    let (_, advance) = draw_type1_native(
        &face,
        1,
        Scale::Scaled {
            x: factor,
            y: factor,
        },
        HintTarget::Mono,
        &mut FnPen(|command| commands.push(command)),
    )
    .unwrap();
    let mut points = Vec::new();
    for command in commands {
        match command {
            Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
            Command::Close => {
                if points.last() == points.first() {
                    points.pop();
                }
            }
            other => assert_eq!(other, Command::Close),
        }
    }
    let mut actual = format!("advance {advance} 0\n");
    for point in &points {
        actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
    }
    actual.push_str(&format!("end {}\n", points.len() - 1));
    let reference = std::process::Command::new(paths::ft_dump())
        .args(["--hint-slot", "50", "1", "native", "mono"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0064_i1_native_type1_single_stem_matches_freetype_all_targets() {
    let (pfa, _) = triangle::with_glyph_program(&[
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-type1-stems-{}.pfa",
        std::process::id()
    ));
    std::fs::write(&path, &pfa).unwrap();
    let source = Type1Source::parse_pfa(&pfa, 4096).unwrap();
    let face = Type1Face::new(&source).unwrap();
    for (name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
        ("lcd", HintTarget::Lcd),
        ("lcd-v", HintTarget::VerticalLcd),
    ] {
        let factor = Fixed(div_fix(16 * 64, 1000));
        let mut commands = Vec::new();
        let (_, advance) = draw_type1_native(
            &face,
            1,
            Scale::Scaled {
                x: factor,
                y: factor,
            },
            target,
            &mut FnPen(|c| commands.push(c)),
        )
        .unwrap();
        let mut points = Vec::new();
        for command in commands {
            match command {
                Command::MoveTo(point) | Command::LineTo(point) => points.push(point),
                Command::Close => {
                    if points.last() == points.first() {
                        points.pop();
                    }
                }
                other => assert_eq!(other, Command::Close),
            }
        }
        let mut actual = format!("advance {advance} 0\n");
        for point in &points {
            actual.push_str(&format!("point {} {} 1\n", point.x, point.y));
        }
        actual.push_str(&format!("end {}\n", points.len() - 1));
        let reference = std::process::Command::new(paths::ft_dump())
            .args(["--hint-slot", "16", "1", "native", name])
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        assert_eq!(
            actual,
            String::from_utf8(reference.stdout).unwrap(),
            "{name}"
        );
    }
    std::fs::remove_file(path).unwrap();
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

#[test]
fn adr0064_i1_cli_type1_and_cid_slots_match_freetype() {
    let program = [
        139, 248, 136, 13, 239, 189, 1, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ];
    let (pfa, _) = triangle::with_glyph_program(&program);
    let cid = cid_sample(&program);
    for (kind, bytes, glyph) in [("type1", pfa, "1"), ("cid", cid, "0")] {
        let path = std::env::temp_dir().join(format!(
            "sofel-adr0064-cli-{kind}-{}.{}",
            std::process::id(),
            if kind == "type1" { "pfa" } else { "cid" }
        ));
        std::fs::write(&path, &bytes).unwrap();
        for engine in ["native", "none"] {
            for (name, _) in [
                ("normal", HintTarget::Normal),
                ("light", HintTarget::Light),
                ("mono", HintTarget::Mono),
                ("lcd", HintTarget::Lcd),
                ("lcd-v", HintTarget::VerticalLcd),
            ] {
                let args = [
                    "--engine",
                    engine,
                    "--ppem",
                    "16",
                    "--glyph",
                    glyph,
                    "--target",
                    name,
                    path.to_str().unwrap(),
                ]
                .map(std::ffi::OsString::from);
                let (mut actual, mut error) = (Vec::new(), Vec::new());
                assert_eq!(
                    sofel_cli::hint::run(&args, &mut actual, &mut error),
                    0,
                    "{kind} {name}: {}",
                    String::from_utf8_lossy(&error)
                );
                let reference = std::process::Command::new(paths::ft_dump())
                    .args(["--hint-slot", "16", glyph, engine, name])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(reference.status.success());
                assert_eq!(actual, reference.stdout, "{kind} {name}");
            }
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn adr0064_i1_native_empty_cid_with_leniv_matches_freetype() {
    let mut bytes = cid_sample(&[]);
    let old = b"/lenIV -1";
    let new = b"/lenIV 4";
    let at = bytes
        .windows(old.len())
        .position(|window| window == old)
        .unwrap();
    bytes.splice(at..at + old.len(), new.iter().copied());
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-empty-cid-{}.cid",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let face = CidFace::parse(&bytes, 1024).unwrap();
    assert_eq!(face.glyph_is_empty(0), Some(true));
    for name in ["normal", "light", "mono", "lcd", "lcd-v"] {
        let mut commands = Vec::new();
        let (_, advance) = draw_cid_native(
            &face,
            0,
            Scale::Scaled {
                x: Fixed(div_fix(16 * 64, i32::from(face.units_per_em()))),
                y: Fixed(div_fix(16 * 64, i32::from(face.units_per_em()))),
            },
            HintTarget::Normal,
            &mut FnPen(|command| commands.push(command)),
        )
        .unwrap();
        assert!(commands.is_empty());
        let actual = format!("advance {advance} 0\n");
        let reference = std::process::Command::new(paths::ft_dump())
            .args(["--hint-slot", "16", "0", "native", name])
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        assert_eq!(actual.as_bytes(), reference.stdout, "{name}");
        let args = [
            "--engine",
            "none",
            "--ppem",
            "16",
            "--glyph",
            "0",
            "--target",
            name,
            path.to_str().unwrap(),
        ]
        .map(std::ffi::OsString::from);
        let (mut cli, mut error) = (Vec::new(), Vec::new());
        assert_eq!(
            sofel_cli::hint::run(&args, &mut cli, &mut error),
            0,
            "{}",
            String::from_utf8_lossy(&error)
        );
        let unhinted = std::process::Command::new(paths::ft_dump())
            .args(["--hint-slot", "16", "0", "none", name])
            .arg(&path)
            .output()
            .unwrap();
        assert!(unhinted.status.success());
        assert_eq!(cli, unhinted.stdout, "{name}");
    }
    std::fs::remove_file(path).unwrap();
}
