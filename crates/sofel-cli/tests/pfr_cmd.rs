use std::process::Command;

#[path = "support/pfr.rs"]
mod fixture;

#[test]
fn adr0065_i1_pfr_renders_glyph_and_character() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("sofel-pfr-{}.pfr", std::process::id()));
    std::fs::write(&path, fixture::triangle())?;
    let run = |target: [&str; 2]| -> Result<_, Box<dyn std::error::Error>> {
        Ok(Command::new(env!("CARGO_BIN_EXE_sofel-render"))
            .args(["--ppem", "100", target[0], target[1]])
            .arg(&path)
            .output()?)
    };
    let glyph = run(["--glyph", "1"])?;
    let character = run(["--char", "U+0041"])?;
    std::fs::remove_file(path)?;
    assert_eq!(
        glyph.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&glyph.stderr)
    );
    assert_eq!(character.status.code(), Some(0));
    assert_eq!(glyph.stdout, character.stdout);
    assert!(glyph.stdout.starts_with(b"P5\n# left 0 top 6\n8 6\n255\n"));
    Ok(())
}

#[test]
fn adr0065_i1_pfr_renders_embedded_bitmap() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("sofel-pfr-bitmap-{}.pfr", std::process::id()));
    std::fs::write(&path, fixture::bitmap_only())?;
    let output = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(["--ppem", "12", "--glyph", "1"])
        .arg(&path)
        .output()?;
    std::fs::remove_file(path)?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = b"P5\n# left 0 top 2\n3 2\n255\n".to_vec();
    expected.extend_from_slice(&[255, 0, 255, 0, 255, 0]);
    assert_eq!(output.stdout, expected);
    Ok(())
}

#[test]
fn adr0065_i2_pfr_rejects_out_of_range_face() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!("sofel-pfr-face-{}.pfr", std::process::id()));
    std::fs::write(&path, fixture::triangle())?;
    let output = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(["--face", "1", "--ppem", "100", "--glyph", "1"])
        .arg(&path)
        .output()?;
    std::fs::remove_file(path)?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid PFR font or face"));
    Ok(())
}
