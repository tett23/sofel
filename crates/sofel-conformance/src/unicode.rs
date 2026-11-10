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
//! - `modified_combining_class`, `default_ignorable`, `space_fallback`,
//!   `vertical_char_for`: the hand-written functions of
//!   [`sofel_shape::unicode`] (ADR 0020) for every code point.
//! - `indic`, `use`, `joining`: the shaper tables of sofel-shape
//!   (`crates/sofel-shape/src/tables/`, ADR 0024) for every code point:
//!   `indic_categories` against `hb_indic_get_categories`, `use_category`
//!   against `hb_use_get_category`, and `joining_type` completed as
//!   HarfBuzz's `get_joining_type` (T for Mn, Me and Cf, U otherwise, when
//!   the table has X).
//! - `arabic_pua_simp`, `arabic_pua_trad`: the maps of sofel-font.
//! - `vowel_constraints`: every line of `hb-dump --vowel-constraints`,
//!   replayed with sofel-shape's `vowel_constraint_match`.
//! - `arabic_shaping_table`, `arabic_ligature_table`,
//!   `arabic_ligature_mark_table`, `arabic_ligature_3_table`,
//!   `arabic_joining_scripts`: the constant tables of sofel-shape's
//!   `tables/arabic.rs` against HarfBuzz's generated sources
//!   (`src/hb-ot-shaper-arabic-table.hh`,
//!   `src/hb-ot-shaper-arabic-joining-list.hh`), which `hb-dump` cannot
//!   reach.
//!
//! The generated tables of sofel-font and sofel-shape are included here
//! from their files because those crates keep them to themselves.

use sofel_unicode as u;

use crate::compare::Item;

// The generated maps of sofel-font, compiled into this crate as they are
// (sofel-font does not export them).
#[rustfmt::skip]
#[path = "../../sofel-font/src/tables/arabic_pua.rs"]
#[allow(dead_code, missing_docs)]
mod arabic_pua;

// The generated shaper tables of sofel-shape, likewise.
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/indic.rs"]
#[allow(dead_code, missing_docs)]
mod indic;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/use_.rs"]
#[allow(dead_code, missing_docs)]
mod use_;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/arabic.rs"]
#[allow(dead_code, missing_docs)]
mod arabic;
#[rustfmt::skip]
#[path = "../../sofel-shape/src/tables/vowel_constraints.rs"]
#[allow(dead_code, missing_docs)]
mod vowel_constraints;

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
    /// `modified_combining_class` (ADR 0020).
    pub modified_combining_class: u8,
    /// `is_default_ignorable`.
    pub default_ignorable: bool,
    /// `space_fallback_type` as its number.
    pub space_fallback: u8,
    /// `vertical_char_for`.
    pub vertical: u32,
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
            modified_combining_class: num(w.next(), line)?,
            default_ignorable: flag(w.next(), line)?,
            space_fallback: num(w.next(), line)?,
            vertical: num(w.next(), line)?,
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

/// HarfBuzz's `get_joining_type` (`hb-ot-shaper-arabic.cc`): the table's
/// joining type, or for a character it does not list (X), T for the
/// nonspacing and enclosing marks and the format characters, U otherwise.
pub fn joining_type(cp: u32) -> u8 {
    const X: u8 = 7;
    const T: u8 = 6;
    const U: u8 = 0;
    match arabic::joining_type(cp) {
        X => match u::general_category(cp) {
            u::GeneralCategory::NonspacingMark
            | u::GeneralCategory::EnclosingMark
            | u::GeneralCategory::Format => T,
            _ => U,
        },
        j => j,
    }
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
        per_char("indic", hb, |c| c.indic, indic::indic_categories),
        per_char("use", hb, |c| c.use_category, use_::use_category),
        per_char("joining", hb, |c| c.joining, joining_type),
        per_char(
            "modified_combining_class",
            hb,
            |c| c.modified_combining_class,
            sofel_shape::unicode::modified_combining_class,
        ),
        per_char(
            "default_ignorable",
            hb,
            |c| c.default_ignorable,
            sofel_shape::unicode::is_default_ignorable,
        ),
        per_char(
            "space_fallback",
            hb,
            |c| c.space_fallback,
            |cp| sofel_shape::unicode::space_fallback_type(cp) as u8,
        ),
        per_char(
            "vertical_char_for",
            hb,
            |c| c.vertical,
            sofel_shape::unicode::vertical_char_for,
        ),
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

/// The output of the loop of HarfBuzz's
/// `_hb_preprocess_text_vowel_constraints` over `cps` under `script`, with
/// sofel-shape's `vowel_constraint_match` deciding where the dotted
/// circles go.
pub fn vowel_constraints(script: [u8; 4], cps: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < cps.len() {
        match vowel_constraints::vowel_constraint_match(script, cps, i) {
            Some(n) => {
                out.extend_from_slice(&cps[i..i + n - 1]);
                out.push(0x25CC);
                out.push(cps[i + n - 1]);
                i += n;
            }
            None => {
                out.push(cps[i]);
                i += 1;
            }
        }
    }
    out.extend_from_slice(&cps[i.min(cps.len())..]);
    out
}

/// Compares every line of `hb-dump --vowel-constraints`.
pub fn compare_vowel_constraints(hb: &[HbVowel]) -> Item {
    let m: Vec<String> = hb
        .iter()
        .filter_map(|v| {
            let o = vowel_constraints(v.script, &v.input);
            (o != v.output).then(|| {
                format!(
                    "{} {:X?}: HarfBuzz {:X?}, sofel {o:X?}",
                    String::from_utf8_lossy(&v.script),
                    v.input,
                    v.output
                )
            })
        })
        .collect();
    item("vowel_constraints", &m, hb.len())
}

/// A ligature table: each first component with its `(components,
/// ligature)` entries.
type Ligatures = Vec<(u32, Vec<(Vec<u32>, u32)>)>;

/// A ligature table of sofel-shape.
type OurLigatures<'a, const N: usize> = &'a [(u16, &'a [([u16; N], u16)])];

/// The hexadecimal numbers of a line of C, outside its comments.
fn c_hex(line: &str) -> Vec<u32> {
    line.split("/*")
        .next()
        .unwrap_or("")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|w| w.strip_prefix("0x"))
        .filter_map(|w| u32::from_str_radix(w.trim_end_matches('u'), 16).ok())
        .collect()
}

/// The lines of `text` after the line containing `start`, up to the first
/// `};`.
fn c_block<'a>(text: &'a str, start: &str) -> Vec<&'a str> {
    text.lines()
        .skip_while(|l| !l.contains(start))
        .skip(1)
        .take_while(|l| !l.starts_with("};"))
        .collect()
}

/// A ligature table of HarfBuzz's C source: each first component with its
/// `(components, ligature)` entries.
fn c_ligatures(text: &str, start: &str) -> Ligatures {
    let mut out: Ligatures = Vec::new();
    for l in c_block(text, start) {
        match c_hex(l)[..] {
            [first] => out.push((first, Vec::new())),
            [ref comps @ .., lig] if !comps.is_empty() => {
                if let Some(last) = out.last_mut() {
                    last.1.push((comps.to_vec(), lig));
                }
            }
            _ => {}
        }
    }
    out
}

fn ours<const N: usize>(t: OurLigatures<N>) -> Ligatures {
    t.iter()
        .map(|(f, ls)| {
            (
                u32::from(*f),
                ls.iter()
                    .map(|(c, l)| (c.iter().map(|&x| u32::from(x)).collect(), u32::from(*l)))
                    .collect(),
            )
        })
        .collect()
}

/// Compares the constant tables of sofel-shape's `tables/arabic.rs` with
/// HarfBuzz's `src/hb-ot-shaper-arabic-table.hh` (`table_hh`),
/// `src/hb-ot-shaper-arabic-joining-list.hh` (`joining_list_hh`) and the
/// script tags of `src/hb-script-list.h` (`script_list_h`).
pub fn compare_arabic_tables(
    table_hh: &str,
    joining_list_hh: &str,
    script_list_h: &str,
) -> Vec<Item> {
    let define = |name: &str| {
        table_hh
            .lines()
            .find(|l| l.starts_with(&format!("#define {name}")))
            .and_then(|l| c_hex(l).first().copied())
    };
    let hb_rows: Vec<Vec<u32>> = c_block(table_hh, "shaping_table[][4]")
        .into_iter()
        .map(c_hex)
        .filter(|r| !r.is_empty())
        .collect();
    let our_rows: Vec<Vec<u32>> = arabic::SHAPING_TABLE
        .iter()
        .map(|r| r.iter().map(|&x| u32::from(x)).collect())
        .collect();
    let mut shaping = Vec::new();
    let bounds = (define("SHAPING_TABLE_FIRST"), define("SHAPING_TABLE_LAST"));
    let ours_bounds = (
        Some(arabic::SHAPING_TABLE_FIRST),
        Some(arabic::SHAPING_TABLE_LAST),
    );
    if bounds != ours_bounds {
        shaping.push(format!(
            "bounds: HarfBuzz {bounds:X?}, sofel {ours_bounds:X?}"
        ));
    }
    if hb_rows != our_rows {
        shaping.push(format!(
            "rows: HarfBuzz {} rows, sofel {}; first different: {:?}",
            hb_rows.len(),
            our_rows.len(),
            hb_rows.iter().zip(&our_rows).position(|(a, b)| a != b)
        ));
    }
    let lig = |name: &str, c_name: &str, o: Ligatures| {
        let hb = c_ligatures(table_hh, &format!("}} {c_name}[] ="));
        let m = if hb == o {
            vec![]
        } else {
            vec![format!("HarfBuzz {hb:X?}, sofel {o:X?}")]
        };
        item(name, &m, hb.len())
    };

    // Every script tag, and whether HarfBuzz lists it.
    let tags: Vec<(String, [u8; 4])> = script_list_h
        .lines()
        .filter_map(|l| {
            let (name, rest) = l.trim().split_once('=')?;
            let name = name.trim().strip_prefix("HB_SCRIPT_")?;
            let tag: Vec<u8> = rest
                .split_once("HB_TAG")?
                .1
                .split('\'')
                .skip(1)
                .step_by(2)
                .take(4)
                .filter_map(|c| c.bytes().next())
                .collect();
            Some((name.to_string(), <[u8; 4]>::try_from(tag).ok()?))
        })
        .collect();
    let listed: Vec<&str> = joining_list_hh
        .lines()
        .filter_map(|l| l.trim().strip_prefix("case HB_SCRIPT_"))
        .map(|l| l.trim_end_matches(':'))
        .collect();
    let mut scripts: Vec<String> = tags
        .iter()
        .filter(|(name, tag)| listed.contains(&name.as_str()) != arabic::has_arabic_joining(*tag))
        .map(|(name, _)| format!("{name}: HarfBuzz {}", listed.contains(&name.as_str())))
        .collect();
    if let Some(n) = listed
        .iter()
        .find(|n| !tags.iter().any(|(name, _)| name == *n))
    {
        scripts.push(format!("{n} has no tag in hb-script-list.h"));
    }

    vec![
        item("arabic_shaping_table", &shaping, hb_rows.len()),
        lig(
            "arabic_ligature_table",
            "ligature_table",
            ours(&arabic::LIGATURE_TABLE),
        ),
        lig(
            "arabic_ligature_mark_table",
            "ligature_mark_table",
            ours(&arabic::LIGATURE_MARK_TABLE),
        ),
        lig(
            "arabic_ligature_3_table",
            "ligature_3_table",
            ours(&arabic::LIGATURE_3_TABLE),
        ),
        item(
            "arabic_joining_scripts",
            &scripts,
            listed.len().min(tags.len()),
        ),
    ]
}
