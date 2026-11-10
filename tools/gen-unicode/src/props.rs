//! The properties of `sofel-unicode`, read from the UCD files (ADR 0019).
//!
//! Which decompositions are kept and which pairs compose follow the rules
//! of HarfBuzz 15.1.0's `src/gen-ucd-table.py` (MIT; THIRD_PARTY_NOTICES.md).

use std::collections::{BTreeMap, BTreeSet};

use crate::ucd::{
    CODE_POINTS, binary_property, fill, missing_default, parse_ranges, parse_script_aliases,
    parse_unicode_data,
};

/// General_Category values in the order of HarfBuzz's
/// `hb_unicode_general_category_t`.
pub const GC_ORDER: [&str; 30] = [
    "Cc", "Cf", "Cn", "Co", "Cs", "Ll", "Lm", "Lo", "Lt", "Lu", "Mc", "Me", "Mn", "Nd", "Nl", "No",
    "Pc", "Pd", "Pe", "Pf", "Pi", "Po", "Ps", "Sc", "Sk", "Sm", "So", "Zl", "Zp", "Zs",
];

/// Vertical_Orientation values in the order of `sofel_unicode::VerticalOrientation`.
pub const VO_ORDER: [&str; 4] = ["U", "R", "Tu", "Tr"];

/// Hangul syllables, composed and decomposed by arithmetic.
pub const HANGUL: std::ops::Range<u32> = 0xAC00..0xAC00 + 11172;

/// The texts of the UCD files `sofel-unicode` reads.
pub struct Inputs<'a> {
    /// `UnicodeData.txt`.
    pub unicode_data: &'a str,
    /// `Scripts.txt`.
    pub scripts: &'a str,
    /// `PropertyValueAliases.txt`.
    pub property_value_aliases: &'a str,
    /// `BidiMirroring.txt`.
    pub bidi_mirroring: &'a str,
    /// `DerivedNormalizationProps.txt`.
    pub derived_normalization_props: &'a str,
    /// `emoji/emoji-data.txt`.
    pub emoji_data: &'a str,
    /// `VerticalOrientation.txt`.
    pub vertical_orientation: &'a str,
}

/// A table of one value per code point and its default.
pub struct Values {
    /// One value per code point, U+0000..=U+10FFFF.
    pub values: Vec<u32>,
    /// The value beyond the table.
    pub default: u32,
}

/// Everything `crates/sofel-unicode/src/tables.rs` holds.
pub struct Properties {
    /// General_Category, as indices of [`GC_ORDER`].
    pub gc: Values,
    /// Canonical_Combining_Class.
    pub ccc: Values,
    /// Script, as indices of `script_tags`.
    pub sc: Values,
    /// ISO 15924 tags of every script, sorted.
    pub script_tags: Vec<String>,
    /// Bidi_Mirroring_Glyph minus the code point, as indices of `bmg_palette`.
    pub bmg: Values,
    /// The distinct differences of Bidi_Mirroring_Glyph, 0 first, then sorted.
    pub bmg_palette: Vec<i32>,
    /// Decomposition numbers: 0 none, `1..=dm1.len()` one character,
    /// then two characters.
    pub dm: Values,
    /// One-character decompositions, sorted.
    pub dm1: Vec<u32>,
    /// Two-character decompositions `(a << 42) | (b << 21) | ab`, sorted;
    /// `ab` is 0 if the pair does not compose.
    pub dm2: Vec<u64>,
    /// Extended_Pictographic, 0 or 1.
    pub ext_pict: Values,
    /// Vertical_Orientation, as indices of [`VO_ORDER`].
    pub vo: Values,
}

fn index_of(order: &[&str], name: &str) -> Result<u32, String> {
    order
        .iter()
        .position(|&v| v == name)
        .map(|i| i as u32)
        .ok_or(format!("unknown value {name:?}"))
}

fn first_field(l: &crate::ucd::RangeLine) -> Result<&str, String> {
    l.fields
        .first()
        .map(String::as_str)
        .ok_or(format!("no value for {:04X}", l.first))
}

/// Encode a two-character decomposition the way HarfBuzz's
/// `HB_CODEPOINT_ENCODE3` does.
pub fn encode3(a: u32, b: u32, ab: u32) -> u64 {
    (u64::from(a) << 42) | (u64::from(b) << 21) | u64::from(ab)
}

/// Read every property.
pub fn read(inputs: &Inputs) -> Result<Properties, String> {
    let entries = parse_unicode_data(inputs.unicode_data)?;

    let mut gc = vec![index_of(&GC_ORDER, "Cn")?; CODE_POINTS];
    let mut ccc = vec![0u32; CODE_POINTS];
    let mut canonical: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for e in &entries {
        let g = index_of(&GC_ORDER, &e.gc)?;
        (e.first..=e.last).for_each(|cp| {
            gc[cp as usize] = g;
            ccc[cp as usize] = u32::from(e.ccc);
        });
        if let Some(d) = &e.canonical {
            if e.first != e.last {
                return Err(format!("decomposition on a range at U+{:04X}", e.first));
            }
            canonical.insert(e.first, d.clone());
        }
    }

    let aliases = parse_script_aliases(inputs.property_value_aliases);
    let script_tags: Vec<String> = aliases
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let tag_index = |long: &str| -> Result<u32, String> {
        let short = aliases
            .get(long)
            .ok_or(format!("unknown script {long:?}"))?;
        Ok(script_tags
            .binary_search(short)
            .map_err(|_| short.clone())? as u32)
    };
    let (sc_lines, sc_missing) = parse_ranges(inputs.scripts)?;
    let sc_default = tag_index(&missing_default(&sc_missing)?)?;
    let sc = fill(sc_default, &sc_lines, |l| {
        tag_index(first_field(l)?).map(Some)
    })?;

    let (bmg_lines, _) = parse_ranges(inputs.bidi_mirroring)?;
    let diffs = fill(0i32, &bmg_lines, |l| {
        let to = u32::from_str_radix(first_field(l)?, 16).map_err(|e| e.to_string())?;
        i32::try_from(i64::from(to) - i64::from(l.first))
            .map(Some)
            .map_err(|e| e.to_string())
    })?;
    let bmg_palette: Vec<i32> = std::iter::once(0)
        .chain(
            diffs
                .iter()
                .copied()
                .filter(|&d| d != 0)
                .collect::<BTreeSet<_>>(),
        )
        .collect();
    let bmg = diffs
        .iter()
        .map(|d| {
            bmg_palette
                .iter()
                .position(|p| p == d)
                .map_or(0, |i| i as u32)
        })
        .collect();

    let (dnp, _) = parse_ranges(inputs.derived_normalization_props)?;
    let comp_ex = binary_property(&dnp, "Full_Composition_Exclusion")?;

    let (emoji, _) = parse_ranges(inputs.emoji_data)?;
    let ext_pict = binary_property(&emoji, "Extended_Pictographic")?
        .into_iter()
        .map(u32::from)
        .collect();

    let (vo_lines, vo_missing) = parse_ranges(inputs.vertical_orientation)?;
    let vo_default = index_of(&VO_ORDER, &missing_default(&vo_missing)?)?;
    let vo = fill(vo_default, &vo_lines, |l| {
        index_of(&VO_ORDER, first_field(l)?).map(Some)
    })?;

    let (dm, dm1, dm2) = decompositions(&canonical, &ccc, &comp_ex)?;

    Ok(Properties {
        gc: Values {
            values: gc,
            default: index_of(&GC_ORDER, "Cn")?,
        },
        ccc: Values {
            values: ccc,
            default: 0,
        },
        sc: Values {
            values: sc,
            default: sc_default,
        },
        script_tags,
        bmg: Values {
            values: bmg,
            default: 0,
        },
        bmg_palette,
        dm: Values {
            values: dm,
            default: 0,
        },
        dm1,
        dm2,
        ext_pict: Values {
            values: ext_pict,
            default: 0,
        },
        vo: Values {
            values: vo,
            default: vo_default,
        },
    })
}

type Decompositions = (Vec<u32>, Vec<u32>, Vec<u64>);

/// The decomposition tables of HarfBuzz's `gen-ucd-table.py`: canonical
/// decompositions except the Hangul syllables; a pair composes unless the
/// composite is a full composition exclusion or has a non-zero combining
/// class.
pub fn decompositions(
    canonical: &BTreeMap<u32, Vec<u32>>,
    ccc: &[u32],
    comp_ex: &[bool],
) -> Result<Decompositions, String> {
    let dm: BTreeMap<u32, &Vec<u32>> = canonical
        .iter()
        .filter(|(cp, _)| !HANGUL.contains(cp))
        .map(|(&cp, d)| (cp, d))
        .collect();
    if let Some((cp, _)) = dm.iter().find(|(_, d)| !matches!(d.len(), 1 | 2)) {
        return Err(format!(
            "U+{cp:04X} decomposes into neither 1 nor 2 characters"
        ));
    }
    let dm1: Vec<u32> = dm
        .values()
        .filter(|d| d.len() == 1)
        .map(|d| d[0])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let composes = |cp: u32| !comp_ex[cp as usize] && ccc[cp as usize] == 0;
    let mut pairs: Vec<(u32, u32, u32)> = dm
        .iter()
        .filter(|(_, d)| d.len() == 2)
        .map(|(&cp, d)| (d[0], d[1], if composes(cp) { cp } else { 0 }))
        .collect();
    pairs.sort_unstable();
    if let Some(w) = pairs
        .windows(2)
        .find(|w| (w[0].0, w[0].1) == (w[1].0, w[1].1))
    {
        return Err(format!(
            "two characters decompose into U+{:04X} U+{:04X}",
            w[0].0, w[0].1
        ));
    }
    // HarfBuzz looks pairs with a < U+0800 and b in U+0300..U+037F up only
    // in its 32-bit array, which holds a composite only if it fits; one
    // that does not would compose here and not in HarfBuzz.
    if let Some(p) = pairs.iter().find(|&&(a, b, ab)| {
        a & 0xFFFF_F800 == 0 && b & 0xFFFF_FF80 == 0x300 && ab & 0xFFF0_C000 != 0
    }) {
        return Err(format!("HarfBuzz would not compose {p:04X?}"));
    }
    let base = 1 + dm1.len() as u32;
    let number = |cp: u32, d: &[u32]| -> u32 {
        match d {
            [x] => 1 + dm1.binary_search(x).map_or(0, |i| i as u32),
            [a, b] => {
                base + pairs
                    .binary_search_by_key(&(*a, *b), |p| (p.0, p.1))
                    .map_or(0, |i| i as u32)
            }
            _ => unreachable!("U+{cp:04X}"),
        }
    };
    let mut values = vec![0u32; CODE_POINTS];
    dm.iter()
        .for_each(|(&cp, d)| values[cp as usize] = number(cp, d));
    let dm2 = pairs.iter().map(|&(a, b, ab)| encode3(a, b, ab)).collect();
    Ok((values, dm1, dm2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposition_numbers() {
        let mut canonical = BTreeMap::new();
        canonical.insert(0xC5, vec![0x41, 0x30A]);
        canonical.insert(0x212B, vec![0xC5]);
        canonical.insert(0x958, vec![0x915, 0x93C]);
        canonical.insert(0x344, vec![0x308, 0x301]);
        canonical.insert(0xAC00, vec![0x1100, 0x1161]);
        let mut ccc = vec![0u32; CODE_POINTS];
        ccc[0x344] = 230;
        let mut comp_ex = vec![false; CODE_POINTS];
        comp_ex[0x958] = true;
        comp_ex[0x344] = true;
        let (dm, dm1, dm2) = decompositions(&canonical, &ccc, &comp_ex).unwrap();
        assert_eq!(dm1, vec![0xC5]);
        assert_eq!(
            dm2,
            vec![
                encode3(0x41, 0x30A, 0xC5),
                encode3(0x308, 0x301, 0),
                encode3(0x915, 0x93C, 0)
            ]
        );
        assert_eq!(dm[0x212B], 1);
        assert_eq!(dm[0xC5], 2);
        assert_eq!(dm[0x344], 3);
        assert_eq!(dm[0x958], 4);
        assert_eq!(dm[0xAC00], 0);
        assert_eq!(dm[0x41], 0);
    }
}
