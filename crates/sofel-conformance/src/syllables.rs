//! sofel-shape's syllable machines against HarfBuzz's (ADR 0024, 状態機械
//! のHarfBuzzとの突き合わせ): every line of `hb-dump --syllables` must
//! match. The alphabet of each machine is the category values HarfBuzz's
//! `hb-ot-shaper-<name>-machine.rl` exports, plus the smallest value it
//! does not export (for `other`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use crate::compare::Item;
use crate::paths;

// sofel-shape's runtime and generated tables, compiled into this crate as
// they are (sofel-shape does not export them).
#[path = "../../sofel-shape/src/machine.rs"]
#[allow(dead_code, missing_docs)]
mod machine;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/machine_indic.rs"]
#[allow(dead_code, missing_docs)]
mod machine_indic;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/machine_khmer.rs"]
#[allow(dead_code, missing_docs)]
mod machine_khmer;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/machine_myanmar.rs"]
#[allow(dead_code, missing_docs)]
mod machine_myanmar;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/machine_use.rs"]
#[allow(dead_code, missing_docs)]
mod machine_use;

use machine::{KeptIndices, Machine};

impl KeptIndices for Vec<usize> {
    fn clear_indices(&mut self) {
        self.clear();
    }
    fn push_index(&mut self, i: usize) {
        self.push(i);
    }
    fn indices(&self) -> &[usize] {
        self
    }
}

macro_rules! machine {
    ($m:ident) => {
        Machine {
            classes: &$m::CLASSES,
            other: $m::OTHER,
            num_classes: $m::NUM_CLASSES,
            transitions: &$m::TRANSITIONS,
            accept: &$m::ACCEPT,
        }
    };
}

/// The machines: name and tables.
pub const MACHINES: [&str; 4] = ["indic", "khmer", "myanmar", "use"];

fn tables(name: &str) -> Machine {
    match name {
        "indic" => machine!(machine_indic),
        "khmer" => machine!(machine_khmer),
        "myanmar" => machine!(machine_myanmar),
        _ => machine!(machine_use),
    }
}

/// sofel's syllable bytes for `cats` with machine `name` (every item's
/// syllable starts at 0, as hb-dump's buffers).
pub fn syllables(name: &str, cats: &[u8], kept: &mut Vec<usize>) -> Vec<u8> {
    let m = tables(name);
    let mut out = vec![0; cats.len()];
    if name == "use" {
        // hb-dump's items have no Unicode properties: none is a mark.
        m.syllables_use(cats.len(), |i| cats[i], |_| false, kept, |i, s| out[i] = s);
    } else {
        m.syllables(cats.len(), |i| cats[i], |i, s| out[i] = s);
    }
    out
}

/// The alphabet of a machine: the values exported by HarfBuzz's grammar
/// (sorted, without duplicates) and the smallest value not exported.
pub fn alphabet(rl: &str) -> Vec<u8> {
    let mut v: Vec<u8> = rl
        .lines()
        .filter_map(|l| l.trim().strip_prefix("export "))
        .filter_map(|l| l.split('=').nth(1))
        .filter_map(|l| l.split(';').next())
        .filter_map(|n| n.trim().parse().ok())
        .collect();
    v.sort_unstable();
    v.dedup();
    let other = (0..=255u8).find(|x| !v.contains(x)).unwrap();
    v.push(other);
    v
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect()
}

/// Runs `hb-dump --syllables` for machine `name` with `count` random
/// sequences and compares every line, streaming. The item is
/// `syllables:<name>`.
pub fn compare(name: &str, count: u32, seed: u32) -> Result<Item, String> {
    let rl = paths::testdata().join(format!("harfbuzz/src/hb-ot-shaper-{name}-machine.rl"));
    let rl = std::fs::read_to_string(&rl).map_err(|e| format!("{}: {e}", rl.display()))?;
    let alpha = alphabet(&rl);
    let list: Vec<String> = alpha.iter().map(u8::to_string).collect();
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump")?;
    let mut child = Command::new(exe)
        .args([
            "--syllables",
            name,
            &list.join(","),
            &count.to_string(),
            &seed.to_string(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let reader = BufReader::new(child.stdout.take().unwrap());
    let n = alpha.len();
    let mut exhaustive = (1usize..=4).flat_map(|len| {
        let total = n.pow(len as u32);
        let alpha = &alpha;
        (0..total).map(move |idx| {
            let mut v = vec![0u8; len];
            let mut x = idx;
            for k in (0..len).rev() {
                v[k] = alpha[x % n];
                x /= n;
            }
            v
        })
    });
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
        ((state >> 16) & 0x7FFF) as usize
    };
    let mut kept = Vec::new();
    let (mut lines, mut randoms) = (0usize, 0u32);
    let mut mismatches = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        lines += 1;
        let mut w = line.split(' ');
        let (cats, want) = match (w.next(), w.next(), w.next()) {
            (Some("e"), Some(s), None) => {
                let cats = exhaustive
                    .next()
                    .ok_or("more exhaustive lines than expected")?;
                (cats, s)
            }
            (Some("r"), Some(c), Some(s)) => {
                let len = 5 + next() % 28;
                let cats: Vec<u8> = (0..len).map(|_| alpha[next() % n]).collect();
                let got: Vec<u8> = c
                    .split(',')
                    .map(|h| u8::from_str_radix(h, 16).map_err(|_| format!("bad line: {line}")))
                    .collect::<Result<_, _>>()?;
                if got != cats {
                    return Err(format!("random sequence {randoms} differs: {line}"));
                }
                randoms += 1;
                (cats, s)
            }
            _ => return Err(format!("bad line: {line}")),
        };
        let want = hex_bytes(want).ok_or_else(|| format!("bad line: {line}"))?;
        let ours = syllables(name, &cats, &mut kept);
        if ours != want && mismatches.len() < 20 {
            mismatches.push(format!("{cats:?}: HarfBuzz {want:02x?}, sofel {ours:02x?}"));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("hb-dump --syllables {name} failed"));
    }
    if exhaustive.next().is_some() || randoms != count {
        return Err(format!("hb-dump --syllables {name}: missing lines"));
    }
    Ok(Item {
        id: format!("syllables:{name}"),
        ok: mismatches.is_empty(),
        detail: if mismatches.is_empty() {
            format!("{lines} sequences")
        } else {
            mismatches.join("\n")
        },
    })
}
