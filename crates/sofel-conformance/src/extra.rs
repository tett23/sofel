//! Registration and bounded glyph sampling for the fixed extra-font corpus
//! (ADR 0031).

use std::collections::{BTreeMap, BTreeSet};

/// One registered font and its license and intended test features.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtraFont {
    pub id: String,
    pub source_id: String,
    pub member: String,
    pub path: String,
    pub sha256: String,
    pub license_id: String,
    pub features: Vec<String>,
}

/// Validated records from `fonts.lock` and `conformance/fonts.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    pub fonts: Vec<ExtraFont>,
}

fn rows(text: &str, columns: usize) -> Result<Vec<Vec<&str>>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(line_no, line)| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != columns || fields.iter().any(|field| field.is_empty()) {
                Err(format!(
                    "line {}: expected {columns} nonempty columns",
                    line_no + 1
                ))
            } else {
                Ok(fields)
            }
        })
        .collect()
}

fn safe_path(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains('\\')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl Corpus {
    /// Parses and validates both manifests before any font is opened.
    pub fn parse(lock: &str, manifest: &str) -> Result<Self, String> {
        let lock_rows = rows(lock, 5)?;
        let font_rows = rows(manifest, 7)?;
        if lock_rows.is_empty() || font_rows.is_empty() {
            return Err("empty corpus".to_string());
        }
        let mut files = BTreeMap::<&str, (&str, &str)>::new();
        let mut hashes = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for row in lock_rows {
            let [id, url, hash, bytes, path] = row.as_slice() else {
                unreachable!()
            };
            if !url.starts_with("https://")
                || !sha256(hash)
                || bytes.parse::<u64>().ok().filter(|n| *n > 0).is_none()
                || !safe_path(path)
            {
                return Err(format!("invalid source {id}"));
            }
            if files.insert(id, (hash, path)).is_some()
                || !hashes.insert(*hash)
                || !paths.insert(*path)
            {
                return Err(format!("duplicate source {id}"));
            }
        }
        let mut ids = BTreeSet::new();
        let mut font_hashes = BTreeSet::new();
        let mut font_paths = BTreeSet::new();
        let fonts = font_rows
            .into_iter()
            .map(|row| {
                let [id, source, member, path, hash, license, features] = row.as_slice() else {
                    unreachable!()
                };
                let Some(&(source_hash, source_path)) = files.get(source) else {
                    return Err(format!("missing source {source}"));
                };
                let Some(&(_, license_path)) = files.get(license) else {
                    return Err(format!("missing license {license}"));
                };
                if !license_path.starts_with("licenses/")
                    || !safe_path(path)
                    || !sha256(hash)
                    || (member != &"-" && !safe_path(member))
                    || (member == &"-" && (path != &source_path || hash != &source_hash))
                {
                    return Err(format!("invalid font {id}"));
                }
                let feature_list = features.split(',').map(str::to_string).collect::<Vec<_>>();
                if feature_list.iter().any(|feature| {
                    !matches!(
                        feature.as_str(),
                        "shape"
                            | "rtl"
                            | "indic"
                            | "use"
                            | "variable"
                            | "cff"
                            | "cff2"
                            | "math"
                            | "ttc"
                            | "cjk"
                            | "vertical"
                            | "avar2"
                            | "colr"
                            | "sbix"
                            | "svg"
                            | "metrics"
                            | "outline"
                    )
                }) || feature_list.iter().collect::<BTreeSet<_>>().len() != feature_list.len()
                {
                    return Err(format!("invalid features for {id}"));
                }
                if !ids.insert(*id) || !font_hashes.insert(*hash) || !font_paths.insert(*path) {
                    return Err(format!("duplicate font {id}"));
                }
                Ok(ExtraFont {
                    id: (*id).to_string(),
                    source_id: (*source).to_string(),
                    member: (*member).to_string(),
                    path: (*path).to_string(),
                    sha256: (*hash).to_string(),
                    license_id: (*license).to_string(),
                    features: feature_list,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self { fonts })
    }
}

/// Selects up to 128 glyphs, retaining all mandatory case glyphs and the
/// evenly spaced candidates of ADR 0031. The returned glyphs are sorted.
pub fn sample_glyphs(total: u32, mandatory: &[u32]) -> Result<Vec<u32>, String> {
    let cap = total.min(128) as usize;
    let mut selected = mandatory.iter().copied().collect::<BTreeSet<_>>();
    if selected.len() > cap || selected.iter().any(|&glyph| glyph >= total) {
        return Err("mandatory glyphs exceed the font or 128-glyph limit".to_string());
    }
    if total == 0 {
        return Ok(Vec::new());
    }
    selected.insert(0);
    selected.insert(total - 1);
    if selected.len() > cap {
        return Err("mandatory glyphs exceed the 128-glyph limit".to_string());
    }
    if cap >= 2 {
        for i in 0..cap {
            let glyph = ((i as u64 * u64::from(total - 1)) / (cap as u64 - 1)) as u32;
            selected.insert(glyph);
            if selected.len() == cap {
                break;
            }
        }
    }
    Ok(selected.into_iter().collect())
}

/// FNV-1a of the nominal glyph ID (zero when unmapped) for every Unicode
/// scalar from U+0000 through U+10FFFF, excluding surrogates. Each ID is
/// hashed as four little-endian bytes, like `hb-dump --cmap-digest`.
pub fn nominal_digest(mut glyph: impl FnMut(u32) -> u32) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for scalar in 0..=0x10_ffff {
        if (0xd800..=0xdfff).contains(&scalar) {
            continue;
        }
        for byte in glyph(scalar).to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}
