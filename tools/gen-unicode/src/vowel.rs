//! The vowel constraints of HarfBuzz's `src/gen-vowel-constraints.py`
//! (MIT; THIRD_PARTY_NOTICES.md): sequences of Microsoft's
//! `ms-use/IndicShapingInvalidCluster.txt` (MIT), grouped by the script of
//! their first character, between whose last two characters HarfBuzz's
//! `_hb_preprocess_text_vowel_constraints` puts a dotted circle.
//!
//! The script builds a trie of the sequences (`ConstraintSet`) and writes
//! it as nested `switch` statements. The trie keeps the shorter of two
//! sequences one of which starts the other, and its code generation
//! supports only these shapes, which are what [`sequences`] returns:
//! - a first character with one sequence of 2 or 3 characters
//!   (3 checks `idx + 2 < count` before reading the third),
//! - a first character with several second characters, each ending a
//!   sequence of 2.
//!
//! Any other shape fails the script's assertions (or, for a single
//! sequence of 3 or more for a whole script, would read past the buffer);
//! [`sequences`] fails on them too.

use std::collections::BTreeMap;

use crate::ucd::parse_ranges;

/// `ConstraintSet`: a sequence, or the sets of sequences by their first
/// character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A single sequence (possibly empty: matched).
    Seq(Vec<u32>),
    /// Sequences by their first character.
    Branch(BTreeMap<u32, Node>),
}

impl Node {
    /// `ConstraintSet.add`.
    pub fn add(&mut self, constraint: &[u32]) {
        let Some((&first, rest)) = constraint.split_first() else {
            return;
        };
        if let Node::Seq(c) = self {
            if c.starts_with(constraint) {
                *c = constraint.to_vec();
            } else if !constraint.starts_with(c) {
                let mut m = BTreeMap::new();
                if let Some((&h, t)) = c.split_first() {
                    m.insert(h, Node::Seq(t.to_vec()));
                }
                *self = Node::Branch(m);
            }
        }
        if let Node::Branch(m) = self {
            match m.get_mut(&first) {
                Some(n) => n.add(rest),
                None => {
                    m.insert(first, Node::Seq(rest.to_vec()));
                }
            }
        }
    }
}

/// Read `IndicShapingInvalidCluster.txt`: the sequences in file order.
pub fn parse(text: &str) -> Result<Vec<Vec<u32>>, String> {
    let mut lines = text.lines();
    lines
        .by_ref()
        .find(|l| l.trim() == "#")
        .ok_or("no header end (`#`) in IndicShapingInvalidCluster.txt")?;
    lines
        .map(|l| {
            l.split('#')
                .next()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
        })
        .map(|l| {
            l.split_whitespace()
                .map(|h| u32::from_str_radix(h, 16).map_err(|e| format!("{l}: {e}")))
                .collect::<Result<Vec<u32>, String>>()
        })
        .filter(|s| !matches!(s, Ok(v) if v.is_empty()))
        .map(|s| {
            s.and_then(|v| {
                if v.len() < 2 {
                    Err(format!("prohibited sequence is too short: {v:X?}"))
                } else {
                    Ok(v)
                }
            })
        })
        .collect()
}

/// The sequences a trie matches, sorted, each of length 2 or 3.
pub fn sequences(node: &Node) -> Result<Vec<Vec<u32>>, String> {
    let unsupported = |what: String| Err(format!("unsupported vowel constraint: {what}"));
    match node {
        Node::Seq(s) if s.len() == 2 => Ok(vec![s.clone()]),
        Node::Seq(s) => unsupported(format!("{s:X?} alone")),
        Node::Branch(m) => {
            let mut out = Vec::new();
            for (&c0, child) in m {
                match child {
                    Node::Seq(s) if s.len() == 1 || s.len() == 2 => {
                        out.push([&[c0][..], s].concat());
                    }
                    Node::Seq(s) => return unsupported(format!("{c0:X} then {s:X?}")),
                    Node::Branch(m1) => {
                        for (&c1, rest) in m1 {
                            if *rest != Node::Seq(vec![]) {
                                return unsupported(format!("{c0:X} {c1:X} then {rest:X?}"));
                            }
                            out.push(vec![c0, c1]);
                        }
                    }
                }
            }
            Ok(out)
        }
    }
}

/// The sequences of every script, as `(long script name, sequences)` in
/// the order of the scripts' first lines in `Scripts.txt`.
#[allow(clippy::type_complexity)]
pub fn by_script(clusters: &str, scripts: &str) -> Result<Vec<(String, Vec<Vec<u32>>)>, String> {
    let (sc_lines, _) = parse_ranges(scripts)?;
    let mut order: Vec<&str> = Vec::new();
    let mut sc: BTreeMap<u32, &str> = BTreeMap::new();
    for l in &sc_lines {
        let name = l.fields.first().ok_or("empty Scripts.txt line")?.as_str();
        if !order.contains(&name) {
            order.push(name);
        }
        (l.first..=l.last).for_each(|u| {
            sc.insert(u, name);
        });
    }
    let mut tries: BTreeMap<&str, Node> = BTreeMap::new();
    for c in parse(clusters)? {
        let script = *sc
            .get(&c[0])
            .ok_or(format!("U+{:04X} has no script", c[0]))?;
        match tries.get_mut(script) {
            Some(t) => t.add(&c),
            None => {
                tries.insert(script, Node::Seq(c));
            }
        }
    }
    order
        .iter()
        .filter_map(|s| tries.get(s).map(|t| (s, t)))
        .map(|(s, t)| Ok((s.to_string(), sequences(t)?)))
        .collect()
}

/// The hand-written lookup emitted after the data.
pub const LOOKUP: &str = r#"/// The length of the vowel constraint sequence of `script` that starts at
/// `cps[i]`, as the loop of HarfBuzz's generated
/// `_hb_preprocess_text_vowel_constraints` matches it when it is at `i`
/// (while `i + 1 < cps.len()`). On `Some(n)` the loop writes
/// `cps[i..i + n - 1]`, U+25CC DOTTED CIRCLE and `cps[i + n - 1]`, then
/// goes on at `i + n`; on `None` it writes `cps[i]` and goes on at `i + 1`.
/// A sequence of 3 needs `i + 2 < cps.len()`, as HarfBuzz checks.
pub(crate) fn vowel_constraint_match(script: [u8; 4], cps: &[u32], i: usize) -> Option<usize> {
    let (_, seqs) = VOWEL_CONSTRAINTS.iter().find(|(s, _)| *s == script)?;
    let at = |k: usize| i.checked_add(k).and_then(|j| cps.get(j)).copied();
    seqs.iter().find_map(|s| match *s {
        [a, b, 0] => (at(0) == Some(a) && at(1) == Some(b)).then_some(2),
        [a, b, c] => (at(0) == Some(a) && at(1) == Some(b) && at(2) == Some(c)).then_some(3),
    })
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn trie(seqs: &[&[u32]]) -> Node {
        let mut t = Node::Seq(seqs[0].to_vec());
        seqs[1..].iter().for_each(|s| t.add(s));
        t
    }

    #[test]
    fn the_shorter_sequence_wins() {
        // Gujarati: A+AA+CANDRA E is left out by A+AA, in either order.
        let a = trie(&[&[0xA85, 0xABE], &[0xA85, 0xABE, 0xAC5], &[0xAC5, 0xABE]]);
        let b = trie(&[&[0xA85, 0xABE, 0xAC5], &[0xA85, 0xABE], &[0xAC5, 0xABE]]);
        assert_eq!(a, b);
        assert_eq!(
            sequences(&a).unwrap(),
            vec![vec![0xA85, 0xABE], vec![0xAC5, 0xABE]]
        );
    }

    #[test]
    fn shapes() {
        // One sequence for a script.
        assert_eq!(
            sequences(&trie(&[&[0xB85, 0xBC2]])).unwrap(),
            vec![vec![0xB85, 0xBC2]]
        );
        assert!(sequences(&trie(&[&[1, 2, 3]])).is_err());
        // A sequence of 3 under its own first character.
        let deva = trie(&[&[0x905, 0x946], &[0x930, 0x94D, 0x907], &[0x905, 0x93E]]);
        assert_eq!(
            sequences(&deva).unwrap(),
            vec![
                vec![0x905, 0x93E],
                vec![0x905, 0x946],
                vec![0x930, 0x94D, 0x907]
            ]
        );
        // A first character with a sequence of 2 and one of 3 that are not
        // prefixes of each other: HarfBuzz's script asserts.
        assert!(sequences(&trie(&[&[1, 2], &[1, 3, 4]])).is_err());
        assert!(sequences(&trie(&[&[9, 9], &[1, 2, 3, 4]])).is_err());
    }

    #[test]
    fn file_and_scripts() {
        let clusters = "# IndicShapingInvalidCluster.txt\n# Date: x\n#\n  0905 0946   ; # A, SHORT E\n\n  0930 094D 0907 ; # RA VIRAMA I\n 0B85 0BC2 ; # TAMIL\n";
        let scripts = "0000..007F ; Latin\n0900..097F ; Devanagari\n0B80..0BFF ; Tamil\n";
        assert_eq!(
            by_script(clusters, scripts).unwrap(),
            vec![
                (
                    "Devanagari".to_string(),
                    vec![vec![0x905, 0x946], vec![0x930, 0x94D, 0x907]]
                ),
                ("Tamil".to_string(), vec![vec![0xB85, 0xBC2]]),
            ]
        );
        assert!(parse("#\n 0905 ;\n").is_err());
        assert!(parse("# no end\n").is_err());
    }
}
