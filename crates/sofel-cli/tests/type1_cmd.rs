//! sofel ADR 0065 I1: Type 1 PFA and PFB through the ordinary renderer.

use std::process::Command;

#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod fixture;

#[test]
fn adr0065_i1_type1_pfa_pfb_render_like_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let (pfa, pfb) = fixture::sample();
    for (extension, bytes, target) in [("pfa", pfa, "--char"), ("pfb", pfb, "--glyph")] {
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
