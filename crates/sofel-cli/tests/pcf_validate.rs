//! sofel ADR 0065 I2: PCF validation reports table directory damage.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const PCF: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.pcf");

fn validate(data: &[u8]) -> Result<(i32, String), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-pcf-validate-{}-{}.pcf",
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
        output.status.code().ok_or("missing status")?,
        String::from_utf8(output.stdout)?,
    ))
}

#[test]
fn adr0065_i1_pcf_signature_precedes_bare_cff() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = validate(PCF)?;
    assert_eq!(status, 0, "{output}");
    assert!(output.is_empty());
    Ok(())
}

#[test]
fn adr0065_i2_pcf_reports_bitmap_table_offset_field() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = PCF.to_vec();
    let record = (0..9)
        .map(|i| 8 + 16 * i)
        .find(|at| data.get(*at) == Some(&8))
        .ok_or("missing bitmap table")?;
    let offset = record + 12;
    data.get_mut(offset..offset + 4)
        .ok_or("missing offset field")?
        .copy_from_slice(&u32::MAX.to_le_bytes());
    let (status, output) = validate(&data)?;
    assert_eq!(status, 1);
    assert!(output.contains("\"table\":\"PCF \""), "{output}");
    assert!(output.contains(&format!("\"offset\":{offset}")), "{output}");
    assert!(!output.contains("cff"));
    Ok(())
}

#[test]
fn adr0065_i2_pcf_reports_truncated_directory() -> Result<(), Box<dyn std::error::Error>> {
    let (status, output) = validate(&PCF[..10])?;
    assert_eq!(status, 1);
    assert!(output.contains("\"table\":\"PCF \""), "{output}");
    assert!(output.contains("\"offset\":8"), "{output}");
    Ok(())
}
