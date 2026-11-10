//! Numbers HarfBuzz's generated shaper tables take from HarfBuzz's own
//! sources (ADR 0019): the `export NAME = N;` lines of the Ragel machines
//! (`src/hb-ot-shaper-*-machine.rl`) and the `POS_NAME = N` members of
//! `ot_position_t` (`src/hb-ot-shaper-indic.hh`). HarfBuzz is under the
//! MIT license (THIRD_PARTY_NOTICES.md).

use std::collections::BTreeMap;

use crate::ucd::{CODE_POINTS, RangeLine};

fn parse_assignment(body: &str) -> Option<(String, u32)> {
    let (name, value) = body.split_once('=')?;
    let value = value.trim().trim_end_matches([';', ',']).trim();
    Some((name.trim().to_string(), value.parse().ok()?))
}

/// The `export NAME = N;` lines of a Ragel machine.
pub fn ragel_exports(text: &str) -> Result<BTreeMap<String, u32>, String> {
    text.lines()
        .filter_map(|l| {
            let body = l.split('#').next().unwrap_or("").trim();
            body.strip_prefix("export ")
        })
        .map(|body| parse_assignment(body).ok_or(format!("bad export line: {body}")))
        .collect()
}

/// The `POS_NAME = N` members of `ot_position_t`, keyed by `NAME`.
pub fn indic_positions(text: &str) -> Result<BTreeMap<String, u32>, String> {
    let start = text
        .find("enum ot_position_t")
        .ok_or("no ot_position_t in hb-ot-shaper-indic.hh")?;
    let body = text
        .get(start..)
        .and_then(|t| t.split_once('{'))
        .and_then(|(_, t)| t.split_once('}'))
        .map(|(b, _)| b)
        .ok_or("unterminated ot_position_t")?;
    body.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("POS_"))
        .map(|l| parse_assignment(l).ok_or(format!("bad position: {l}")))
        .collect()
}

/// A code point map of one field of a UCD range file, later lines
/// winning, as the Python dictionaries of HarfBuzz's generators fill it.
pub fn field_map<'a>(
    out: &mut [Option<&'a str>],
    lines: &'a [RangeLine],
    field: usize,
) -> Result<(), String> {
    for l in lines {
        let v = l
            .fields
            .get(field)
            .ok_or(format!("no field {field} for U+{:04X}", l.first))?;
        out.get_mut(l.first as usize..=l.last as usize)
            .ok_or(format!("range beyond U+10FFFF: {:04X}", l.first))?
            .iter_mut()
            .for_each(|x| *x = Some(v.as_str()));
    }
    Ok(())
}

/// An empty code point map.
pub fn empty_map<'a>() -> Vec<Option<&'a str>> {
    vec![None; CODE_POINTS]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ucd::parse_ranges;

    #[test]
    fn exports() {
        let e = ragel_exports(
            "%%{\n  machine x;\n  write exports;\n}%%\n\nexport X    = 0;\nexport DB   = 3;\t# Dot below\t= OT_N\n#export Q = 9;\n",
        )
        .unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e["X"], 0);
        assert_eq!(e["DB"], 3);
        assert!(ragel_exports("export X = y;\n").is_err());
    }

    #[test]
    fn positions() {
        let p = indic_positions(
            "enum ot_position_t {\n  POS_START = 0,\n\n  POS_PRE_M = 2,\n  POS_END = 14\n};\nPOS_X = 99,\n",
        )
        .unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p["START"], 0);
        assert_eq!(p["END"], 14);
    }

    #[test]
    fn later_lines_win() {
        let (lines, _) = parse_ranges("0041..0043 ; A\n0042 ; B # c\n").unwrap();
        let mut m = empty_map();
        field_map(&mut m, &lines, 0).unwrap();
        assert_eq!(
            (m[0x41], m[0x42], m[0x43], m[0x44]),
            (Some("A"), Some("B"), Some("A"), None)
        );
        assert!(field_map(&mut m, &lines, 1).is_err());
    }
}
