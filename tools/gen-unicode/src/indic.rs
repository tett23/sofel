//! The categories and positions of HarfBuzz's indic, khmer and myanmar
//! shapers: the rules of HarfBuzz 15.1.0's `src/gen-indic-table.py` (MIT;
//! THIRD_PARTY_NOTICES.md), giving the value of `hb_indic_get_categories`
//! (the category in the low byte, the position in the high byte).

use std::collections::BTreeMap;

use crate::hbsrc::{empty_map, field_map};
use crate::ucd::{CODE_POINTS, parse_ranges};

/// Code points kept whatever their block.
pub const ALLOWED_SINGLES: [u32; 2] = [0x00A0, 0x25CC];

/// The blocks whose characters are kept.
pub const ALLOWED_BLOCKS: [&str; 22] = [
    "Basic Latin",
    "Latin-1 Supplement",
    "Devanagari",
    "Bengali",
    "Gurmukhi",
    "Gujarati",
    "Oriya",
    "Tamil",
    "Telugu",
    "Kannada",
    "Malayalam",
    "Myanmar",
    "Khmer",
    "Vedic Extensions",
    "General Punctuation",
    "Superscripts and Subscripts",
    "Devanagari Extended",
    "Myanmar Extended-B",
    "Myanmar Extended-A",
    "Myanmar Extended-C",
    "Devanagari Extended-A",
    "Bengali Supplement",
];

/// The categories of each shaper, in the order of the script's
/// `categories`; a category takes its number from the first shaper that
/// lists it.
pub const CATEGORIES: [(&str, &[&str]); 3] = [
    (
        "indic",
        &[
            "X",
            "C",
            "V",
            "N",
            "H",
            "ZWNJ",
            "ZWJ",
            "M",
            "SM",
            "A",
            "VD",
            "PLACEHOLDER",
            "DOTTEDCIRCLE",
            "RS",
            "MPst",
            "Repha",
            "Ra",
            "CM",
            "Symbol",
            "CS",
            "SMPst",
        ],
    ),
    (
        "khmer",
        &[
            "VAbv", "VBlw", "VPre", "VPst", "Robatic", "Xgroup", "Ygroup",
        ],
    ),
    (
        "myanmar",
        &[
            "VAbv", "VBlw", "VPre", "VPst", "IV", "As", "DB", "GB", "MH", "MR", "MW", "MY", "PT",
            "VS", "ML",
        ],
    ),
];

fn category_map(isc: &str) -> Option<&'static str> {
    Some(match isc {
        "Other" => "X",
        "Avagraha" => "Symbol",
        "Bindu" => "SM",
        "Brahmi_Joining_Number" => "PLACEHOLDER",
        "Cantillation_Mark" => "A",
        "Consonant" => "C",
        "Consonant_Dead" => "C",
        "Consonant_Final" => "CM",
        "Consonant_Head_Letter" => "C",
        "Consonant_Initial_Postfixed" => "C",
        "Consonant_Killer" => "M",
        "Consonant_Medial" => "CM",
        "Consonant_Placeholder" => "PLACEHOLDER",
        "Consonant_Preceding_Repha" => "Repha",
        "Consonant_Prefixed" => "X",
        "Consonant_Subjoined" => "CM",
        "Consonant_Succeeding_Repha" => "CM",
        "Consonant_With_Stacker" => "CS",
        "Gemination_Mark" => "SM",
        "Invisible_Stacker" => "H",
        "Joiner" => "ZWJ",
        "Modifying_Letter" => "X",
        "Non_Joiner" => "ZWNJ",
        "Nukta" => "N",
        "Number" => "PLACEHOLDER",
        "Number_Joiner" => "PLACEHOLDER",
        "Pure_Killer" => "M",
        "Register_Shifter" => "RS",
        "Syllable_Modifier" => "SM",
        "Tone_Letter" => "X",
        "Tone_Mark" => "N",
        "Virama" => "H",
        "Visarga" => "SM",
        "Vowel" => "V",
        "Vowel_Dependent" => "M",
        "Vowel_Independent" => "V",
        _ => return None,
    })
}

fn position_map(ipc: &str) -> Option<&'static str> {
    Some(match ipc {
        "Not_Applicable" => "END",
        "Left" => "PRE_C",
        "Top" => "ABOVE_C",
        "Bottom" => "BELOW_C",
        "Right" => "POST_C",
        "Bottom_And_Right" => "POST_C",
        "Left_And_Right" => "POST_C",
        "Top_And_Bottom" => "BELOW_C",
        "Top_And_Bottom_And_Left" => "BELOW_C",
        "Top_And_Bottom_And_Right" => "POST_C",
        "Top_And_Left" => "ABOVE_C",
        "Top_And_Left_And_Right" => "POST_C",
        "Top_And_Right" => "POST_C",
        "Overstruck" => "AFTER_MAIN",
        "Visual_order_left" => "PRE_M",
        _ => return None,
    })
}

/// `category_overrides`, in the script's order.
pub const CATEGORY_OVERRIDES: &[(u32, &str)] = &[
    (0xFE00, "VS"),
    (0xFE01, "VS"),
    (0xFE02, "VS"),
    (0xFE03, "VS"),
    (0xFE04, "VS"),
    (0xFE05, "VS"),
    (0xFE06, "VS"),
    (0xFE07, "VS"),
    (0xFE08, "VS"),
    (0xFE09, "VS"),
    (0xFE0A, "VS"),
    (0xFE0B, "VS"),
    (0xFE0C, "VS"),
    (0xFE0D, "VS"),
    (0xFE0E, "VS"),
    (0xFE0F, "VS"),
    (0x2015, "PLACEHOLDER"),
    (0x2022, "PLACEHOLDER"),
    (0x25FB, "PLACEHOLDER"),
    (0x25FC, "PLACEHOLDER"),
    (0x25FD, "PLACEHOLDER"),
    (0x25FE, "PLACEHOLDER"),
    (0x0930, "Ra"),
    (0x09B0, "Ra"),
    (0x09F0, "Ra"),
    (0x0A30, "Ra"),
    (0x0AB0, "Ra"),
    (0x0B30, "Ra"),
    (0x0BB0, "Ra"),
    (0x0C30, "Ra"),
    (0x0CB0, "Ra"),
    (0x0D30, "Ra"),
    (0x0953, "SM"),
    (0x0954, "SM"),
    (0x0A40, "MPst"),
    (0x0A72, "C"),
    (0x0A73, "C"),
    (0x1CE2, "A"),
    (0x1CE3, "A"),
    (0x1CE4, "A"),
    (0x1CE5, "A"),
    (0x1CE6, "A"),
    (0x1CE7, "A"),
    (0x1CE8, "A"),
    (0x1CED, "A"),
    (0xA8F2, "Symbol"),
    (0xA8F3, "Symbol"),
    (0xA8F4, "Symbol"),
    (0xA8F5, "Symbol"),
    (0xA8F6, "Symbol"),
    (0xA8F7, "Symbol"),
    (0x1CE9, "Symbol"),
    (0x1CEA, "Symbol"),
    (0x1CEB, "Symbol"),
    (0x1CEC, "Symbol"),
    (0x1CEE, "Symbol"),
    (0x1CEF, "Symbol"),
    (0x1CF0, "Symbol"),
    (0x1CF1, "Symbol"),
    (0x0A51, "M"),
    (0x11301, "SM"),
    (0x11302, "SM"),
    (0x11303, "SM"),
    (0x1133B, "N"),
    (0x1133C, "N"),
    (0x0AFB, "N"),
    (0x0B55, "N"),
    (0x09FC, "PLACEHOLDER"),
    (0x0C80, "PLACEHOLDER"),
    (0x0D04, "PLACEHOLDER"),
    (0x25CC, "DOTTEDCIRCLE"),
    (0x179A, "Ra"),
    (0x17CC, "Robatic"),
    (0x17C9, "Robatic"),
    (0x17CA, "Robatic"),
    (0x17C6, "Xgroup"),
    (0x17CB, "Xgroup"),
    (0x17CD, "Xgroup"),
    (0x17CE, "Xgroup"),
    (0x17CF, "Xgroup"),
    (0x17D0, "Xgroup"),
    (0x17D1, "Xgroup"),
    (0x17C7, "Ygroup"),
    (0x17C8, "Ygroup"),
    (0x17DD, "Ygroup"),
    (0x17D3, "Ygroup"),
    (0x17D9, "PLACEHOLDER"),
    (0x104E, "C"),
    (0x1004, "Ra"),
    (0x101B, "Ra"),
    (0x105A, "Ra"),
    (0x1032, "A"),
    (0x1036, "A"),
    (0x103A, "As"),
    (0x103E, "MH"),
    (0x1060, "ML"),
    (0x103C, "MR"),
    (0x103D, "MW"),
    (0x1082, "MW"),
    (0x103B, "MY"),
    (0x105E, "MY"),
    (0x105F, "MY"),
    (0x1063, "PT"),
    (0x1064, "PT"),
    (0x1069, "PT"),
    (0x106A, "PT"),
    (0x106B, "PT"),
    (0x106C, "PT"),
    (0x106D, "PT"),
    (0xAA7B, "PT"),
    (0x1038, "SM"),
    (0x1087, "SM"),
    (0x1088, "SM"),
    (0x1089, "SM"),
    (0x108A, "SM"),
    (0x108B, "SM"),
    (0x108C, "SM"),
    (0x108D, "SM"),
    (0x108F, "SM"),
    (0x109A, "SM"),
    (0x109B, "SM"),
    (0x109C, "SM"),
    (0x104A, "PLACEHOLDER"),
];

/// `position_overrides`.
pub const POSITION_OVERRIDES: [(u32, &str); 2] = [(0x0A51, "BELOW_C"), (0x0B01, "BEFORE_SUB")];

fn matra_pos_left(_u: u32, _block: &str) -> &'static str {
    "PRE_M"
}

fn matra_pos_right(u: u32, block: &str) -> &'static str {
    match block {
        "Devanagari" => "AFTER_SUB",
        "Bengali" | "Gurmukhi" | "Gujarati" | "Oriya" | "Tamil" => "AFTER_POST",
        "Telugu" if u <= 0x0C42 => "BEFORE_SUB",
        "Telugu" => "AFTER_SUB",
        "Kannada" if !(0x0CC3..=0x0CD6).contains(&u) => "BEFORE_SUB",
        "Kannada" => "AFTER_SUB",
        "Malayalam" => "AFTER_POST",
        _ => "AFTER_SUB",
    }
}

fn matra_pos_top(_u: u32, block: &str) -> &'static str {
    match block {
        "Devanagari" => "AFTER_SUB",
        "Gurmukhi" => "AFTER_POST",
        "Gujarati" => "AFTER_SUB",
        "Oriya" => "AFTER_MAIN",
        "Tamil" => "AFTER_SUB",
        "Telugu" | "Kannada" => "BEFORE_SUB",
        _ => "AFTER_SUB",
    }
}

fn matra_pos_bottom(_u: u32, block: &str) -> &'static str {
    match block {
        "Devanagari" | "Bengali" => "AFTER_SUB",
        "Gurmukhi" | "Gujarati" => "AFTER_POST",
        "Oriya" => "AFTER_SUB",
        "Tamil" => "AFTER_POST",
        "Telugu" | "Kannada" => "BEFORE_SUB",
        "Malayalam" => "AFTER_POST",
        _ => "AFTER_SUB",
    }
}

fn indic_matra_position(u: u32, pos: &str, block: &str) -> Result<&'static str, String> {
    match pos {
        "PRE_C" => Ok(matra_pos_left(u, block)),
        "POST_C" => Ok(matra_pos_right(u, block)),
        "ABOVE_C" => Ok(matra_pos_top(u, block)),
        "BELOW_C" => Ok(matra_pos_bottom(u, block)),
        _ => Err(format!("U+{u:04X}: matra at position {pos}")),
    }
}

fn position_to_category(u: u32, pos: &str) -> Result<&'static str, String> {
    match pos {
        "PRE_C" => Ok("VPre"),
        "ABOVE_C" => Ok("VAbv"),
        "BELOW_C" => Ok("VBlw"),
        "POST_C" => Ok("VPst"),
        _ => Err(format!(
            "U+{u:04X}: Khmer or Myanmar matra at position {pos}"
        )),
    }
}

/// The inputs of the table.
pub struct Inputs<'a> {
    /// `IndicSyllabicCategory.txt`.
    pub syllabic: &'a str,
    /// `IndicPositionalCategory.txt`.
    pub positional: &'a str,
    /// `Blocks.txt`.
    pub blocks: &'a str,
    /// `src/hb-ot-shaper-{indic,khmer,myanmar}-machine.rl`, in the order
    /// of [`CATEGORIES`].
    pub machines: [&'a str; 3],
    /// `src/hb-ot-shaper-indic.hh`.
    pub indic_hh: &'a str,
}

/// The categories and positions, by name, of every code point the script
/// writes (the others have `("X", "END")`).
pub fn names(inputs: &Inputs) -> Result<BTreeMap<u32, (&'static str, &'static str)>, String> {
    let (isc_lines, _) = parse_ranges(inputs.syllabic)?;
    let (ipc_lines, _) = parse_ranges(inputs.positional)?;
    let (blk_lines, _) = parse_ranges(inputs.blocks)?;
    let mut isc = empty_map();
    let mut ipc = empty_map();
    let mut blk = empty_map();
    field_map(&mut isc, &isc_lines, 0)?;
    field_map(&mut ipc, &ipc_lines, 0)?;
    field_map(&mut blk, &blk_lines, 0)?;

    // (category, position, block) after the category and position maps.
    let mut data: BTreeMap<u32, (&'static str, &'static str, String)> = BTreeMap::new();
    for u in 0..CODE_POINTS as u32 {
        let (s, p) = (isc[u as usize], ipc[u as usize]);
        if s.is_none() && p.is_none() {
            continue;
        }
        let (s, p) = (s.unwrap_or("Other"), p.unwrap_or("Not_Applicable"));
        let block = blk[u as usize].unwrap_or("No_Block");
        if !ALLOWED_SINGLES.contains(&u) && !ALLOWED_BLOCKS.contains(&block) {
            continue;
        }
        let mut cat = category_map(s).ok_or(format!("unknown category {s}"))?;
        if cat == "SM" && p == "Not_Applicable" {
            cat = "SMPst";
        }
        let pos = position_map(p).ok_or(format!("unknown position {p}"))?;
        data.insert(u, (cat, pos, block.to_string()));
    }
    let block_of = |k: u32| {
        blk[k as usize]
            .map(str::to_string)
            .ok_or(format!("U+{k:04X} has no block"))
    };
    for &(k, new_cat) in CATEGORY_OVERRIDES {
        let pos = data.get(&k).map_or("END", |d| d.1);
        data.insert(k, (new_cat, pos, block_of(k)?));
    }
    const POSITIONED: [&str; 6] = ["CM", "SM", "RS", "H", "M", "MPst"];
    data.values_mut()
        .filter(|d| !POSITIONED.contains(&d.0))
        .for_each(|d| d.1 = "END");
    const CONSONANT: [&str; 7] = ["C", "CS", "Ra", "CM", "V", "PLACEHOLDER", "DOTTEDCIRCLE"];
    const MATRA: [&str; 2] = ["M", "MPst"];
    const SMVD: [&str; 5] = ["SM", "SMPst", "VD", "A", "Symbol"];
    for (&k, d) in data.iter_mut() {
        if CONSONANT.contains(&d.0) {
            d.1 = "BASE_C";
        } else if MATRA.contains(&d.0) {
            if d.2.starts_with("Khmer") || d.2.starts_with("Myanmar") {
                d.0 = position_to_category(k, d.1)?;
            } else {
                d.1 = indic_matra_position(k, d.1, &d.2)?;
            }
        } else if SMVD.contains(&d.0) {
            d.1 = "SMVD";
        }
    }
    for (k, new_pos) in POSITION_OVERRIDES {
        let cat = data.get(&k).map_or("X", |d| d.0);
        data.insert(k, (cat, new_pos, block_of(k)?));
    }
    if let Some(u) = ALLOWED_SINGLES.iter().find(|u| !data.contains_key(u)) {
        return Err(format!("U+{u:04X} has no indic data"));
    }
    Ok(data.into_iter().map(|(k, (c, p, _))| (k, (c, p))).collect())
}

/// The numbers of the categories (from the machines' exports) and of the
/// positions (`ot_position_t`).
#[allow(clippy::type_complexity)]
pub fn numbers(
    inputs: &Inputs,
) -> Result<(BTreeMap<&'static str, u32>, BTreeMap<String, u32>), String> {
    let exports: Vec<BTreeMap<String, u32>> = inputs
        .machines
        .iter()
        .map(|t| crate::hbsrc::ragel_exports(t))
        .collect::<Result<_, _>>()?;
    let mut cats: BTreeMap<&'static str, u32> = BTreeMap::new();
    for ((shaper, list), ex) in CATEGORIES.iter().zip(&exports) {
        for &c in *list {
            let n = *ex
                .get(c)
                .ok_or(format!("{shaper} machine does not export {c}"))?;
            // The first shaper defines it; the others assert the same number.
            if *cats.entry(c).or_insert(n) != n {
                return Err(format!("{c} differs between machines"));
            }
        }
    }
    let positions = crate::hbsrc::indic_positions(inputs.indic_hh)?;
    Ok((cats, positions))
}

/// `hb_indic_get_categories` for every code point, and its default.
pub fn values(inputs: &Inputs) -> Result<(Vec<u32>, u32), String> {
    let names = names(inputs)?;
    let (cats, positions) = numbers(inputs)?;
    let encode = |c: &str, p: &str| -> Result<u32, String> {
        let c = cats.get(c).ok_or(format!("no number for category {c}"))?;
        let p = positions
            .get(p)
            .ok_or(format!("no number for position {p}"))?;
        Ok(c | (p << 8))
    };
    let default = encode("X", "END")?;
    let mut out = vec![default; CODE_POINTS];
    for (u, (c, p)) in names {
        out[u as usize] = encode(c, p)?;
    }
    Ok((out, default))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs<'a>(syllabic: &'a str, positional: &'a str, blocks: &'a str) -> Inputs<'a> {
        Inputs {
            syllabic,
            positional,
            blocks,
            machines: [
                "export X = 0;\nexport C = 1;\nexport V = 2;\nexport N = 3;\nexport H = 4;\nexport ZWNJ = 5;\nexport ZWJ = 6;\nexport M = 7;\nexport SM = 8;\nexport A = 9;\nexport VD = 9;\nexport PLACEHOLDER = 10;\nexport DOTTEDCIRCLE = 11;\nexport RS = 12;\nexport MPst = 13;\nexport Repha = 14;\nexport Ra = 15;\nexport CM = 16;\nexport Symbol = 17;\nexport CS = 18;\nexport SMPst = 57;\n",
                "export VAbv = 20;\nexport VBlw = 21;\nexport VPre = 22;\nexport VPst = 23;\nexport Robatic = 25;\nexport Xgroup = 26;\nexport Ygroup = 27;\n",
                "export VAbv = 20;\nexport VBlw = 21;\nexport VPre = 22;\nexport VPst = 23;\nexport IV = 2;\nexport As = 32;\nexport DB = 3;\nexport GB = 10;\nexport MH = 35;\nexport MR = 36;\nexport MW = 37;\nexport MY = 38;\nexport PT = 39;\nexport VS = 40;\nexport ML = 41;\n",
            ],
            indic_hh: "enum ot_position_t {\n POS_START = 0,\n POS_RA_TO_BECOME_REPH = 1,\n POS_PRE_M = 2,\n POS_PRE_C = 3,\n POS_BASE_C = 4,\n POS_AFTER_MAIN = 5,\n POS_ABOVE_C = 6,\n POS_BEFORE_SUB = 7,\n POS_BELOW_C = 8,\n POS_AFTER_SUB = 9,\n POS_BEFORE_POST = 10,\n POS_POST_C = 11,\n POS_AFTER_POST = 12,\n POS_SMVD = 13,\n POS_END = 14\n};\n",
        }
    }

    const BLOCKS: &str = "0000..10FFFF; Elsewhere\n0000..007F; Basic Latin\n0080..00FF; Latin-1 Supplement\n0900..097F; Devanagari\n0A00..0A7F; Gurmukhi\n0C00..0C7F; Telugu\n1000..109F; Myanmar\n1780..17FF; Khmer\n25A0..25FF; Geometric Shapes\nA900..A92F; Kayah Li\nFE00..FE0F; Variation Selectors\n";

    #[test]
    fn categories_positions_and_overrides() {
        let syllabic = "\
0901 ; Bindu
0903 ; Visarga
0915 ; Consonant
0930 ; Consonant
093F ; Vowel_Dependent
0940 ; Vowel_Dependent
094D ; Virama
0C3E ; Vowel_Dependent
0C46 ; Vowel_Dependent
1031 ; Vowel_Dependent
17C1 ; Vowel_Dependent
A90A ; Consonant
25CC ; Consonant_Placeholder
00A0 ; Consonant_Placeholder
";
        let positional = "\
0901 ; Top
093F ; Left
0940 ; Right
0C3E ; Right
0C46 ; Top
1031 ; Left
17C1 ; Left
0A40 ; Right
0A51 ; Bottom
";
        let i = inputs(syllabic, positional, BLOCKS);
        let n = names(&i).unwrap();
        // Bindu with a position stays SM; Visarga without one becomes SMPst.
        assert_eq!(n[&0x0901], ("SM", "SMVD"));
        assert_eq!(n[&0x0903], ("SMPst", "SMVD"));
        assert_eq!(n[&0x0915], ("C", "BASE_C"));
        // An override keeps the position and changes the category.
        assert_eq!(n[&0x0930], ("Ra", "BASE_C"));
        assert_eq!(n[&0x093F], ("M", "PRE_M"));
        assert_eq!(n[&0x0940], ("M", "AFTER_SUB"));
        assert_eq!(n[&0x094D], ("H", "END"));
        // Telugu right matras before U+0C42 are before the subjoined ones.
        assert_eq!(n[&0x0C3E], ("M", "BEFORE_SUB"));
        assert_eq!(n[&0x0C46], ("M", "BEFORE_SUB"));
        // Khmer and Myanmar matras take a category from their position.
        assert_eq!(n[&0x1031], ("VPre", "PRE_C"));
        assert_eq!(n[&0x17C1], ("VPre", "PRE_C"));
        // Outside the allowed blocks.
        assert!(!n.contains_key(&0xA90A));
        // Overrides outside the data and blocks.
        assert_eq!(n[&0xFE00], ("VS", "END"));
        assert_eq!(n[&0x25CC], ("DOTTEDCIRCLE", "BASE_C"));
        assert_eq!(n[&0x25FB], ("PLACEHOLDER", "BASE_C"));
        // Gurmukhi U+0A51 becomes a matra, then is moved below.
        assert_eq!(n[&0x0A51], ("M", "BELOW_C"));

        let (v, default) = values(&i).unwrap();
        assert_eq!(default, 14 << 8);
        assert_eq!(v[0x0915], 1 | (4 << 8));
        assert_eq!(v[0x1031], 22 | (3 << 8));
        assert_eq!(v[0xFE00], 40 | (14 << 8));
        assert_eq!(v[0xA90A], default);
    }

    #[test]
    fn unknown_values_fail() {
        let blocks = "0000..10FFFF; Elsewhere\n0900..097F; Devanagari\n";
        let i = inputs("0915 ; Consonant_Mystery\n", "", blocks);
        assert!(names(&i).is_err());
        let i = inputs("0915 ; Vowel_Dependent\n", "", blocks);
        // A Devanagari matra without a position cannot be placed.
        assert!(names(&i).is_err());
    }
}
