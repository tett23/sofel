//! sofel's Unicode character data against HarfBuzz (ADR 0019): the
//! output of `hb-dump --unicode`, `--arabic-pua` and
//! `--vowel-constraints`, whose format is described at the top of
//! `tools/hb-dump/hb-dump.cc`.
//!
//! Items, identified by `unicode:<item>`, each covering every value
//! `hb-dump` wrote:
//! - `general_category`, `combining_class`, `script`, `mirroring`,
//!   `decompose`, `extended_pictographic`: sofel-unicode for every code
//!   point.
//! - `compose`: [`sofel_unicode::compose`] for every pair `hb-dump`
//!   tried.
//! - `arabic_pua_simp`, `arabic_pua_trad`: the maps of sofel-font,
//!   included here from the generated file because sofel-font keeps them
//!   to itself.
//!
//! The shaper classes (indic, USE, Arabic joining) and the vowel
//! constraints are parsed but not compared yet: their tables belong to
//! sofel-shape (ADR 0020, ADR 0024), which does not exist yet.

use sofel_unicode as u;

use crate::compare::Item;

// The generated maps of sofel-font, compiled into this crate as they are
// (sofel-font does not export them).
#[rustfmt::skip]
#[path = "../../sofel-font/src/tables/arabic_pua.rs"]
#[allow(dead_code, missing_docs)]
mod arabic_pua;

/// One line of `hb-dump --unicode` for a code point.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HbChar {
    pub cp: u32,
    /// The number of HarfBuzz's `hb_unicode_general_category_t`.
    pub gc: u8,
    pub ccc: u8,
    pub script: [u8; 4],
    pub mirroring: u32,
    pub decomposition: Option<(u32, u32)>,
    pub extended_pictographic: bool,
    /// `hb_indic_get_categories`: the category in the low byte, the
    /// position in the high byte.
    pub indic: u16,
    /// `hb_use_get_category`.
    pub use_category: u8,
    /// `get_joining_type` of HarfBuzz's Arabic shaper.
    pub joining: u8,
}

/// The output of `hb-dump --unicode`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HbUnicode {
    pub chars: Vec<HbChar>,
    /// `(a, b, composite)`.
    pub compositions: Vec<(u32, u32, Option<u32>)>,
}

fn num<T: std::str::FromStr>(s: Option<&str>, line: &str) -> Result<T, String> {
    s.and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("bad line: {line}"))
}

fn flag(s: Option<&str>, line: &str) -> Result<bool, String> {
    match s {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err(format!("bad line: {line}")),
    }
}

/// Parses the output of `hb-dump --unicode`.
pub fn parse(text: &str) -> Result<HbUnicode, String> {
    let mut out = HbUnicode::default();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let mut w = line.split(' ');
        if line.starts_with("compose ") {
            w.next();
            let a = num(w.next(), line)?;
            let b = num(w.next(), line)?;
            let composed = flag(w.next(), line)?;
            let ab: u32 = num(w.next(), line)?;
            out.compositions.push((a, b, composed.then_some(ab)));
            continue;
        }
        let cp = num(w.next(), line)?;
        let gc = num(w.next(), line)?;
        let ccc = num(w.next(), line)?;
        let script = w
            .next()
            .and_then(|t| <[u8; 4]>::try_from(t.as_bytes()).ok())
            .ok_or_else(|| format!("bad line: {line}"))?;
        let mirroring = num(w.next(), line)?;
        let decomposed = flag(w.next(), line)?;
        let a: u32 = num(w.next(), line)?;
        let b: u32 = num(w.next(), line)?;
        let c = HbChar {
            cp,
            gc,
            ccc,
            script,
            mirroring,
            decomposition: decomposed.then_some((a, b)),
            extended_pictographic: flag(w.next(), line)?,
            indic: num(w.next(), line)?,
            use_category: num(w.next(), line)?,
            joining: num(w.next(), line)?,
        };
        if w.next().is_some() {
            return Err(format!("bad line: {line}"));
        }
        out.chars.push(c);
    }
    Ok(out)
}

fn item(name: &str, mismatches: &[String], total: usize) -> Item {
    Item {
        id: format!("unicode:{name}"),
        ok: mismatches.is_empty() && total > 0,
        detail: format!(
            "{} of {total} differ; first: {:?}",
            mismatches.len(),
            &mismatches[..mismatches.len().min(5)]
        ),
    }
}

fn per_char<T: PartialEq + std::fmt::Debug>(
    name: &str,
    hb: &HbUnicode,
    want: impl Fn(&HbChar) -> T,
    ours: impl Fn(u32) -> T,
) -> Item {
    let mismatches: Vec<String> = hb
        .chars
        .iter()
        .filter_map(|c| {
            let (w, o) = (want(c), ours(c.cp));
            (w != o).then(|| format!("{:#X}: HarfBuzz {w:?}, sofel {o:?}", c.cp))
        })
        .collect();
    item(name, &mismatches, hb.chars.len())
}

/// Compares every value of `hb-dump --unicode` with sofel-unicode.
pub fn compare(hb: &HbUnicode) -> Vec<Item> {
    let compose: Vec<String> = hb
        .compositions
        .iter()
        .filter_map(|&(a, b, want)| {
            let o = u::compose(a, b);
            (want != o).then(|| format!("({a:#X}, {b:#X}): HarfBuzz {want:?}, sofel {o:?}"))
        })
        .collect();
    vec![
        per_char(
            "general_category",
            hb,
            |c| c.gc,
            |cp| u::general_category(cp) as u8,
        ),
        per_char("combining_class", hb, |c| c.ccc, u::combining_class),
        per_char("script", hb, |c| c.script, |cp| u::script(cp).tag()),
        per_char("mirroring", hb, |c| c.mirroring, u::mirroring),
        per_char("decompose", hb, |c| c.decomposition, u::decompose),
        per_char(
            "extended_pictographic",
            hb,
            |c| c.extended_pictographic,
            u::is_extended_pictographic,
        ),
        item("compose", &compose, hb.compositions.len()),
    ]
}

/// The output of `hb-dump --arabic-pua`: `(code point, simplified,
/// traditional)`.
pub fn parse_arabic_pua(text: &str) -> Result<Vec<(u32, u32, u32)>, String> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut w = line.split(' ');
            if w.next() != Some("pua") {
                return Err(format!("bad line: {line}"));
            }
            Ok((
                num(w.next(), line)?,
                num(w.next(), line)?,
                num(w.next(), line)?,
            ))
        })
        .collect()
}

/// Compares the Arabic PUA maps of sofel-font with HarfBuzz's.
pub fn compare_arabic_pua(hb: &[(u32, u32, u32)]) -> Vec<Item> {
    let diff = |name: &str, want: fn(&(u32, u32, u32)) -> u32, ours: fn(u32) -> u32| {
        let m: Vec<String> = hb
            .iter()
            .filter_map(|e| {
                let (w, o) = (want(e), ours(e.0));
                (w != o).then(|| format!("{:#X}: HarfBuzz {w:#X}, sofel {o:#X}", e.0))
            })
            .collect();
        item(name, &m, hb.len())
    };
    vec![
        diff("arabic_pua_simp", |e| e.1, arabic_pua::simp_map),
        diff("arabic_pua_trad", |e| e.2, arabic_pua::trad_map),
    ]
}

/// One line of `hb-dump --vowel-constraints`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HbVowel {
    pub script: [u8; 4],
    pub input: Vec<u32>,
    pub output: Vec<u32>,
}

fn hex_list(s: Option<&str>, line: &str) -> Result<Vec<u32>, String> {
    s.ok_or_else(|| format!("bad line: {line}"))?
        .split(',')
        .map(|h| u32::from_str_radix(h, 16).map_err(|_| format!("bad line: {line}")))
        .collect()
}

/// Parses the output of `hb-dump --vowel-constraints`.
pub fn parse_vowel_constraints(text: &str) -> Result<Vec<HbVowel>, String> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut w = line.split(' ');
            if w.next() != Some("vowel") {
                return Err(format!("bad line: {line}"));
            }
            let script = w
                .next()
                .and_then(|t| <[u8; 4]>::try_from(t.as_bytes()).ok())
                .ok_or_else(|| format!("bad line: {line}"))?;
            Ok(HbVowel {
                script,
                input: hex_list(w.next(), line)?,
                output: hex_list(w.next(), line)?,
            })
        })
        .collect()
}
