//! sofel ADR 0065 I1: CID Type 1 through the ordinary render command.
#![allow(clippy::unwrap_used)]

use std::process::Command;

#[test]
fn adr0065_i1_cid_triangle_renders_like_freetype() {
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
    let path = std::env::temp_dir().join(format!("sofel-cid-{}.ps", std::process::id()));
    std::fs::write(&path, &source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(["--ppem", "100", "--glyph", "0"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
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
    let validation = Command::new(env!("CARGO_BIN_EXE_sofel-validate"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(validation.status.code(), Some(0));
    assert!(validation.stdout.is_empty());
    let old = b"[0.001 0 0 0.001 0 0]";
    let at = source.windows(old.len()).position(|w| w == old).unwrap();
    source
        .get_mut(at..at.saturating_add(old.len()))
        .unwrap()
        .copy_from_slice(b"[0.002 0 0 0.002 0 0]");
    std::fs::write(&path, source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(["--ppem", "100", "--glyph", "0"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert!(
        result
            .stdout
            .starts_with(b"P5\n# left 20 top 40\n20 20\n255\n")
    );
}

#[test]
fn adr0065_i2_cid_validator_reports_truncated_map_position() {
    let mut source = b"%!PS-Adobe-3.0 Resource-CIDFont\n\
/CIDFontType 0 def\n/CIDCount 1 def\n/FDBytes 1 def\n/GDBytes 2 def\n/CIDMapOffset 0 def\n\
/FDArray 1 array\n%ADOBeginFontDict\n/FontMatrix [0.001 0 0 0.001 0 0] def\n\
/Private 1 dict begin /lenIV -1 def end\n%ADOEndFontDict\n\
(Binary) 2 StartData\n"
        .to_vec();
    source.extend_from_slice(&[0, 0]);
    let path = std::env::temp_dir().join(format!("sofel-cid-broken-{}.ps", std::process::id()));
    std::fs::write(&path, &source).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_sofel-validate"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(result.status.code(), Some(1));
    let json = String::from_utf8(result.stdout).unwrap();
    assert!(json.contains("\"table\":\"CID \""), "{json}");
    assert!(
        json.contains(&format!("\"offset\":{}", source.len())),
        "{json}"
    );
}
