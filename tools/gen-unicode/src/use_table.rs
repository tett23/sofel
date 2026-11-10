//! The categories of HarfBuzz's Universal Shaping Engine: the rules of
//! HarfBuzz 15.1.0's `src/gen-use-table.py` (MIT; THIRD_PARTY_NOTICES.md),
//! giving the value of `hb_use_get_category`. Two of the inputs come from
//! Microsoft's `ms-use` files in HarfBuzz's repository (MIT).

use std::collections::BTreeMap;

use crate::hbsrc::{empty_map, field_map, ragel_exports};
use crate::ucd::{CODE_POINTS, parse_ranges};

/// Scripts left to their own shapers.
pub const DISABLED_SCRIPTS: [&str; 5] = ["Arabic", "Lao", "Samaritan", "Syriac", "Thai"];

/// The inputs of the table.
pub struct Inputs<'a> {
    /// `IndicSyllabicCategory.txt`.
    pub syllabic: &'a str,
    /// `IndicPositionalCategory.txt`.
    pub positional: &'a str,
    /// `ArabicShaping.txt`.
    pub arabic_shaping: &'a str,
    /// `DerivedCoreProperties.txt`.
    pub derived_core_properties: &'a str,
    /// `UnicodeData.txt`.
    pub unicode_data: &'a str,
    /// `Blocks.txt`.
    pub blocks: &'a str,
    /// `Scripts.txt`.
    pub scripts: &'a str,
    /// `ms-use/IndicSyllabicCategory-Additional.txt`.
    pub syllabic_additional: &'a str,
    /// `ms-use/IndicPositionalCategory-Additional.txt`.
    pub positional_additional: &'a str,
    /// `src/hb-ot-shaper-use-machine.rl`.
    pub machine: &'a str,
}

/// The properties of one code point the script looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Props<'a> {
    /// The code point.
    pub u: u32,
    /// Indic_Syllabic_Category.
    pub isc: &'a str,
    /// Indic_Positional_Category.
    pub ipc: &'a str,
    /// Joining_Type, prefixed with `jt_`.
    pub ajt: &'a str,
    /// Default_Ignorable_Code_Point.
    pub di: bool,
    /// General_Category.
    pub gc: &'a str,
}

fn is_base(p: &Props) -> bool {
    [
        "Number",
        "Consonant",
        "Consonant_Head_Letter",
        "Tone_Letter",
        "Vowel_Independent",
    ]
    .contains(&p.isc)
        || (["jt_C", "jt_D", "jt_L", "jt_R"].contains(&p.ajt) && p.isc != "Joiner")
        || (p.gc == "Lo"
            && [
                "Avagraha",
                "Bindu",
                "Consonant_Final",
                "Consonant_Medial",
                "Consonant_Subjoined",
                "Vowel",
                "Vowel_Dependent",
            ]
            .contains(&p.isc))
}

fn is_base_other(p: &Props) -> bool {
    p.isc == "Consonant_Placeholder"
        || [0x2015, 0x2022, 0x25FB, 0x25FC, 0x25FD, 0x25FE].contains(&p.u)
}

fn is_cgj(p: &Props) -> bool {
    p.isc == "Joiner" || (p.di && ["Mc", "Me", "Mn"].contains(&p.gc))
}

fn is_halant_or_vowel_modifier(p: &Props) -> bool {
    p.u == 0x0DCA
}

fn is_sakot(p: &Props) -> bool {
    p.u == 0x1A60
}

fn is_sym_mod(p: &Props) -> bool {
    p.isc == "Symbol_Modifier"
}

fn is_word_joiner(p: &Props) -> bool {
    (p.di
        && ![
            0x115F, 0x1160, 0x3164, 0xFFA0, 0x1BCA0, 0x1BCA1, 0x1BCA2, 0x1BCA3,
        ]
        .contains(&p.u)
        && p.isc == "Other"
        && !is_cgj(p))
        || p.gc == "Cn"
}

fn is_other(p: &Props) -> bool {
    (p.gc == "Po" || ["Consonant_Dead", "Joiner", "Modifying_Letter", "Other"].contains(&p.isc))
        && !is_base(p)
        && !is_base_other(p)
        && !is_cgj(p)
        && !is_sym_mod(p)
        && !is_word_joiner(p)
}

type Rule = fn(&Props) -> bool;

/// `use_mapping`: each USE category and its rule, in the script's order.
pub const USE_MAPPING: [(&str, Rule); 29] = [
    ("B", is_base),
    ("N", |p| p.isc == "Brahmi_Joining_Number"),
    ("GB", is_base_other),
    ("CGJ", is_cgj),
    ("F", |p| {
        (p.isc == "Consonant_Final" && p.gc != "Lo") || p.isc == "Consonant_Succeeding_Repha"
    }),
    ("FM", |p| p.isc == "Syllable_Modifier"),
    ("M", |p| {
        (p.isc == "Consonant_Medial" && p.gc != "Lo") || p.isc == "Consonant_Initial_Postfixed"
    }),
    ("CM", |p| {
        ["Nukta", "Gemination_Mark", "Consonant_Killer"].contains(&p.isc)
    }),
    ("SUB", |p| p.isc == "Consonant_Subjoined" && p.gc != "Lo"),
    ("CS", |p| p.isc == "Consonant_With_Stacker"),
    ("H", |p| {
        p.isc == "Virama" && !is_halant_or_vowel_modifier(p)
    }),
    ("HVM", is_halant_or_vowel_modifier),
    ("HN", |p| p.isc == "Number_Joiner"),
    ("IS", |p| p.isc == "Invisible_Stacker" && !is_sakot(p)),
    ("G", |p| p.isc == "Hieroglyph"),
    ("HM", |p| p.isc == "Hieroglyph_Modifier"),
    ("HR", |p| p.isc == "Hieroglyph_Mirror"),
    ("J", |p| p.isc == "Hieroglyph_Joiner"),
    ("SB", |p| {
        ["Hieroglyph_Mark_Begin", "Hieroglyph_Segment_Begin"].contains(&p.isc)
    }),
    ("SE", |p| {
        ["Hieroglyph_Mark_End", "Hieroglyph_Segment_End"].contains(&p.isc)
    }),
    ("ZWNJ", |p| p.isc == "Non_Joiner"),
    ("O", is_other),
    ("RK", |p| p.isc == "Reordering_Killer"),
    ("R", |p| {
        ["Consonant_Preceding_Repha", "Consonant_Prefixed"].contains(&p.isc)
    }),
    ("Sk", is_sakot),
    ("SM", is_sym_mod),
    ("V", |p| {
        p.isc == "Pure_Killer" || (p.gc != "Lo" && ["Vowel", "Vowel_Dependent"].contains(&p.isc))
    }),
    ("VM", |p| {
        [
            "Tone_Mark",
            "Cantillation_Mark",
            "Register_Shifter",
            "Visarga",
        ]
        .contains(&p.isc)
            || (p.gc != "Lo" && p.isc == "Bindu")
    }),
    ("WJ", is_word_joiner),
];

type Positions = &'static [(&'static str, &'static [&'static str])];

/// `use_positions`: the suffix of a category by Indic_Positional_Category.
/// `Some(&[])` stands for the script's `None` (a category that takes no
/// suffix but may have a position).
pub fn use_positions(category: &str) -> Option<Positions> {
    Some(match category {
        "F" => &[("Abv", &["Top"]), ("Blw", &["Bottom"]), ("Pst", &["Right"])],
        "M" => &[
            ("Abv", &["Top"]),
            ("Blw", &["Bottom", "Bottom_And_Left", "Bottom_And_Right"]),
            ("Pst", &["Right"]),
            ("Pre", &["Left", "Top_And_Bottom_And_Left"]),
        ],
        "CM" => &[("Abv", &["Top"]), ("Blw", &["Bottom", "Overstruck"])],
        "V" => &[
            (
                "Abv",
                &[
                    "Top",
                    "Top_And_Bottom",
                    "Top_And_Bottom_And_Right",
                    "Top_And_Right",
                ],
            ),
            ("Blw", &["Bottom", "Overstruck", "Bottom_And_Right"]),
            ("Pst", &["Right"]),
            (
                "Pre",
                &[
                    "Left",
                    "Top_And_Left",
                    "Top_And_Left_And_Right",
                    "Left_And_Right",
                ],
            ),
        ],
        "VM" => &[
            ("Abv", &["Top"]),
            ("Blw", &["Bottom", "Overstruck"]),
            ("Pst", &["Right"]),
            ("Pre", &["Left"]),
        ],
        "SM" => &[("Abv", &["Top"]), ("Blw", &["Bottom"])],
        "FM" => &[
            ("Abv", &["Top"]),
            ("Blw", &["Bottom"]),
            ("Pst", &["Not_Applicable"]),
        ],
        "H" | "HM" | "HR" | "HVM" | "IS" | "B" | "R" | "RK" | "SUB" => &[],
        _ => return None,
    })
}

/// `map_to_use` for one code point: its USE category by name.
pub fn map_to_use(p: Props) -> Result<String, String> {
    let mut p = p;
    if (0x1CE2..=0x1CE8).contains(&p.u) {
        p.isc = "Cantillation_Mark";
    }
    if (0x0F18..=0x0F19).contains(&p.u) || (0x0F3E..=0x0F3F).contains(&p.u) {
        p.isc = "Vowel_Dependent";
    }
    if p.u == 0x1CED {
        p.isc = "Tone_Mark";
    }
    let matches: Vec<&str> = USE_MAPPING
        .iter()
        .filter(|(_, rule)| rule(&p))
        .map(|(name, _)| *name)
        .collect();
    let category = match matches[..] {
        [c] => c,
        _ => return Err(format!("{p:?} matches {matches:?}")),
    };
    if [0x11302, 0x11303, 0x114C1].contains(&p.u) {
        p.ipc = "Top";
    }
    let positions = use_positions(category);
    if !(["Not_Applicable", "Visual_Order_Left"].contains(&p.ipc)
        || p.u == 0x0F7F
        || positions.is_some())
    {
        return Err(format!("{p:?}: {category} takes no position"));
    }
    match positions {
        Some(list) if !list.is_empty() => {
            let suffixes: Vec<&str> = list
                .iter()
                .filter(|(_, ipcs)| ipcs.contains(&p.ipc))
                .map(|(s, _)| *s)
                .collect();
            match suffixes[..] {
                [s] => Ok(format!("{category}{s}")),
                _ => Err(format!("{p:?}: {category} at {suffixes:?}")),
            }
        }
        _ => Ok(category.to_string()),
    }
}

/// Read `UnicodeData.txt` the way the script does: one code point per
/// line, so the `<..., First>` and `<..., Last>` lines give only the ends
/// of their ranges.
fn general_categories<'a>(text: &'a str, out: &mut [Option<&'a str>]) -> Result<(), String> {
    for line in text.lines() {
        let body = line.split('#').next().unwrap_or("");
        let f: Vec<&str> = body.split(';').map(str::trim).collect();
        if f.len() == 1 {
            continue;
        }
        let gc = f.get(2).ok_or(format!("short line: {line}"))?;
        let cp = u32::from_str_radix(f[0], 16).map_err(|e| format!("{line}: {e}"))?;
        *out.get_mut(cp as usize)
            .ok_or(format!("bad code point: {line}"))? = Some(gc);
    }
    Ok(())
}

/// The USE category name of every code point the script writes (the
/// others are `O`).
pub fn names(inputs: &Inputs) -> Result<BTreeMap<u32, String>, String> {
    let parsed = |t: &str| parse_ranges(t).map(|(d, _)| d);
    let isc_lines = parsed(inputs.syllabic)?;
    let ipc_lines = parsed(inputs.positional)?;
    let jt_lines = parsed(inputs.arabic_shaping)?;
    let dcp_lines = parsed(inputs.derived_core_properties)?;
    let blk_lines = parsed(inputs.blocks)?;
    let sc_lines = parsed(inputs.scripts)?;
    let isc_add = parsed(inputs.syllabic_additional)?;
    let ipc_add = parsed(inputs.positional_additional)?;

    let mut isc = empty_map();
    field_map(&mut isc, &isc_lines, 0)?;
    for l in &isc_add {
        let v = match l.fields.first().map(String::as_str) {
            // https://github.com/MicrosoftDocs/typography-issues/issues/336
            Some("Consonant_Final_Modifier") => "Syllable_Modifier",
            Some(v) => v,
            None => return Err(format!("no value for U+{:04X}", l.first)),
        };
        (l.first..=l.last).for_each(|u| isc[u as usize] = Some(v));
    }
    let mut ipc = empty_map();
    field_map(&mut ipc, &ipc_lines, 0)?;
    for l in &ipc_add {
        let v = match l.fields.first().map(String::as_str) {
            Some("NA") => "Not_Applicable",
            Some(v) => v,
            None => return Err(format!("no value for U+{:04X}", l.first)),
        };
        (l.first..=l.last).for_each(|u| ipc[u as usize] = Some(v));
    }
    let mut ajt = empty_map();
    field_map(&mut ajt, &jt_lines, 1)?;
    let mut di = vec![false; CODE_POINTS];
    dcp_lines
        .iter()
        .filter(|l| l.fields.first().map(String::as_str) == Some("Default_Ignorable_Code_Point"))
        .for_each(|l| (l.first..=l.last).for_each(|u| di[u as usize] = true));
    let mut gc = empty_map();
    general_categories(inputs.unicode_data, &mut gc)?;
    let mut sc = empty_map();
    field_map(&mut sc, &sc_lines, 0)?;
    // Blocks.txt is read by the script but takes no part in the categories.
    let mut blk = empty_map();
    field_map(&mut blk, &blk_lines, 0)?;

    let mut out = BTreeMap::new();
    for u in 0..CODE_POINTS as u32 {
        let i = u as usize;
        if isc[i].is_none() && ipc[i].is_none() && ajt[i].is_none() && !di[i] {
            continue;
        }
        if DISABLED_SCRIPTS.contains(&sc[i].unwrap_or("Unknown")) {
            continue;
        }
        let jt = ajt[i].map_or("jt_X".to_string(), |t| format!("jt_{t}"));
        let p = Props {
            u,
            isc: isc[i].unwrap_or("Other"),
            ipc: ipc[i].unwrap_or("Not_Applicable"),
            ajt: &jt,
            di: di[i],
            gc: gc[i].unwrap_or("Cn"),
        };
        out.insert(u, map_to_use(p)?);
    }
    Ok(out)
}

/// `hb_use_get_category` for every code point, and its default (`O`).
pub fn values(inputs: &Inputs) -> Result<(Vec<u32>, u32), String> {
    let exports = ragel_exports(inputs.machine)?;
    let number = |c: &str| {
        exports
            .get(c)
            .copied()
            .ok_or(format!("the USE machine does not export {c}"))
    };
    let default = number("O")?;
    let mut out = vec![default; CODE_POINTS];
    for (u, c) in names(inputs)? {
        out[u as usize] = number(&c)?;
    }
    Ok((out, default))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props<'a>(u: u32, isc: &'a str, ipc: &'a str, gc: &'a str) -> Props<'a> {
        Props {
            u,
            isc,
            ipc,
            ajt: "jt_X",
            di: false,
            gc,
        }
    }

    #[test]
    fn categories_and_positions() {
        let m = |p| map_to_use(p).unwrap();
        assert_eq!(m(props(0x0915, "Consonant", "Not_Applicable", "Lo")), "B");
        assert_eq!(m(props(0x093F, "Vowel_Dependent", "Left", "Mc")), "VPre");
        assert_eq!(m(props(0x0941, "Vowel_Dependent", "Bottom", "Mn")), "VBlw");
        // A vowel sign that is a letter is a base.
        assert_eq!(
            m(props(0x1000, "Vowel_Dependent", "Not_Applicable", "Lo")),
            "B"
        );
        assert_eq!(m(props(0x0901, "Bindu", "Top", "Mn")), "VMAbv");
        assert_eq!(m(props(0x094D, "Virama", "Bottom", "Mn")), "H");
        assert_eq!(m(props(0x0DCA, "Virama", "Top", "Mn")), "HVM");
        assert_eq!(
            m(props(0x1A60, "Invisible_Stacker", "Not_Applicable", "Mn")),
            "Sk"
        );
        assert_eq!(m(props(0x0378, "Other", "Not_Applicable", "Cn")), "WJ");
        assert_eq!(m(props(0x0021, "Other", "Not_Applicable", "Po")), "O");
        // U+11302 is moved to the top.
        assert_eq!(m(props(0x11302, "Bindu", "Not_Applicable", "Mn")), "VMAbv");
        // Syllable modifiers without a position are post.
        assert_eq!(
            m(props(0x1A7F, "Syllable_Modifier", "Not_Applicable", "Mn")),
            "FMPst"
        );
        // Vedic tone marks become cantillation marks.
        assert_eq!(m(props(0x1CE2, "Other", "Overstruck", "Mn")), "VMBlw");
        let joining = Props {
            ajt: "jt_D",
            ..props(0xA840, "Other", "Not_Applicable", "Lo")
        };
        assert_eq!(m(joining), "B");
        let cgj = Props {
            di: true,
            ..props(0x034F, "Other", "Not_Applicable", "Mn")
        };
        assert_eq!(m(cgj), "CGJ");
        let wj = Props {
            di: true,
            ..props(0x2060, "Other", "Not_Applicable", "Cf")
        };
        assert_eq!(m(wj), "WJ");
    }

    #[test]
    fn unresolved_characters_fail() {
        // No rule.
        assert!(map_to_use(props(0x0041, "Mystery", "Not_Applicable", "Lu")).is_err());
        // A position the category cannot take.
        assert!(map_to_use(props(0x0915, "Number_Joiner", "Top", "Mn")).is_err());
        assert!(map_to_use(props(0x0915, "Nukta", "Right", "Mn")).is_err());
    }

    #[test]
    fn files_are_merged() {
        let inputs = Inputs {
            syllabic: "0915 ; Consonant\n0941 ; Vowel_Dependent\n0E01 ; Consonant\n",
            positional: "0941 ; Bottom\n",
            arabic_shaping: "0628; BEH; D; BEH\n",
            derived_core_properties: "034F ; Default_Ignorable_Code_Point\n0041 ; Alphabetic\n",
            unicode_data: "034F;CGJ;Mn;1;NSM;;;;;N;;;;;\n0915;KA;Lo;0;L;;;;;N;;;;;\n0941;U;Mn;0;NSM;;;;;N;;;;;\n0E01;KO KAI;Lo;0;L;;;;;N;;;;;\n",
            blocks: "0900..097F; Devanagari\n",
            scripts: "0628 ; Arabic\n0915..0941 ; Devanagari\n0E01 ; Thai\n",
            syllabic_additional: "0941 ; Consonant_Final_Modifier\n",
            positional_additional: "0941 ; NA\n",
            machine: "export O = 0;\nexport B = 1;\nexport CGJ = 6;\nexport FMPst = 47;\n",
        };
        let n = names(&inputs).unwrap();
        assert_eq!(n.len(), 3);
        assert_eq!(n[&0x0915], "B");
        // ms-use overrides both the category and the position.
        assert_eq!(n[&0x0941], "FMPst");
        assert_eq!(n[&0x034F], "CGJ");
        // Arabic and Thai are left out.
        assert!(!n.contains_key(&0x0628) && !n.contains_key(&0x0E01));
        let (v, default) = values(&inputs).unwrap();
        assert_eq!((v[0x0941], v[0x0628], default), (47, 0, 0));
    }
}
