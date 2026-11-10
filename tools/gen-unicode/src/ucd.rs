//! Readers of the Unicode Character Database text files (ADR 0019).

use std::collections::BTreeMap;

/// Number of code points, U+0000..=U+10FFFF.
pub const CODE_POINTS: usize = 0x11_0000;

/// A line of a range file: `first..last ; field ; field # comment`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeLine {
    /// First code point.
    pub first: u32,
    /// Last code point.
    pub last: u32,
    /// The fields after the code points, trimmed.
    pub fields: Vec<String>,
}

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s.trim(), 16).map_err(|e| format!("bad code point {s:?}: {e}"))
}

fn range(s: &str) -> Result<(u32, u32), String> {
    match s.trim().split_once("..") {
        Some((a, b)) => Ok((hex(a)?, hex(b)?)),
        None => hex(s).map(|c| (c, c)),
    }
}

fn range_line(body: &str) -> Result<RangeLine, String> {
    let mut parts = body.split(';');
    let (first, last) = range(parts.next().unwrap_or(""))?;
    let fields = parts.map(|f| f.trim().to_string()).collect();
    Ok(RangeLine {
        first,
        last,
        fields,
    })
}

/// Read a file of the UCD's range format. Returns the data lines and the
/// `# @missing:` lines, each in file order.
pub fn parse_ranges(text: &str) -> Result<(Vec<RangeLine>, Vec<RangeLine>), String> {
    let mut data = Vec::new();
    let mut missing = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("# @missing:") {
            missing.push(range_line(rest)?);
            continue;
        }
        let body = line.split('#').next().unwrap_or("").trim();
        if !body.is_empty() {
            data.push(range_line(body)?);
        }
    }
    Ok((data, missing))
}

/// One code point or range of `UnicodeData.txt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnicodeDataEntry {
    /// First code point.
    pub first: u32,
    /// Last code point (the `<..., Last>` line of a range).
    pub last: u32,
    /// General_Category, two letters.
    pub gc: String,
    /// Canonical_Combining_Class.
    pub ccc: u8,
    /// Canonical decomposition mapping; `None` if there is none or it is a
    /// compatibility one (starts with `<`).
    pub canonical: Option<Vec<u32>>,
}

/// Read `UnicodeData.txt`, joining `<..., First>` and `<..., Last>` lines
/// into ranges.
pub fn parse_unicode_data(text: &str) -> Result<Vec<UnicodeDataEntry>, String> {
    let mut out: Vec<UnicodeDataEntry> = Vec::new();
    let mut open: Option<UnicodeDataEntry> = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 6 {
            return Err(format!("short UnicodeData line: {line}"));
        }
        let cp = hex(f[0])?;
        let ccc = f[3]
            .parse::<u8>()
            .map_err(|e| format!("bad ccc in {line}: {e}"))?;
        let dm = f[5].trim();
        let canonical = if dm.is_empty() || dm.starts_with('<') {
            None
        } else {
            Some(
                dm.split_whitespace()
                    .map(hex)
                    .collect::<Result<Vec<_>, _>>()?,
            )
        };
        let entry = UnicodeDataEntry {
            first: cp,
            last: cp,
            gc: f[2].to_string(),
            ccc,
            canonical,
        };
        if f[1].ends_with(", First>") {
            open = Some(entry);
        } else if f[1].ends_with(", Last>") {
            let start = open.take().ok_or(format!("Last without First: {line}"))?;
            out.push(UnicodeDataEntry { last: cp, ..start });
        } else {
            out.push(entry);
        }
    }
    match open {
        Some(e) => Err(format!("First without Last at U+{:04X}", e.first)),
        None => Ok(out),
    }
}

/// The long-to-short names of the `sc` lines of `PropertyValueAliases.txt`
/// (every alias maps to the short name).
pub fn parse_script_aliases(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter_map(|l| {
            let f: Vec<&str> = l.split(';').map(str::trim).collect();
            (f.first() == Some(&"sc") && f.len() >= 3).then(|| {
                f[1..]
                    .iter()
                    .map(|alias| (alias.to_string(), f[1].to_string()))
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
        .collect()
}

/// A value for every code point: `default`, then each line's value.
pub fn fill<T: Clone>(
    default: T,
    lines: &[RangeLine],
    value: impl Fn(&RangeLine) -> Result<Option<T>, String>,
) -> Result<Vec<T>, String> {
    let mut out = vec![default; CODE_POINTS];
    for l in lines {
        if let Some(v) = value(l)? {
            let slot = out
                .get_mut(l.first as usize..=l.last as usize)
                .ok_or(format!(
                    "range beyond U+10FFFF: {:04X}..{:04X}",
                    l.first, l.last
                ))?;
            slot.iter_mut().for_each(|x| *x = v.clone());
        }
    }
    Ok(out)
}

/// The value of the single `@missing` line for the whole code space.
pub fn missing_default(missing: &[RangeLine]) -> Result<String, String> {
    match missing {
        [m] if m.first == 0 && m.last == 0x10_FFFF => m
            .fields
            .first()
            .cloned()
            .ok_or("empty @missing line".to_string()),
        _ => Err(format!(
            "expected one @missing line for the whole code space: {missing:?}"
        )),
    }
}

/// True for the code points listed with `property` in a binary-property
/// file (`DerivedNormalizationProps.txt`, `emoji-data.txt`).
pub fn binary_property(lines: &[RangeLine], property: &str) -> Result<Vec<bool>, String> {
    fill(false, lines, |l| {
        Ok((l.fields.first().map(String::as_str) == Some(property)).then_some(true))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_data_ranges_are_expanded() {
        let text = "\
0041;LATIN CAPITAL LETTER A;Lu;0;L;;;;;N;;;0061;
00C5;LATIN CAPITAL LETTER A WITH RING ABOVE;Lu;0;L;0041 030A;;;;N;;;;00E5;
AC00;<Hangul Syllable, First>;Lo;0;L;;;;;N;;;;;
D7A3;<Hangul Syllable, Last>;Lo;0;L;;;;;N;;;;;
";
        let e = parse_unicode_data(text).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(
            (e[2].first, e[2].last, e[2].gc.as_str()),
            (0xAC00, 0xD7A3, "Lo")
        );
        assert_eq!(e[1].canonical, Some(vec![0x41, 0x30A]));
    }

    #[test]
    fn compatibility_decompositions_are_dropped() {
        let text = "\
00A0;NO-BREAK SPACE;Zs;0;CS;<noBreak> 0020;;;;N;NON-BREAKING SPACE;;;;
212B;ANGSTROM SIGN;Lu;0;L;00C5;;;;N;ANGSTROM UNIT;;;00E5;
0301;COMBINING ACUTE ACCENT;Mn;230;NSM;;;;;N;NON-SPACING ACUTE;;;;
";
        let e = parse_unicode_data(text).unwrap();
        assert_eq!(e[0].canonical, None);
        assert_eq!(e[1].canonical, Some(vec![0xC5]));
        assert_eq!(e[2].ccc, 230);
    }

    #[test]
    fn unmatched_range_lines_fail() {
        assert!(parse_unicode_data("AC00;<Hangul Syllable, First>;Lo;0;L;;;;;N;;;;;\n").is_err());
        assert!(parse_unicode_data("D7A3;<Hangul Syllable, Last>;Lo;0;L;;;;;N;;;;;\n").is_err());
    }

    #[test]
    fn script_ranges() {
        let text = "\
# Scripts-18.0.0.txt
# @missing: 0000..10FFFF; Unknown

0000..001F    ; Common # Cc  [32] <control-0000>..<control-001F>
0041..005A    ; Latin # L&  [26] LATIN CAPITAL LETTER A..LATIN CAPITAL LETTER Z
0301          ; Inherited # Mn       COMBINING ACUTE ACCENT
";
        let (data, missing) = parse_ranges(text).unwrap();
        assert_eq!(missing_default(&missing).unwrap(), "Unknown");
        let sc = fill("Unknown".to_string(), &data, |l| {
            Ok(Some(l.fields[0].clone()))
        })
        .unwrap();
        assert_eq!(sc[0x1F], "Common");
        assert_eq!(sc[0x20], "Unknown");
        assert_eq!(sc[0x41], "Latin");
        assert_eq!(sc[0x5A], "Latin");
        assert_eq!(sc[0x5B], "Unknown");
        assert_eq!(sc[0x301], "Inherited");
        assert_eq!(sc[0x10FFFF], "Unknown");
    }

    #[test]
    fn missing_lines() {
        let text = "\
# @missing: 0000..10FFFF; R
0000..001F     ; R  # Cc    [32] <control-0000>..<control-001F>
3001           ; Tu # Po         IDEOGRAPHIC COMMA
";
        let (data, missing) = parse_ranges(text).unwrap();
        assert_eq!(missing_default(&missing).unwrap(), "R");
        assert_eq!(data[1].fields, vec!["Tu"]);
        let partial = parse_ranges("# @missing: 0000..00FF; X\n").unwrap().1;
        assert!(missing_default(&partial).is_err());
        assert!(missing_default(&[]).is_err());
    }

    #[test]
    fn binary_properties_pick_their_lines() {
        let text = "\
0340..0341    ; Full_Composition_Exclusion # Mn   [2]
0958          ; Full_Composition_Exclusion # Lo
0300          ; NFD_QC; N # Mn
";
        let (data, _) = parse_ranges(text).unwrap();
        let ce = binary_property(&data, "Full_Composition_Exclusion").unwrap();
        assert!(ce[0x340] && ce[0x341] && ce[0x958]);
        assert!(!ce[0x300] && !ce[0x342]);
    }

    #[test]
    fn script_aliases() {
        let text = "\
sc ; Latn                             ; Latin
sc ; Zinh                             ; Inherited                        ; Qaai
gc ; Lu                               ; Uppercase_Letter
";
        let a = parse_script_aliases(text);
        assert_eq!(a["Latin"], "Latn");
        assert_eq!(a["Inherited"], "Zinh");
        assert_eq!(a["Qaai"], "Zinh");
        assert!(!a.contains_key("Uppercase_Letter"));
    }
}
