//! sofel ADR 0065 I2: PFR validation reports the damaged byte range.

#[path = "support/pfr.rs"]
mod pfr;

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn validate(data: &[u8]) -> Result<(i32, String), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-pfr-validate-{}-{}.pfr",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, data)?;
    let output = Command::new(env!("CARGO_BIN_EXE_sofel-validate"))
        .arg("--json")
        .arg(&path)
        .output()?;
    std::fs::remove_file(path)?;
    Ok((
        output.status.code().ok_or("missing exit status")?,
        String::from_utf8(output.stdout)?,
    ))
}

#[test]
fn adr0065_i1_pfr_valid_file_has_no_errors() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(validate(&pfr::triangle())?.0, 0);
    assert_eq!(validate(&pfr::bitmap_only())?.0, 0);
    Ok(())
}

#[test]
fn adr0065_i2_pfr_reports_glyph_program_size_field() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = pfr::triangle();
    *data.get_mut(113).ok_or("missing glyph size field")? = u8::MAX;
    let (status, output) = validate(&data)?;
    assert_eq!(status, 1);
    assert!(output.contains("\"table\":\"PFR \""), "{output}");
    assert!(output.contains("\"offset\":113"), "{output}");
    Ok(())
}

#[test]
fn adr0065_i2_pfr_reports_directory_offset_field() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = pfr::triangle();
    data.get_mut(12..14)
        .ok_or("missing directory offset field")?
        .copy_from_slice(&u16::MAX.to_be_bytes());
    let (status, output) = validate(&data)?;
    assert_eq!(status, 1);
    assert!(output.contains("\"offset\":12"), "{output}");
    Ok(())
}

#[test]
fn adr0065_i2_pfr_reports_zero_units_field() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = pfr::triangle();
    data.get_mut(85..87).ok_or("missing units field")?.fill(0);
    let (status, output) = validate(&data)?;
    assert_eq!(status, 1);
    assert!(output.contains("\"offset\":85"), "{output}");
    Ok(())
}
