#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

fn run(bin: &str, args: &[&str]) -> Output {
    Command::new(bin).args(args).output().unwrap()
}

const BIN: &str = env!("CARGO_BIN_EXE_sofel");
const TOOLS: [(&str, &str); 5] = [
    ("shape", env!("CARGO_BIN_EXE_sofel-shape")),
    ("render", env!("CARGO_BIN_EXE_sofel-render")),
    ("paint", env!("CARGO_BIN_EXE_sofel-paint")),
    ("validate", env!("CARGO_BIN_EXE_sofel-validate")),
    ("subset", env!("CARGO_BIN_EXE_sofel-subset")),
];

#[test]
fn inv1_help_forms_and_inv2_examples() {
    let list = run(BIN, &[]);
    assert!(list.status.success() && list.stderr.is_empty());
    for arg in ["--help", "-h", "help"] {
        assert_eq!(run(BIN, &[arg]), list);
    }
    for (name, _) in TOOLS {
        assert!(String::from_utf8_lossy(&list.stdout).contains(name));
        let detail = run(BIN, &["help", name]);
        assert!(detail.status.success() && detail.stderr.is_empty());
        for flag in ["--help", "-h"] {
            assert_eq!(run(BIN, &[name, flag]), detail);
        }
        let text = String::from_utf8_lossy(&detail.stdout);
        assert!(text.contains(&format!("usage: sofel {name} ")));
        assert!(text.contains("Example"));
    }
    assert!(
        String::from_utf8_lossy(&run(BIN, &["help", "render"]).stdout)
            .contains("--text-file sample.txt")
    );
}

#[test]
fn inv3_command_parity() {
    for (name, binary) in TOOLS {
        for args in [&[][..], &["missing-example-font.ttf"][..]] {
            let mut unified = vec![name];
            unified.extend_from_slice(args);
            assert_eq!(run(BIN, &unified), run(binary, args));
        }
    }
}

#[test]
fn inv4_unknown_command() {
    for (args, bad) in [
        (&["renderer"][..], "renderer"),
        (&["help", "renderer"], "renderer"),
        (&["help", "render", "extra"], "extra"),
    ] {
        let output = run(BIN, args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(bad) && error.contains("sofel help"));
    }
}

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(
    dead_code,
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::panic,
    clippy::expect_used
)]
mod build;

#[test]
fn inv3_generated_files_and_output() {
    use build::*;
    let dir = std::env::temp_dir().join(format!("sofel-parity-{}", std::process::id()));
    let unified = dir.join("unified");
    let legacy = dir.join("legacy");
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 1000),
            Pt::On(1000, 1000),
            Pt::On(1000, 0),
        ]],
        false,
        &[],
    );
    let font = glyph_font_with(
        &[Vec::new(), square.clone(), square],
        true,
        false,
        vec![
            (b"COLR", colr_v0(&[(1, 0, 1)], &[(2, 0)])),
            (b"CPAL", cpal(1, &[0], &[[0, 0, 255, 255]], None)),
        ],
    );
    for d in [&unified, &legacy] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::write(d.join("font.ttf"), &font).unwrap();
    }
    for (name, bin) in TOOLS {
        let args: &[&str] = match name {
            "shape" => &["font.ttf", "A"],
            "render" => &["--ppem", "12", "--glyph", "1", "font.ttf"],
            "validate" => &["--json", "font.ttf"],
            "paint" => &["--glyph", "1", "font.ttf"],
            _ => &["--output-file=out.ttf", "font.ttf", "A"],
        };
        let old = Command::new(bin)
            .args(args)
            .current_dir(&legacy)
            .output()
            .unwrap();
        let new = Command::new(BIN)
            .arg(name)
            .args(args)
            .current_dir(&unified)
            .output()
            .unwrap();
        assert_eq!(new, old, "{name}");
        if name == "render" {
            assert_eq!(
                new.stdout,
                [&b"P5\n# left 0 top 12\n12 12\n255\n"[..], &[0; 144]].concat()
            );
        }
    }
    assert_eq!(
        std::fs::read(legacy.join("out.ttf")).unwrap(),
        std::fs::read(unified.join("out.ttf")).unwrap()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
