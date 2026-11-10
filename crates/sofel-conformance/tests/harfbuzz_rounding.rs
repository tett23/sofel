//! HarfBuzz's rounding against `sofel_geom::float` (ADR 0026), through
//! `hb-dump --roundf` and `hb-dump --mult`: HarfBuzz's `roundf` macro
//! (`_hb_roundf` of `hb-algs.hh`) for `float` and `double` values, and
//! `hb_font_t`'s `x_mult` against `HbScale::x_mult`.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::io::Write;
use std::process::{Command, Stdio};

use sofel_conformance::paths;
use sofel_font::HbScale;
use sofel_geom::float::{hb_roundf, hb_roundf32};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// A signed integer of up to `bits` bits, small magnitudes as likely
    /// as large ones.
    fn int(&mut self, bits: u32) -> i64 {
        let n = (self.next() % u64::from(bits)) as u32;
        let m = (self.next() >> (64 - n.max(1))) as i64;
        if self.next() & 1 == 0 { m } else { -m }
    }
}

/// Runs `hb-dump MODE` with `input` on stdin; one output line per input
/// line.
fn hb_dump(mode: &str, input: String) -> Vec<String> {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let mut child = Command::new(&exe)
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Write in another thread: hb-dump answers line by line, and its
    // output would fill the pipe while we are still writing.
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    assert!(out.status.success(), "hb-dump {mode} failed");
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// A value to round, as HarfBuzz's `float` or `double`.
#[derive(Clone, Copy, Debug)]
enum Value {
    F32(f32),
    F64(f64),
}

impl Value {
    fn line(self) -> String {
        match self {
            Value::F32(v) => format!("f {:08x}\n", v.to_bits()),
            Value::F64(v) => format!("d {:016x}\n", v.to_bits()),
        }
    }
    fn ours(self) -> f64 {
        match self {
            Value::F32(v) => hb_roundf32(v),
            Value::F64(v) => hb_roundf(v),
        }
    }
}

/// The values of the unit tests, as both types.
fn edge_values() -> Vec<Value> {
    let f32_below_half = f32::from_bits(0.5f32.to_bits() - 1);
    let f64_below_half = f64::from_bits(0.5f64.to_bits() - 1);
    let mut d: Vec<f64> = vec![
        -2.5,
        -1.5,
        -0.5,
        -0.0,
        0.0,
        0.5,
        1.5,
        2.5,
        -494.5,
        3.0,
        -3.0,
        f64::from(f32_below_half),
        -f64::from(f32_below_half),
        8_388_607.5,
        -8_388_607.5,
        8_388_608.0,
        -8_388_609.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from(f32::MAX),
        f64::from(f32::MIN),
    ];
    let mut values: Vec<Value> = d.iter().map(|&v| Value::F32(v as f32)).collect();
    d.extend([
        f64_below_half,
        -f64_below_half,
        4_503_599_627_370_495.5,
        -4_503_599_627_370_495.5,
        4_503_599_627_370_496.0,
        9.0e18,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
    ]);
    values.extend(d.iter().map(|&v| Value::F64(v)));
    values
}

/// One million values from a fixed seed, most of them halves or next to
/// halves.
fn random_values() -> Vec<Value> {
    let mut r = XorShift(0x0026_2026_1110_0026);
    (0..1_000_000)
        .map(|i| {
            let half = r.int(53) as f64 + 0.5;
            match i % 8 {
                // Halves k + 0.5.
                0 => Value::F32((r.int(24) as f64 + 0.5) as f32),
                1 => Value::F64(half),
                // One unit in the last place around a half.
                2 => {
                    let h = (r.int(23) as f32 + 0.5).to_bits();
                    Value::F32(f32::from_bits(if r.next() & 1 == 0 {
                        h + 1
                    } else {
                        h - 1
                    }))
                }
                3 => {
                    let h = (r.int(51) as f64 + 0.5).to_bits();
                    Value::F64(f64::from_bits(if r.next() & 1 == 0 {
                        h + 1
                    } else {
                        h - 1
                    }))
                }
                // Quarters, small fractions and any bit pattern.
                4 => Value::F32(r.int(24) as f32 / 4.0),
                5 => Value::F64(r.int(40) as f64 / 64.0),
                6 => Value::F32(f32::from_bits(r.next() as u32)),
                _ => Value::F64(f64::from_bits(r.next())),
            }
        })
        .collect()
}

#[test]
fn roundf_agrees_with_harfbuzz() {
    let mut values = edge_values();
    values.extend(random_values());
    let input: String = values.iter().map(|v| v.line()).collect();
    let results = hb_dump("--roundf", input);
    assert_eq!(results.len(), values.len());
    let mismatches: Vec<String> = values
        .iter()
        .zip(&results)
        .filter_map(|(v, hb)| {
            let theirs = u64::from_str_radix(hb, 16).unwrap();
            let ours = v.ours().to_bits();
            // NaNs agree whatever their payload.
            let both_nan = f64::from_bits(theirs).is_nan() && f64::from_bits(ours).is_nan();
            (theirs != ours && !both_nan).then(|| {
                format!(
                    "{v:?}: HarfBuzz {:e} ({theirs:016x}), sofel {:e} ({ours:016x})",
                    f64::from_bits(theirs),
                    f64::from_bits(ours)
                )
            })
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} of {} differ:\n{}",
        mismatches.len(),
        values.len(),
        mismatches[..mismatches.len().min(20)].join("\n")
    );
}

#[test]
fn x_mult_agrees_with_harfbuzz() {
    let upems: [u16; 5] = [16, 1000, 1024, 2048, 16384];
    let mut scales: Vec<i32> = (-5000..=5000).collect();
    scales.extend([
        i32::MIN,
        i32::MIN + 1,
        i32::MAX,
        i32::MAX - 1,
        32_767,
        -32_768,
    ]);
    scales.extend([
        65_535,
        65_536,
        -65_536,
        1 << 24,
        -(1 << 24) - 1,
        123_456_789,
    ]);
    let mut r = XorShift(0x0016_0026);
    scales.extend((0..20_000).map(|_| r.int(32) as i32));
    let cases: Vec<(u16, i32)> = upems
        .iter()
        .flat_map(|&u| scales.iter().map(move |&s| (u, s)))
        .collect();
    let input: String = cases.iter().map(|(u, s)| format!("{u} {s}\n")).collect();
    let results = hb_dump("--mult", input);
    assert_eq!(results.len(), cases.len());
    let mismatches: Vec<String> = cases
        .iter()
        .zip(&results)
        .filter_map(|(&(upem, scale), hb)| {
            let theirs: i64 = hb.parse().unwrap();
            let ours = HbScale::new(scale, scale, upem);
            (theirs != ours.x_mult() || theirs != ours.y_mult()).then(|| {
                format!(
                    "upem {upem}, scale {scale}: HarfBuzz {theirs}, sofel {} {}",
                    ours.x_mult(),
                    ours.y_mult()
                )
            })
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} of {} differ:\n{}",
        mismatches.len(),
        cases.len(),
        mismatches[..mismatches.len().min(20)].join("\n")
    );
}
