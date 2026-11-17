//! The `sofel-woff` command (sofel ADR 0070), moved to sofel-cli by
//! sofel ADR 0081.
#![allow(clippy::unwrap_used)]

#[test]
fn i3_webfont_error_keeps_existing_and_absent_output() {
    use std::{
        fs,
        process::{Command, Stdio},
    };
    let dir = std::env::temp_dir().join(format!("sofel-woff-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("result");
    for format in ["woff", "woff2"] {
        for exists in [false, true] {
            if exists {
                fs::write(&path, b"original").unwrap();
            }
            let result = Command::new(env!("CARGO_BIN_EXE_sofel-woff"))
                .args(["--format", format, "-", "-o"])
                .arg(&path)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            if exists {
                assert_eq!(fs::read(&path).unwrap(), b"original");
                fs::remove_file(&path).unwrap();
            } else {
                assert!(!path.exists());
            }
        }
        let out = Command::new(env!("CARGO_BIN_EXE_sofel-woff"))
            .args(["--format", format, "-", "-o", "-"])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    fs::remove_dir(dir).unwrap();
}
