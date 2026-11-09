//! sofel-geom's arithmetic against FreeType's, through `ft-dump --calc`
//! (ADR 0009). FreeType computes in `long`; where its result does not fit
//! in i32, sofel must give the saturated value.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::io::Write;
use std::process::{Command, Stdio};

use sofel_conformance::paths;
use sofel_geom::{div_fix, mul_div, mul_fix, vector_length};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn i32(&mut self) -> i32 {
        let v = self.next();
        let bits = (v % 32) as u32;
        let m = (self.next() as u32) >> (31 - bits.min(31));
        if v & (1 << 40) != 0 {
            -(m as i32)
        } else {
            m as i32
        }
    }
}

const EDGES: [i32; 12] = [
    0,
    1,
    -1,
    2,
    0x7FFF,
    0x8000,
    -0x8000,
    0x1_0000,
    -0x1_0000,
    i32::MAX,
    i32::MIN + 1,
    1000,
];

fn sat(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[test]
fn arithmetic_agrees_with_freetype() {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));

    let mut cases: Vec<(String, i32)> = Vec::new();
    let mut add = |op: &str, args: &[i32], ours: i32| {
        let args: Vec<String> = args.iter().map(i32::to_string).collect();
        cases.push((format!("{op} {}", args.join(" ")), ours));
    };
    let mut r = XorShift(7);
    let mut inputs: Vec<(i32, i32, i32)> = Vec::new();
    for &a in &EDGES {
        for &b in &EDGES {
            for &c in &EDGES {
                inputs.push((a, b, c));
            }
        }
    }
    for _ in 0..30_000 {
        inputs.push((r.i32(), r.i32(), r.i32()));
    }
    for &(a, b, c) in &inputs {
        add("mul_fix", &[a, b], mul_fix(a, b));
        add("div_fix", &[a, b], div_fix(a, b));
        add("mul_div", &[a, b, c], mul_div(a, b, c));
        // FT_Vector_Length is used on component scales (FT_Hypot); keep the
        // inputs where FreeType's prenormalization is defined.
        if a != i32::MIN && b != i32::MIN {
            add("length", &[a, b], vector_length(a, b));
        }
    }

    let mut child = Command::new(&exe)
        .arg("--calc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Write in another thread: ft-dump answers line by line, and its
    // output would fill the pipe while we are still writing.
    let mut stdin = child.stdin.take().unwrap();
    let input: String = cases.iter().map(|(l, _)| format!("{l}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    assert!(out.status.success());
    let results: Vec<i64> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| l.parse().unwrap())
        .collect();
    assert_eq!(results.len(), cases.len());

    let mismatches: Vec<String> = cases
        .iter()
        .zip(&results)
        .filter(|((_, ours), ft)| *ours != sat(**ft))
        .map(|((line, ours), ft)| format!("{line}: FreeType {ft}, sofel {ours}"))
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} of {} differ:\n{}",
        mismatches.len(),
        cases.len(),
        mismatches[..mismatches.len().min(20)].join("\n")
    );
}
