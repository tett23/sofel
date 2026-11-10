//! The generator of sofel's Unicode tables (ADR 0019).
//!
//! It reads the Unicode Character Database files fetched by
//! `scripts/fetch-ucd.sh` and the inputs in HarfBuzz's repository fetched
//! by `scripts/fetch-testdata.sh`, packs each property into a multi-level
//! table, checks every code point of every table, and writes Rust source.

pub mod arabic;
pub mod emit;
pub mod hbsrc;
pub mod indic;
pub mod pack;
pub mod props;
pub mod pua;
pub mod ucd;
pub mod use_table;
pub mod vowel;

use std::fmt::Write;

use emit::Table;

/// A line of `ucd.lock`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockLine {
    /// Path under `target/ucd`.
    pub path: String,
    /// Where it is fetched from.
    pub url: String,
    /// Its SHA-256.
    pub sha256: String,
}

/// Read `ucd.lock` (or `testdata.lock`, whose third field is a commit).
pub fn parse_lock(text: &str) -> Result<Vec<LockLine>, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [path, url, sha256] => Ok(LockLine {
                path: path.to_string(),
                url: url.to_string(),
                sha256: sha256.to_string(),
            }),
            _ => Err(format!("bad lock line: {l}")),
        })
        .collect()
}

/// The Unicode version in the URL of a UCD file
/// (`.../Public/18.0.0/ucd/...`).
pub fn unicode_version(url: &str) -> Result<(u8, u8, u8), String> {
    let v = url
        .split('/')
        .skip_while(|s| *s != "Public")
        .nth(1)
        .ok_or(format!("no version in {url}"))?;
    let n: Vec<u8> = v
        .split('.')
        .map(|p| p.parse::<u8>().map_err(|e| format!("bad version {v}: {e}")))
        .collect::<Result<_, _>>()?;
    match n[..] {
        [a, b, c] => Ok((a, b, c)),
        _ => Err(format!("bad version {v}")),
    }
}

/// Pack, check every code point, and describe one table.
pub fn packed_table<'a>(
    name: &'a str,
    doc: &'a str,
    values: &[u32],
    default: u32,
) -> Result<Table<'a>, String> {
    let packed = pack::pack(values, default);
    if let Some(p) = &packed {
        pack::verify(name, p, values, default)?;
    } else if values.iter().any(|&v| v != default) {
        return Err(format!("{name}: no table although values differ"));
    }
    Ok(Table {
        name,
        doc,
        packed,
        default,
    })
}

/// A generated file: the header, the getters, then the tables.
pub fn file(header: &[String], tables: &[Table], extra: &str) -> String {
    let mut out = String::new();
    header.iter().for_each(|l| {
        let sep = if l.is_empty() { "" } else { " " };
        writeln!(out, "//{sep}{l}").unwrap();
    });
    writeln!(out, "//").unwrap();
    writeln!(out, "// Table sizes:").unwrap();
    tables
        .iter()
        .for_each(|t| writeln!(out, "//   {}", emit::size_line(t)).unwrap());
    writeln!(out).unwrap();
    let widths = tables.iter().flat_map(emit::widths).collect();
    out.push_str(&emit::getters(&widths));
    tables.iter().for_each(|t| {
        writeln!(out).unwrap();
        out.push_str(&emit::table(t));
    });
    if !extra.is_empty() {
        writeln!(out).unwrap();
        out.push_str(extra);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_lines() {
        let l = parse_lock("# c\n\na.txt https://x/Public/18.0.0/ucd/a.txt abc\n").unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].path, "a.txt");
        assert_eq!(unicode_version(&l[0].url).unwrap(), (18, 0, 0));
        assert!(parse_lock("a b\n").is_err());
    }
}
