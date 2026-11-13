//! sofel ADR 0065 I1: Type 1 PFA and PFB through the ordinary renderer.

use std::process::Command;

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod fixture;

#[test]
fn adr0065_i1_type1_pfa_pfb_render_like_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let (pfa, pfb) = fixture::sample();
    let fonttype_header = |mut font: Vec<u8>, offset: usize| -> Option<Vec<u8>> {
        font.get_mut(offset..offset.checked_add(14)?)?
            .copy_from_slice(b"%!FontType1   ");
        Some(font)
    };
    for (extension, bytes, target) in [
        (
            "pfa",
            fonttype_header(pfa.clone(), 0).ok_or("short PFA header")?,
            "--char",
        ),
        (
            "pfb",
            fonttype_header(pfb.clone(), 6).ok_or("short PFB header")?,
            "--glyph",
        ),
        ("pfa", pfa, "--char"),
        ("pfb", pfb, "--glyph"),
    ] {
        let path =
            std::env::temp_dir().join(format!("sofel-type1-{}.{}", std::process::id(), extension));
        std::fs::write(&path, bytes)?;
        let result = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
            .args([
                "--ppem",
                "100",
                target,
                if target == "--char" { "U+0041" } else { "1" },
            ])
            .arg(&path)
            .output()?;
        std::fs::remove_file(path)?;
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut expected = b"P5\n# left 10 top 20\n10 10\n255\n".to_vec();
        for row in 0..10usize {
            for column in 0..10usize {
                let coverage = if column.saturating_add(row) < 9 {
                    0
                } else if column.saturating_add(row) == 9 {
                    128
                } else {
                    255
                };
                expected.push(!coverage);
            }
        }
        assert_eq!(result.stdout, expected);
    }
    Ok(())
}

#[test]
fn adr0065_i1_type1_shapes_and_renders_text() -> Result<(), Box<dyn std::error::Error>> {
    let (pfa, _) = fixture::sample();
    let path = std::env::temp_dir().join(format!("sofel-type1-text-{}.pfa", std::process::id()));
    std::fs::write(&path, pfa)?;
    let result = Command::new(env!("CARGO_BIN_EXE_sofel"))
        .args(["render", "--ppem", "100", "--text", "A"])
        .arg(&path)
        .output()?;
    std::fs::remove_file(path)?;
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.starts_with(b"P5\n50 10\n255\n"));
    Ok(())
}

#[test]
fn adr0065_i1_type1_maps_unicode_beyond_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let program = [
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ];
    let (pfa, _) = fixture::with_glyph_name(&program, "Euro");
    let path = std::env::temp_dir().join(format!("sofel-type1-euro-{}.pfa", std::process::id()));
    std::fs::write(&path, pfa)?;
    let render = |target: [&str; 2]| -> Result<_, Box<dyn std::error::Error>> {
        Ok(Command::new(env!("CARGO_BIN_EXE_sofel-render"))
            .args(["--ppem", "100", target[0], target[1]])
            .arg(&path)
            .output()?)
    };
    let by_char = render(["--char", "U+20AC"])?;
    let by_glyph = render(["--glyph", "1"])?;
    std::fs::remove_file(path)?;
    assert_eq!(by_char.status.code(), Some(0));
    assert_eq!(by_char.stdout, by_glyph.stdout);
    Ok(())
}
