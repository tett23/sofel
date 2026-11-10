//! The tables of HarfBuzz's Arabic shaper: the rules of HarfBuzz 15.1.0's
//! `src/gen-arabic-table.py` (joining types, presentation forms and
//! ligatures) and `src/gen-arabic-joining-list.py` (the scripts that join),
//! MIT; THIRD_PARTY_NOTICES.md.

use std::collections::{BTreeMap, BTreeSet};

use crate::ucd::{CODE_POINTS, parse_ranges};

/// `JOINING_CODE`: HarfBuzz's `hb_arabic_joining_type_t` numbers.
pub fn joining_code(name: &str) -> Option<u32> {
    Some(match name {
        "JOINING_TYPE_U" => 0,
        "JOINING_TYPE_L" => 1,
        "JOINING_TYPE_R" => 2,
        "JOINING_TYPE_D" | "JOINING_TYPE_C" => 3,
        "JOINING_GROUP_ALAPH" => 4,
        "JOINING_GROUP_DALATH_RISH" => 5,
        "JOINING_TYPE_T" => 6,
        "JOINING_TYPE_X" => 7,
        _ => return None,
    })
}

/// The value of `joining_type` beyond the table (`JOINING_TYPE_X`).
pub const JOINING_DEFAULT: u32 = 7;

/// The data lines of `ArabicShaping.txt` as both scripts read them: after
/// the `####` line, not starting with `#`, split at `;` without removing
/// comments.
fn shaping_lines(text: &str) -> Result<Vec<Vec<&str>>, String> {
    let mut lines = text.lines();
    lines
        .by_ref()
        .find(|l| l.contains("##################"))
        .ok_or("no #### line in ArabicShaping.txt")?;
    Ok(lines
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split(';').map(str::trim).collect::<Vec<_>>())
        .filter(|f| f.len() > 1)
        .collect())
}

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("bad code point {s:?}: {e}"))
}

/// `joining_type` for every code point.
pub fn joining_types(arabic_shaping: &str) -> Result<Vec<u32>, String> {
    let mut out = vec![JOINING_DEFAULT; CODE_POINTS];
    for f in shaping_lines(arabic_shaping)? {
        let (cp, jt, group) = match f[..] {
            [cp, _, jt, group, ..] => (hex(cp)?, jt, group),
            _ => return Err(format!("short line: {f:?}")),
        };
        let name = if group == "ALAPH" || group == "DALATH RISH" {
            format!("JOINING_GROUP_{}", group.replace(' ', "_"))
        } else {
            format!("JOINING_TYPE_{jt}")
        };
        *out.get_mut(cp as usize)
            .ok_or(format!("bad code point {cp:X}"))? =
            joining_code(&name).ok_or(format!("unknown {name}"))?;
    }
    Ok(out)
}

/// The ISO 15924 tags of the scripts that have a character that joins
/// (not `T` or `U`), other than Common and Inherited, sorted by the long
/// name as HarfBuzz's list is.
pub fn joining_scripts(
    arabic_shaping: &str,
    scripts: &str,
    aliases: &BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    let (sc_lines, _) = parse_ranges(scripts)?;
    let mut sc: BTreeMap<u32, &str> = BTreeMap::new();
    for l in &sc_lines {
        let name = l.fields.first().ok_or("empty Scripts.txt line")?;
        (l.first..=l.last).for_each(|u| {
            sc.insert(u, name.as_str());
        });
    }
    let mut names = BTreeSet::new();
    for f in shaping_lines(arabic_shaping)? {
        let jt = f.get(2).ok_or(format!("short line: {f:?}"))?;
        if *jt == "T" || *jt == "U" {
            continue;
        }
        let u = hex(f[0])?;
        let name = *sc.get(&u).ok_or(format!("U+{u:04X} has no script"))?;
        if name != "Common" && name != "Inherited" {
            names.insert(name);
        }
    }
    names
        .into_iter()
        .map(|n| aliases.get(n).cloned().ok_or(format!("unknown script {n}")))
        .collect()
}

/// `LIGATURES`: the ligatures kept.
pub const LIGATURES: [u32; 67] = [
    0xF2EE, 0xFC08, 0xFC0E, 0xFC12, 0xFC32, 0xFC3F, 0xFC40, 0xFC41, 0xFC42, 0xFC43, 0xFC44, 0xFC4E,
    0xFC5E, 0xFC60, 0xFC61, 0xFC62, 0xFC6A, 0xFC6D, 0xFC6F, 0xFC70, 0xFC73, 0xFC75, 0xFC86, 0xFC8F,
    0xFC91, 0xFC94, 0xFC9C, 0xFC9D, 0xFC9E, 0xFC9F, 0xFCA1, 0xFCA2, 0xFCA3, 0xFCA4, 0xFCA8, 0xFCAA,
    0xFCAC, 0xFCB0, 0xFCC9, 0xFCCA, 0xFCCB, 0xFCCC, 0xFCCD, 0xFCCE, 0xFCCF, 0xFCD0, 0xFCD1, 0xFCD2,
    0xFCD3, 0xFCD5, 0xFCDA, 0xFCDB, 0xFCDC, 0xFCDD, 0xFD30, 0xFD88, 0xFEF5, 0xFEF6, 0xFEF7, 0xFEF8,
    0xFEF9, 0xFEFA, 0xFEFB, 0xFEFC, 0xF201, 0xF211, 0xF2EE,
];

/// The lines the script adds to `UnicodeData.txt`.
pub const EXTRA_LINES: [&str; 3] = [
    "F201;PUA ARABIC LIGATURE LELLAH ISOLATED FORM;Lo;0;AL;<isolated> 0644 0644 0647;;;;N;;;;;",
    "F211;PUA ARABIC LIGATURE LAM WITH MEEM WITH JEEM INITIAL FORM;Lo;0;AL;<initial> 0644 0645 062C;;;;N;;;;;",
    "F2EE;PUA ARABIC LIGATURE SHADDA WITH FATHATAN ISOLATED FORM;Lo;0;AL;<isolated> 0020 064B 0651;;;;N;;;;;",
];

/// The forms of a shaping table row, in its order.
pub const FORMS: [&str; 4] = ["initial", "medial", "final", "isolated"];

/// What `gen-arabic-table.py` writes besides the joining types.
#[allow(clippy::type_complexity)]
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Shaping {
    /// The first character of `shaping_table`.
    pub first: u32,
    /// `shaping_table`: initial, medial, final and isolated forms of each
    /// character from `first`, 0 for none.
    pub table: Vec<[u32; 4]>,
    /// `ligature_table`: a first component, then its `(second, ligature)`
    /// pairs.
    pub ligatures: Vec<(u32, Vec<(u32, u32)>)>,
    /// `ligature_mark_table`, as `ligatures`.
    pub mark_ligatures: Vec<(u32, Vec<(u32, u32)>)>,
    /// `ligature_3_table`: a first component, then its `(second, third,
    /// ligature)` triples.
    pub ligatures_3: Vec<(u32, Vec<(u32, u32, u32)>)>,
}

/// An insertion-ordered map, as Python's `dict`: a key keeps the place of
/// its first insertion.
fn put<K: PartialEq, V>(map: &mut Vec<(K, V)>, key: K, value: V) {
    match map.iter_mut().find(|(k, _)| *k == key) {
        Some(e) => e.1 = value,
        None => map.push((key, value)),
    }
}

/// `read_shaping_table` and `print_shaping_table` of the script.
pub fn shaping(unicode_data: &str) -> Result<Shaping, String> {
    // Single characters: their forms by name.
    let mut shapes: BTreeMap<u32, BTreeMap<&str, u32>> = BTreeMap::new();
    // Ligatures: components, then (form, ligature); `None` for a mark
    // ligature.
    #[allow(clippy::type_complexity)]
    let mut ligatures: Vec<(Vec<u32>, Vec<(Option<&str>, u32)>)> = Vec::new();
    for line in unicode_data.lines().chain(EXTRA_LINES) {
        let f: Vec<&str> = line.split(';').map(str::trim).collect();
        let dm = f.get(5).ok_or(format!("short line: {line}"))?;
        if !dm.starts_with('<') {
            continue;
        }
        let mut items = dm.split(' ');
        let tag = items.next().unwrap_or("");
        let shape = tag.get(1..tag.len().saturating_sub(1)).unwrap_or("");
        let mut items: Vec<u32> = items.map(hex).collect::<Result<_, _>>()?;
        let c = hex(f[0])?;
        let Some(&form) = FORMS.iter().find(|&&s| s == shape) else {
            continue;
        };
        if items.len() != 1 {
            let mut shape = Some(form);
            // Mark ligatures start with a space and are in visual order.
            if items.first() == Some(&0x20) {
                items = items[1..].iter().rev().copied().collect();
                shape = None;
            }
            if !LIGATURES.contains(&c) {
                continue;
            }
            match ligatures.iter_mut().find(|(k, _)| *k == items) {
                Some((_, forms)) => put(forms, shape, c),
                None => ligatures.push((items, vec![(shape, c)])),
            }
        } else {
            shapes.entry(items[0]).or_default().insert(form, c);
        }
    }

    let form = |u: u32, s: &str| -> Result<u32, String> {
        shapes
            .get(&u)
            .and_then(|m| m.get(s))
            .copied()
            .ok_or(format!("U+{u:04X} has no {s} form"))
    };
    let mut ligas_2: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    let mut ligas_mark_2: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    let mut ligas_3: BTreeMap<u32, Vec<(u32, u32, u32)>> = BTreeMap::new();
    for (key, forms) in &ligatures {
        for &(shape, c) in forms {
            match (key.as_slice(), shape) {
                (&[a, b, cc], Some(s)) => {
                    let liga = match s {
                        "isolated" => (form(a, "initial")?, form(b, "medial")?, form(cc, "final")?),
                        "final" => (form(a, "medial")?, form(b, "medial")?, form(cc, "final")?),
                        "initial" => (form(a, "initial")?, form(b, "medial")?, form(cc, "medial")?),
                        _ => return Err(format!("unexpected shape {s}")),
                    };
                    ligas_3.entry(liga.0).or_default().push((liga.1, liga.2, c));
                }
                (&[a, b], None) => ligas_mark_2.entry(a).or_default().push((b, c)),
                (&[a, b], Some(s)) => {
                    let liga = match s {
                        "isolated" => (form(a, "initial")?, form(b, "final")?),
                        "final" => (form(a, "medial")?, form(b, "final")?),
                        "initial" => (form(a, "initial")?, form(b, "medial")?),
                        _ => return Err(format!("unexpected shape {s}")),
                    };
                    ligas_2.entry(liga.0).or_default().push((liga.1, c));
                }
                _ => return Err(format!("unexpected ligature {key:X?} {shape:?}")),
            }
        }
    }

    let (&first, _) = shapes.first_key_value().ok_or("no shapes")?;
    let (&last, _) = shapes.last_key_value().ok_or("no shapes")?;
    let table = (first..=last)
        .map(|u| {
            let m = shapes.get(&u);
            FORMS.map(|s| m.and_then(|m| m.get(s)).copied().unwrap_or(0))
        })
        .collect();
    Ok(Shaping {
        first,
        table,
        ligatures: ligas_2.into_iter().collect(),
        mark_ligatures: ligas_mark_2.into_iter().collect(),
        ligatures_3: ligas_3.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAPING: &str = "\
# ArabicShaping-18.0.0.txt
# comment ; with ; semicolons
# ##################
0620; ARABIC LETTER KASHMIRI YEH; D; YEH
0621; HAMZA; U; No_Joining_Group
0710; SYRIAC LETTER ALAPH; R; ALAPH
0715; SYRIAC LETTER DALATH; R; DALATH RISH
200D; ZERO WIDTH JOINER; C; No_Joining_Group
1E900; ADLAM CAPITAL LETTER ALIF; D; No_Joining_Group
0640; TATWEEL; C; No_Joining_Group
";

    #[test]
    fn joining_types_and_groups() {
        let j = joining_types(SHAPING).unwrap();
        assert_eq!(j[0x620], 3);
        assert_eq!(j[0x621], 0);
        assert_eq!(j[0x710], 4);
        assert_eq!(j[0x715], 5);
        assert_eq!(j[0x200D], 3);
        assert_eq!(j[0x41], JOINING_DEFAULT);
        assert!(joining_types("0620; X; D; YEH\n").is_err());
    }

    #[test]
    fn scripts_that_join() {
        let scripts = "0600..06FF ; Arabic\n0700..074F ; Syriac\n0640 ; Common\n200D ; Inherited\n1E900..1E95F ; Adlam\n";
        let aliases: BTreeMap<String, String> = [
            ("Arabic", "Arab"),
            ("Syriac", "Syrc"),
            ("Adlam", "Adlm"),
            ("Common", "Zyyy"),
            ("Inherited", "Zinh"),
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .into_iter()
        .collect();
        // HAMZA joins on no side; TATWEEL and ZWJ are Common and Inherited.
        assert_eq!(
            joining_scripts(SHAPING, scripts, &aliases).unwrap(),
            ["Adlm", "Arab", "Syrc"]
        );
    }

    #[test]
    fn forms_and_ligatures() {
        let data = "\
0628;ARABIC LETTER BEH;Lo;0;AL;;;;;N;;;;;
0644;ARABIC LETTER LAM;Lo;0;AL;;;;;N;;;;;
FE8F;ARABIC LETTER BEH ISOLATED FORM;Lo;0;AL;<isolated> 0628;;;;N;;;;;
FE90;ARABIC LETTER BEH FINAL FORM;Lo;0;AL;<final> 0628;;;;N;;;;;
FE91;ARABIC LETTER BEH INITIAL FORM;Lo;0;AL;<initial> 0628;;;;N;;;;;
FE92;ARABIC LETTER BEH MEDIAL FORM;Lo;0;AL;<medial> 0628;;;;N;;;;;
FEE1;ARABIC LETTER MEEM ISOLATED FORM;Lo;0;AL;<isolated> 0645;;;;N;;;;;
FEE2;ARABIC LETTER MEEM FINAL FORM;Lo;0;AL;<final> 0645;;;;N;;;;;
FEE3;ARABIC LETTER MEEM INITIAL FORM;Lo;0;AL;<initial> 0645;;;;N;;;;;
FEE4;ARABIC LETTER MEEM MEDIAL FORM;Lo;0;AL;<medial> 0645;;;;N;;;;;
FC08;ARABIC LIGATURE BEH WITH MEEM ISOLATED FORM;Lo;0;AL;<isolated> 0628 0645;;;;N;;;;;
FC9F;ARABIC LIGATURE BEH WITH MEEM INITIAL FORM;Lo;0;AL;<initial> 0628 0645;;;;N;;;;;
FC09;ARABIC LIGATURE NOT KEPT;Lo;0;AL;<isolated> 0628 0628;;;;N;;;;;
FC5E;ARABIC LIGATURE SHADDA WITH DAMMATAN ISOLATED FORM;Lo;0;AL;<isolated> 0020 064C 0651;;;;N;;;;;
FB50;ARABIC LETTER ALEF WASLA ISOLATED FORM;Lo;0;AL;<isolated> 0671;;;;N;;;;;
FE70;ARABIC FATHATAN ISOLATED FORM;Lo;0;AL;<isolated> 0020 064B;;;;N;;;;;
";
        // The PUA lines the script adds need the forms of LAM, HEH and JEEM;
        // without them the script fails, and so does this.
        assert!(shaping(data).is_err());
        let full = format!(
            "{data}{}",
            "\
FEDD;LAM ISOLATED;Lo;0;AL;<isolated> 0644;;;;N;;;;;
FEDE;LAM FINAL;Lo;0;AL;<final> 0644;;;;N;;;;;
FEDF;LAM INITIAL;Lo;0;AL;<initial> 0644;;;;N;;;;;
FEE0;LAM MEDIAL;Lo;0;AL;<medial> 0644;;;;N;;;;;
FEEA;HEH FINAL;Lo;0;AL;<final> 0647;;;;N;;;;;
FEA0;JEEM MEDIAL;Lo;0;AL;<medial> 062C;;;;N;;;;;
"
        );
        let s = shaping(&full).unwrap();
        assert_eq!(s.first, 0x0628);
        let row = |u: u32| s.table[(u - s.first) as usize];
        assert_eq!(row(0x0628), [0xFE91, 0xFE92, 0xFE90, 0xFE8F]);
        assert_eq!(row(0x0629), [0, 0, 0, 0]);
        assert_eq!(row(0x0671), [0, 0, 0, 0xFB50]);
        assert_eq!(s.table.len(), 0x0671 - 0x0628 + 1);
        // Isolated BEH+MEEM is initial BEH + final MEEM; the initial ligature
        // is initial BEH + medial MEEM. FC09 is not in LIGATURES.
        assert_eq!(
            s.ligatures,
            vec![(0xFE91, vec![(0xFEE2, 0xFC08), (0xFEE4, 0xFC9F)])]
        );
        // Mark ligatures are reversed and keep their order of appearance
        // (the PUA line comes last); FE70 has one component.
        assert_eq!(
            s.mark_ligatures,
            vec![(0x0651, vec![(0x064C, 0xFC5E), (0x064B, 0xF2EE)])]
        );
        assert_eq!(
            s.ligatures_3,
            vec![(
                0xFEDF,
                vec![(0xFEE0, 0xFEEA, 0xF201), (0xFEE4, 0xFEA0, 0xF211)]
            ),]
        );
    }
}
