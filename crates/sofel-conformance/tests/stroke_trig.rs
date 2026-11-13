//! Fixed FreeType CORDIC primitives used by sofel ADR 0076.
#![allow(clippy::all, clippy::unwrap_used)]
#[path = "../../sofel-outline/src/stroke_trig.rs"]
mod stroke_trig;

use sofel_conformance::paths;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn inv1_stroke_cordic_matches_freetype() {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let angles = [
        -11_796_480,
        -7_864_320,
        -2_949_120,
        0,
        2_949_120,
        7_864_320,
        11_796_480,
    ];
    let mut lines = Vec::new();
    let mut ours = Vec::new();
    for (x, y) in [
        (-1, 2),
        (0, 1),
        (1, 0),
        (100, 200),
        (-100, 200),
        (555, -123),
    ] {
        lines.push(format!("atan2 {x} {y}"));
        ours.push(stroke_trig::atan2(x, y).to_string());
    }
    for angle in angles {
        lines.push(format!("unit {angle}"));
        let p = stroke_trig::unit(angle);
        ours.push(format!("{} {}", p.x, p.y));
        lines.push(format!("from_polar 100 {angle}"));
        let p = stroke_trig::from_polar(100, angle);
        ours.push(format!("{} {}", p.x, p.y));
        lines.push(format!("tan {angle}"));
        ours.push(stroke_trig::tan(angle).to_string());
        lines.push(format!("angle_diff 0 {angle}"));
        ours.push(stroke_trig::angle_diff(0, angle).to_string());
    }
    let mut child = Command::new(exe)
        .arg("--calc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(lines.join("\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let reference = String::from_utf8(output.stdout).unwrap();
    let actual = ours.join("\n");
    assert_eq!(actual, reference.trim_end());
    assert_eq!(ours.len(), 34);
}
