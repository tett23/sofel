//! The derived sample fonts of the gallery: STIX Two Math with a glyph
//! that uses the CFF `random` operator (sofel ADR 0083), and Caveat with
//! an OpenType `rand` feature over its existing alternates (sofel ADR
//! 0084).
//!
//! Each font is derived in memory from the fixed original, checked
//! against the SHA-256 of its lock, and written to `target/derived-fonts`
//! with the original license next to it. The gallery output never holds
//! them (OFL 1.1, condition 5: the derived fonts are not distributed).

use std::fs;
use std::path::Path;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::cff::dict::TopDict;
use sofel_font::read::cff::{Cff1, Index, Version};
use sofel_font::read::layout::{LangSys, LayoutTable};
use sofel_font::read::sfnt::Sfnt;
use sofel_write::{Errors, ObjIdx, Serializer, Whence};

/// `random 1000 mul random 1000 mul rmoveto 500 0 rlineto 0 500 rlineto
/// endchar` (ADR 0083).
pub const CFF_RANDOM_PROGRAM: [u8; 26] = [
    12, 23, 28, 3, 0xE8, 12, 24, 12, 23, 28, 3, 0xE8, 12, 24, 21, 28, 1, 0xF4, 139, 5, 139, 28, 1,
    0xF4, 5, 14,
];
const ENDCHAR: u8 = 14;

/// Where the derived fonts are written, from the repository root.
pub const DERIVED_DIR: &str = "target/derived-fonts";
/// The ADR 0083 font, in [`DERIVED_DIR`].
pub const CFF_RANDOM_FILE: &str = "sofel-random-sample.otf";
/// The ADR 0084 font, in [`DERIVED_DIR`].
pub const RAND_FILE: &str = "sofel-rand-sample.ttf";

const CFF: Tag = Tag::new(b"CFF ");
const GSUB: Tag = Tag::new(b"GSUB");
const NAME: Tag = Tag::new(b"name");

// SHA-256 (FIPS 180-4).

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

fn compress(state: [u32; 8], block: &[u8; 64]) -> [u32; 8] {
    let mut w = [0u32; 64];
    for (word, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
        *word = u32::from_be_bytes(*bytes);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
    for (k, w) in K.iter().zip(w) {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(*k)
            .wrapping_add(w);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
    }
    let mut out = state;
    for (o, v) in out.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *o = o.wrapping_add(v);
    }
    out
}

/// The SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let bits = (data.len() as u64).wrapping_mul(8);
    let mut tail = data[data.len() - data.len() % 64..].to_vec();
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&bits.to_be_bytes());
    let state = data
        .as_chunks::<64>()
        .0
        .iter()
        .chain(tail.as_chunks::<64>().0)
        .fold(H0, compress);
    let mut out = [0u8; 32];
    for (o, v) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *o = v.to_be_bytes();
    }
    out
}

/// [`sha256`] in lowercase hexadecimal, as in the locks.
pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 and cache path of `id` in a lock (`fonts.lock` or
/// `gallery-fonts.lock`: id, URL, SHA-256, bytes, cache path).
fn lock_entry(lock: &Path, id: &str) -> Result<(String, String), String> {
    let text =
        fs::read_to_string(lock).map_err(|e| format!("cannot read {}: {e}", lock.display()))?;
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .find(|f| f.first() == Some(&id))
        .and_then(|f| Some((f.get(2)?.to_string(), f.get(4)?.to_string())))
        .ok_or_else(|| format!("no {id} in {}", lock.display()))
}

/// The SHA-256 of `id` in a lock.
pub fn lock_sha256(lock: &Path, id: &str) -> Result<String, String> {
    lock_entry(lock, id).map(|(sha, _)| sha)
}

fn check_sha256(base: &[u8], expected: &str) -> Result<(), String> {
    let actual = sha256_hex(base);
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "SHA-256 of the original font is {actual}, not {expected}"
        ))
    }
}

/// The tables of a font in directory order.
fn tables(font: &[u8]) -> Result<Vec<(Tag, &[u8])>, String> {
    let sfnt = Sfnt::new(font).map_err(|e| format!("sfnt: {e:?}"))?;
    sfnt.records()
        .map(|r| {
            r.data(font)
                .map(|d| (r.tag, d))
                .ok_or_else(|| "table outside the file".to_string())
        })
        .collect()
}

fn table<'a>(all: &[(Tag, &'a [u8])], tag: Tag) -> Result<&'a [u8], String> {
    all.iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, d)| *d)
        .ok_or_else(|| format!("no {} table", String::from_utf8_lossy(&tag.0)))
}

/// Rebuilds the font with `replaced` tables; the checksums and
/// `checkSumAdjustment` are computed again.
fn rebuild(all: &[(Tag, &[u8])], replaced: &[(Tag, &[u8])]) -> Result<Vec<u8>, String> {
    let list: Vec<(Tag, &[u8])> = all
        .iter()
        .map(|&(tag, data)| {
            replaced
                .iter()
                .find(|(t, _)| *t == tag)
                .map_or((tag, data), |&r| r)
        })
        .collect();
    sofel_write::build_sfnt(&list).map_err(|e| format!("sfnt: {e:?}"))
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, String> {
    data.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| "truncated table".to_string())
}

fn to_u16(v: usize, what: &str) -> Result<u16, String> {
    u16::try_from(v).map_err(|_| format!("{what} does not fit in 16 bits"))
}

/// The `name` table with IDs 1, 4 and 16 set to `family`, 3 and 6 to
/// `postscript` and 17 to `Regular`; the other records are kept. Only
/// format 0 is read.
fn rename(name: &[u8], family: &str, postscript: &str) -> Result<Vec<u8>, String> {
    if u16_at(name, 0)? != 0 {
        return Err("name table format is not 0".into());
    }
    let count = usize::from(u16_at(name, 2)?);
    let storage = usize::from(u16_at(name, 4)?);
    let mut records = Vec::new();
    let mut strings = Vec::new();
    for i in 0..count {
        let r = 6 + 12 * i;
        let field = |k: usize| u16_at(name, r + 2 * k);
        let (platform, id) = (field(0)?, field(3)?);
        let (len, off) = (usize::from(field(4)?), usize::from(field(5)?));
        let text = match id {
            1 | 4 | 16 => Some(family),
            3 | 6 => Some(postscript),
            17 => Some("Regular"),
            _ => None,
        };
        let bytes = match text {
            Some(t) if platform == 1 => t.as_bytes().to_vec(),
            Some(t) => t.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            None => name
                .get(storage + off..storage + off + len)
                .ok_or("name string outside the table")?
                .to_vec(),
        };
        records.extend_from_slice(name.get(r..r + 8).ok_or("truncated name table")?);
        records.extend_from_slice(&to_u16(bytes.len(), "name string")?.to_be_bytes());
        records.extend_from_slice(&to_u16(strings.len(), "name storage")?.to_be_bytes());
        strings.extend_from_slice(&bytes);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&to_u16(count, "name count")?.to_be_bytes());
    out.extend_from_slice(&to_u16(6 + records.len(), "name storage offset")?.to_be_bytes());
    out.extend_from_slice(&records);
    out.extend_from_slice(&strings);
    Ok(out)
}

/// STIX Two Math with the charstring of `R` replaced by
/// [`CFF_RANDOM_PROGRAM`], padded with `endchar` to its length, and
/// renamed `Sofel Random Sample` (sofel ADR 0083).
///
/// # Errors
///
/// When the SHA-256 of `base` is not `sha256`, the font has no
/// name-keyed CFF or `R`, or the charstring of `R` is shorter than the
/// program.
pub fn cff_random_sample(base: &[u8], sha256: &str) -> Result<Vec<u8>, String> {
    check_sha256(base, sha256)?;
    let all = tables(base)?;
    let cff = table(&all, CFF)?;
    let c = Cff1::new(cff).map_err(|e| format!("CFF: {e:?}"))?;
    let top = c.top_dicts().get(0).ok_or("CFF: no Top DICT")?;
    let top = TopDict::parse(top, Version::Cff).map_err(|e| format!("CFF Top DICT: {e:?}"))?;
    if top.is_cid() {
        return Err("CFF is CID-keyed".into());
    }
    let at = usize::try_from(top.char_strings).map_err(|_| "CharStrings offset")?;
    let index = Index::new(
        cff.get(at..).ok_or("CharStrings outside CFF")?,
        Version::Cff,
    )
    .map_err(|e| format!("CharStrings: {e:?}"))?;
    let font = Font::new(base, 0).map_err(|e| format!("font: {e:?}"))?;
    let glyph = font
        .nominal_glyph(u32::from('R'))
        .ok_or("no glyph for R")?
        .0;
    let offset = |g: u32| {
        index
            .offset(g)
            .and_then(|o| usize::try_from(o).ok())
            .ok_or_else(|| format!("no charstring {g}"))
    };
    let base_at = at + index.data_offset() - 1;
    let (start, end) = (base_at + offset(glyph)?, base_at + offset(glyph + 1)?);
    if end < start + CFF_RANDOM_PROGRAM.len() || end > cff.len() {
        return Err(format!(
            "the charstring of R is {} bytes, shorter than the {}-byte program",
            end.saturating_sub(start),
            CFF_RANDOM_PROGRAM.len()
        ));
    }
    let mut patched = cff.to_vec();
    patched[start..end].fill(ENDCHAR);
    patched[start..start + CFF_RANDOM_PROGRAM.len()].copy_from_slice(&CFF_RANDOM_PROGRAM);
    let name = rename(
        table(&all, NAME)?,
        "Sofel Random Sample",
        "SofelRandomSample-Regular",
    )?;
    rebuild(&all, &[(CFF, &patched), (NAME, &name)])
}

/// `[x, x.ss01, x.ss02]` for a–z and A–Z, by glyph name, sorted by `x`.
fn alternate_sets(base: &[u8]) -> Result<Vec<[u16; 3]>, String> {
    let font = Font::new(base, 0).map_err(|e| format!("font: {e:?}"))?;
    let mut buf = vec![0; font.glyph_names_len()];
    let names = font.glyph_names(&mut buf).ok_or("no glyph names")?;
    let mut sets = ('a'..='z')
        .chain('A'..='Z')
        .map(|c| {
            let g = font
                .nominal_glyph(u32::from(c))
                .ok_or_else(|| format!("no glyph for {c}"))?;
            let name = names
                .glyph_name(g)
                .ok_or_else(|| format!("no name for {c}"))?;
            let mut set = [to_u16(g.0 as usize, "glyph id")?; 3];
            for (slot, suffix) in set.iter_mut().skip(1).zip([".ss01", ".ss02"]) {
                let full = [name, suffix.as_bytes()].concat();
                let alt = names
                    .glyph_from_name(&full)
                    .ok_or_else(|| format!("missing glyph {}", String::from_utf8_lossy(&full)))?;
                *slot = to_u16(alt.0 as usize, "glyph id")?;
            }
            Ok(set)
        })
        .collect::<Result<Vec<_>, String>>()?;
    sets.sort_unstable();
    if sets.windows(2).any(|w| w[0][0] == w[1][0]) {
        return Err("two letters map to one glyph".into());
    }
    Ok(sets)
}

/// Links a 16-bit field at `position` of the current object to the byte
/// `target` of the original GSUB, packed as `original`: the bias moves
/// the offset from the start of that object to `target` inside it.
fn link_original(s: &mut Serializer, position: usize, original: ObjIdx, target: usize) {
    let bias = 0u32.wrapping_sub(u32::try_from(target).unwrap_or(u32::MAX));
    s.add_link(position, 2, false, original, Whence::Head, bias);
}

fn pack_lang_sys(s: &mut Serializer, lang_sys: &LangSys<'_>, rand: u16) -> Result<ObjIdx, String> {
    s.push();
    s.u16(0);
    s.u16(lang_sys.required_feature_index());
    s.u16(to_u16(lang_sys.num_features() + 1, "feature indices")?);
    for k in 0..lang_sys.num_features() {
        s.u16(lang_sys.feature_index(k).ok_or("truncated LangSys")?);
    }
    s.u16(rand);
    Ok(s.pop_pack(true))
}

/// The GSUB of [`rand_sample`]: new lists in front of the unchanged
/// original bytes (sofel ADR 0084, データ構造).
fn rand_gsub(gsub: &[u8], sets: &[[u16; 3]]) -> Result<Vec<u8>, String> {
    let read = |e| format!("GSUB: {e:?}");
    let layout = LayoutTable::new(gsub).map_err(read)?;
    if layout.version() != 0x0001_0000 {
        return Err("GSUB version is not 1.0".into());
    }
    let scripts = layout.script_list().map_err(read)?.ok_or("no ScriptList")?;
    let features = layout
        .feature_list()
        .map_err(read)?
        .ok_or("no FeatureList")?;
    let lookups = layout.lookup_list().map_err(read)?.ok_or("no LookupList")?;
    let (script_at, feature_at, lookup_at) = (
        layout.script_list_offset() as usize,
        layout.feature_list_offset() as usize,
        layout.lookup_list_offset() as usize,
    );
    let rand_feature = to_u16(features.len(), "feature count")?;
    let rand_lookup = to_u16(lookups.len(), "lookup count")?;

    let mut s = Serializer::new();
    s.push();
    s.extend(gsub);
    let original = s.pop_pack(false);

    // Lookup 30: AlternateSubstFormat1.
    let set_objects: Vec<ObjIdx> = sets
        .iter()
        .map(|set| {
            s.push();
            s.u16(3);
            for &g in set {
                s.u16(g);
            }
            s.pop_pack(true)
        })
        .collect();
    s.push();
    s.u16(1);
    s.u16(to_u16(sets.len(), "coverage")?);
    for set in sets {
        s.u16(set[0]);
    }
    let coverage = s.pop_pack(true);
    s.push();
    s.u16(1);
    let at = s.u16(0);
    s.link16(at, coverage);
    s.u16(to_u16(sets.len(), "alternate sets")?);
    for &set in &set_objects {
        let at = s.u16(0);
        s.link16(at, set);
    }
    let subtable = s.pop_pack(true);
    s.push();
    s.u16(3);
    s.u16(0);
    s.u16(1);
    let at = s.u16(0);
    s.link16(at, subtable);
    let lookup = s.pop_pack(true);

    s.push();
    s.u16(to_u16(lookups.len() + 1, "lookup count")?);
    for i in 0..lookups.len() {
        let off = lookups.lookup_offset(i).ok_or("truncated LookupList")? as usize;
        let at = s.u16(0);
        link_original(&mut s, at, original, lookup_at + off);
    }
    let at = s.u16(0);
    s.link16(at, lookup);
    let lookup_list = s.pop_pack(false);

    s.push();
    s.u16(0);
    s.u16(1);
    s.u16(rand_lookup);
    let rand = s.pop_pack(true);
    s.push();
    s.u16(to_u16(features.len() + 1, "feature count")?);
    for i in 0..features.len() {
        let tag = features.tag(i).ok_or("truncated FeatureList")?;
        let off = usize::from(features.feature_offset(i).ok_or("truncated FeatureList")?);
        s.extend(&tag.0);
        let at = s.u16(0);
        link_original(&mut s, at, original, feature_at + off);
    }
    s.extend(b"rand");
    let at = s.u16(0);
    s.link16(at, rand);
    let feature_list = s.pop_pack(false);

    // DFLT and latn get a new Script table whose default LangSys also
    // lists `rand`; their other LangSys stay in the original.
    let mut script_objects = Vec::new();
    let mut changed = 0;
    for i in 0..scripts.len() {
        let tag = scripts.tag(i).ok_or("truncated ScriptList")?;
        let off = usize::from(scripts.script_offset(i).ok_or("truncated ScriptList")?);
        if tag != Tag::new(b"DFLT") && tag != Tag::new(b"latn") {
            script_objects.push((tag, None, script_at + off));
            continue;
        }
        changed += 1;
        let script = scripts.script(i).map_err(read)?.ok_or("no Script")?;
        let default = script
            .default_lang_sys()
            .map_err(read)?
            .ok_or("no default LangSys")?;
        let lang_sys = pack_lang_sys(&mut s, &default, rand_feature)?;
        s.push();
        let at = s.u16(0);
        s.link16(at, lang_sys);
        s.u16(to_u16(script.num_lang_sys(), "LangSys count")?);
        for j in 0..script.num_lang_sys() {
            let tag = script.lang_sys_tag(j).ok_or("truncated Script")?;
            let off_j = usize::from(script.lang_sys_offset(j).ok_or("truncated Script")?);
            s.extend(&tag.0);
            let at = s.u16(0);
            link_original(&mut s, at, original, script_at + off + off_j);
        }
        script_objects.push((tag, Some(s.pop_pack(false)), 0));
    }
    if changed != 2 {
        return Err("GSUB lacks the DFLT or latn script".into());
    }
    s.push();
    s.u16(to_u16(scripts.len(), "script count")?);
    for (tag, object, target) in script_objects {
        s.extend(&tag.0);
        let at = s.u16(0);
        match object {
            Some(o) => s.link16(at, o),
            None => link_original(&mut s, at, original, target),
        }
    }
    let script_list = s.pop_pack(false);

    s.u16(1);
    s.u16(0);
    for list in [script_list, feature_list, lookup_list] {
        let at = s.u16(0);
        s.link16(at, list);
    }
    let packed = s.end().map_err(|e| format!("GSUB serialization: {e:?}"))?;
    packed.resolve().map_err(|e| {
        if e.has(Errors::OFFSET_OVERFLOW) {
            "a GSUB offset exceeds 65535".to_string()
        } else {
            format!("GSUB serialization: {e:?}")
        }
    })
}

/// Caveat with a `rand` feature (lookup 30, AlternateSet `[x, x.ss01,
/// x.ss02]` for a–z and A–Z) in the default LangSys of DFLT and latn,
/// renamed `Sofel Rand Sample` (sofel ADR 0084).
///
/// # Errors
///
/// When the SHA-256 of `base` is not `sha256`, an alternate glyph is
/// missing, or an offset of the new GSUB exceeds 65535.
pub fn rand_sample(base: &[u8], sha256: &str) -> Result<Vec<u8>, String> {
    check_sha256(base, sha256)?;
    let all = tables(base)?;
    let sets = alternate_sets(base)?;
    let gsub = rand_gsub(table(&all, GSUB)?, &sets)?;
    let name = rename(
        table(&all, NAME)?,
        "Sofel Rand Sample",
        "SofelRandSample-Regular",
    )?;
    rebuild(&all, &[(GSUB, &gsub), (NAME, &name)])
}

/// Writes `font` as `dir/name` and copies the license file `license.0`
/// next to it as `license.1`. On an error, nothing is written and an earlier font of
/// that name is removed.
pub fn write_derived(
    dir: &Path,
    name: &str,
    font: Result<Vec<u8>, String>,
    license: (&Path, &str),
) -> Result<(), String> {
    let path = dir.join(name);
    match fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", path.display()));
        }
        _ => {}
    }
    let font = font.map_err(|e| format!("{name}: {e}"))?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::copy(license.0, dir.join(license.1))
        .map_err(|e| format!("{}: {e}", license.0.display()))?;
    fs::write(&path, font).map_err(|e| format!("{}: {e}", path.display()))
}

/// Derives both fonts from the fetched originals into
/// `<repo>/target/derived-fonts`.
pub fn write_derived_fonts(repo: &Path) -> Result<(), String> {
    let fonts = repo.join("target/testdata/extra-fonts");
    let dir = repo.join(DERIVED_DIR);
    for (lock, id, license, license_name, file, derive) in [
        (
            "fonts.lock",
            "stix-math",
            "stix-license",
            "stix-license.txt",
            CFF_RANDOM_FILE,
            cff_random_sample as fn(&[u8], &str) -> Result<Vec<u8>, String>,
        ),
        (
            "gallery-fonts.lock",
            "caveat",
            "caveat-license",
            "OFL.txt",
            RAND_FILE,
            rand_sample,
        ),
    ] {
        let lock = repo.join(lock);
        let (sha, path) = lock_entry(&lock, id)?;
        let (_, license_path) = lock_entry(&lock, license)?;
        let base = fs::read(fonts.join(&path))
            .map_err(|e| format!("{}: {e}", fonts.join(&path).display()))?;
        write_derived(
            &dir,
            file,
            derive(&base, &sha),
            (&fonts.join(license_path), license_name),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_matches_fips_180_examples() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_hex(&[b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // 55, 56 and 64 bytes: the padding fills one or two blocks.
        for (len, digest) in [
            (
                55,
                "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
            ),
            (
                56,
                "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
            ),
            (
                64,
                "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
            ),
        ] {
            assert_eq!(sha256_hex(&vec![b'a'; len]), digest, "{len}");
        }
    }
}
