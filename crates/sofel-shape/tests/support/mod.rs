//! Small fonts with hand-assembled `GSUB`, `GPOS` and `GDEF` tables for
//! the shaping tests (ADR 0020, テスト設計).

#![allow(
    dead_code,
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../../sofel-read/tests/support/build.rs"]
pub mod build;

use build::{Buf, HeadSpec, cmap, cmap12_13, head, hhea, hmtx, maxp05, sfnt};

/// Coverage format 1.
pub fn coverage(glyphs: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(1).u16(glyphs.len() as u16);
    for g in glyphs {
        b = b.u16(*g);
    }
    b.done()
}

/// Appends `tail` to `head` and returns the offset where it starts.
fn append(head: &mut Vec<u8>, tail: &[u8]) -> u16 {
    let at = head.len() as u16;
    head.extend_from_slice(tail);
    at
}

/// A language system: its tag (`None` for the default one), the
/// required feature and the feature indices.
pub struct LangSpec {
    pub tag: Option<[u8; 4]>,
    pub required: Option<u16>,
    pub features: Vec<u16>,
}

pub fn lang(tag: Option<&[u8; 4]>, features: &[u16]) -> LangSpec {
    LangSpec {
        tag: tag.copied(),
        required: None,
        features: features.to_vec(),
    }
}

/// A lookup: type, flag, mark filtering set and subtables.
pub struct LookupSpec {
    pub ty: u16,
    pub flag: u16,
    pub mark_set: Option<u16>,
    pub subtables: Vec<Vec<u8>>,
}

pub fn lookup(ty: u16, flag: u16, subtables: Vec<Vec<u8>>) -> LookupSpec {
    LookupSpec {
        ty,
        flag,
        mark_set: None,
        subtables,
    }
}

fn lang_sys(l: &LangSpec) -> Vec<u8> {
    let mut b = Buf::new()
        .u16(0)
        .u16(l.required.unwrap_or(0xFFFF))
        .u16(l.features.len() as u16);
    for f in &l.features {
        b = b.u16(*f);
    }
    b.done()
}

/// A `GSUB` or `GPOS` table, version 1.0.
pub fn layout(
    scripts: &[(&[u8; 4], Vec<LangSpec>)],
    features: &[(&[u8; 4], Vec<u16>)],
    lookups: &[LookupSpec],
) -> Vec<u8> {
    // ScriptList.
    let mut script_tables: Vec<Vec<u8>> = Vec::new();
    for (_, langs) in scripts {
        let default = langs.iter().find(|l| l.tag.is_none());
        let named: Vec<&LangSpec> = langs.iter().filter(|l| l.tag.is_some()).collect();
        let header = 4 + 6 * named.len();
        let mut body = Vec::new();
        let default_off =
            default.map(|d| (header + append(&mut body, &lang_sys(d)) as usize) as u16);
        let mut b = Buf::new()
            .u16(default_off.unwrap_or(0))
            .u16(named.len() as u16);
        for l in &named {
            let off = header + append(&mut body, &lang_sys(l)) as usize;
            b = b.tag(&l.tag.unwrap()).u16(off as u16);
        }
        let mut t = b.done();
        t.extend_from_slice(&body);
        script_tables.push(t);
    }
    let mut sl = Buf::new().u16(scripts.len() as u16);
    let mut body = Vec::new();
    let header = 2 + 6 * scripts.len();
    for ((tag, _), t) in scripts.iter().zip(&script_tables) {
        let off = header + append(&mut body, t) as usize;
        sl = sl.tag(tag).u16(off as u16);
    }
    let mut script_list = sl.done();
    script_list.extend_from_slice(&body);

    // FeatureList.
    let mut fl = Buf::new().u16(features.len() as u16);
    let mut body = Vec::new();
    let header = 2 + 6 * features.len();
    for (tag, lookups) in features {
        let mut f = Buf::new().u16(0).u16(lookups.len() as u16);
        for l in lookups {
            f = f.u16(*l);
        }
        let off = header + append(&mut body, &f.done()) as usize;
        fl = fl.tag(tag).u16(off as u16);
    }
    let mut feature_list = fl.done();
    feature_list.extend_from_slice(&body);

    // LookupList.
    let mut ll = Buf::new().u16(lookups.len() as u16);
    let mut body = Vec::new();
    let header = 2 + 2 * lookups.len();
    for l in lookups {
        let flag = l.flag | if l.mark_set.is_some() { 0x10 } else { 0 };
        let lheader = 6 + 2 * l.subtables.len() + if l.mark_set.is_some() { 2 } else { 0 };
        let mut lb = Buf::new().u16(l.ty).u16(flag).u16(l.subtables.len() as u16);
        let mut sbody = Vec::new();
        for s in &l.subtables {
            let off = lheader + append(&mut sbody, s) as usize;
            lb = lb.u16(off as u16);
        }
        if let Some(m) = l.mark_set {
            lb = lb.u16(m);
        }
        let mut lt = lb.done();
        lt.extend_from_slice(&sbody);
        let off = header + append(&mut body, &lt) as usize;
        ll = ll.u16(off as u16);
    }
    let mut lookup_list = ll.done();
    lookup_list.extend_from_slice(&body);

    let mut out = Vec::new();
    let s_off = 10;
    let f_off = s_off + script_list.len();
    let l_off = f_off + feature_list.len();
    out.extend_from_slice(
        &Buf::new()
            .u32(0x0001_0000)
            .u16(s_off as u16)
            .u16(f_off as u16)
            .u16(l_off as u16)
            .done(),
    );
    out.extend_from_slice(&script_list);
    out.extend_from_slice(&feature_list);
    out.extend_from_slice(&lookup_list);
    out
}

/// One script `DFLT` with a default language system of all features in
/// order, and one lookup per feature.
pub fn simple_layout(features: &[&[u8; 4]], lookups: Vec<LookupSpec>) -> Vec<u8> {
    let idx: Vec<u16> = (0..features.len() as u16).collect();
    let feats: Vec<(&[u8; 4], Vec<u16>)> = features
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, vec![i as u16]))
        .collect();
    layout(&[(b"DFLT", vec![lang(None, &idx)])], &feats, &lookups)
}

/// Single substitution format 2.
pub fn single_subst(pairs: &[(u16, u16)]) -> Vec<u8> {
    let mut pairs = pairs.to_vec();
    pairs.sort();
    let cov: Vec<u16> = pairs.iter().map(|p| p.0).collect();
    let header = 6 + 2 * pairs.len();
    let mut b = Buf::new().u16(2).u16(header as u16).u16(pairs.len() as u16);
    for p in &pairs {
        b = b.u16(p.1);
    }
    b.bytes(&coverage(&cov)).done()
}

/// Multiple substitution format 1 of one glyph.
pub fn multiple_subst(glyph: u16, seq: &[u16]) -> Vec<u8> {
    let seq_t = {
        let mut b = Buf::new().u16(seq.len() as u16);
        for g in seq {
            b = b.u16(*g);
        }
        b.done()
    };
    let cov = coverage(&[glyph]);
    Buf::new()
        .u16(1)
        .u16(8)
        .u16(1)
        .u16(8 + cov.len() as u16)
        .bytes(&cov)
        .bytes(&seq_t)
        .done()
}

/// Ligature substitution format 1 of ligatures starting with `first`.
pub fn ligature_subst(first: u16, ligs: &[(&[u16], u16)]) -> Vec<u8> {
    let mut set_body = Vec::new();
    let set_header = 2 + 2 * ligs.len();
    let mut set = Buf::new().u16(ligs.len() as u16);
    for (comps, g) in ligs {
        let mut l = Buf::new().u16(*g).u16(comps.len() as u16 + 1);
        for c in *comps {
            l = l.u16(*c);
        }
        let off = set_header + append(&mut set_body, &l.done()) as usize;
        set = set.u16(off as u16);
    }
    let mut set = set.done();
    set.extend_from_slice(&set_body);
    let cov = coverage(&[first]);
    Buf::new()
        .u16(1)
        .u16(8)
        .u16(1)
        .u16(8 + cov.len() as u16)
        .bytes(&cov)
        .bytes(&set)
        .done()
}

/// Alternate substitution format 1 of one glyph.
pub fn alternate_subst(glyph: u16, alts: &[u16]) -> Vec<u8> {
    let mut a = Buf::new().u16(alts.len() as u16);
    for g in alts {
        a = a.u16(*g);
    }
    let cov = coverage(&[glyph]);
    Buf::new()
        .u16(1)
        .u16(8)
        .u16(1)
        .u16(8 + cov.len() as u16)
        .bytes(&cov)
        .bytes(&a.done())
        .done()
}

/// Single positioning format 1 with an x placement and x advance.
pub fn single_pos(glyphs: &[u16], dx: i16, ax: i16) -> Vec<u8> {
    Buf::new()
        .u16(1)
        .u16(10)
        .u16(0x0005)
        .i16(dx)
        .i16(ax)
        .bytes(&coverage(glyphs))
        .done()
}

/// Single positioning format 1 with an x advance and its Device table.
pub fn single_pos_device(glyphs: &[u16], ax: i16, start: u16, deltas: &[i8]) -> Vec<u8> {
    // Format 3 deltas (8 bits each).
    let mut words = Vec::new();
    for pair in deltas.chunks(2) {
        let hi = pair[0] as u8 as u16;
        let lo = pair.get(1).map_or(0, |v| *v as u8 as u16);
        words.push((hi << 8) | lo);
    }
    let mut dev = Buf::new()
        .u16(start)
        .u16(start + deltas.len() as u16 - 1)
        .u16(3);
    for w in words {
        dev = dev.u16(w);
    }
    let dev = dev.done();
    let cov = coverage(glyphs);
    Buf::new()
        .u16(1)
        .u16(10)
        .u16(0x0044)
        .i16(ax)
        .u16(10 + cov.len() as u16)
        .bytes(&cov)
        .bytes(&dev)
        .done()
}

/// Pair positioning format 1: x advance of the first glyph.
pub fn pair_pos(first: u16, second: u16, ax: i16) -> Vec<u8> {
    let set = Buf::new().u16(1).u16(second).i16(ax).done();
    let cov = coverage(&[first]);
    Buf::new()
        .u16(1)
        .u16(12)
        .u16(0x0004)
        .u16(0)
        .u16(1)
        .u16(12 + cov.len() as u16)
        .bytes(&cov)
        .bytes(&set)
        .done()
}

fn anchor(x: i16, y: i16) -> Vec<u8> {
    Buf::new().u16(1).i16(x).i16(y).done()
}

/// Mark-to-base attachment format 1 with one mark class.
pub fn mark_base(marks: &[(u16, i16, i16)], bases: &[(u16, i16, i16)]) -> Vec<u8> {
    let mut marks = marks.to_vec();
    marks.sort();
    let mut bases = bases.to_vec();
    bases.sort();
    let mcov = coverage(&marks.iter().map(|m| m.0).collect::<Vec<_>>());
    let bcov = coverage(&bases.iter().map(|m| m.0).collect::<Vec<_>>());
    // MarkArray.
    let mh = 2 + 4 * marks.len();
    let mut ma = Buf::new().u16(marks.len() as u16);
    let mut mbody = Vec::new();
    for m in &marks {
        let off = mh + append(&mut mbody, &anchor(m.1, m.2)) as usize;
        ma = ma.u16(0).u16(off as u16);
    }
    let mut ma = ma.done();
    ma.extend_from_slice(&mbody);
    // BaseArray.
    let bh = 2 + 2 * bases.len();
    let mut ba = Buf::new().u16(bases.len() as u16);
    let mut bbody = Vec::new();
    for b in &bases {
        let off = bh + append(&mut bbody, &anchor(b.1, b.2)) as usize;
        ba = ba.u16(off as u16);
    }
    let mut ba = ba.done();
    ba.extend_from_slice(&bbody);
    let h = 12;
    let mc = h;
    let bc = mc + mcov.len();
    let mo = bc + bcov.len();
    let bo = mo + ma.len();
    Buf::new()
        .u16(1)
        .u16(mc as u16)
        .u16(bc as u16)
        .u16(1)
        .u16(mo as u16)
        .u16(bo as u16)
        .bytes(&mcov)
        .bytes(&bcov)
        .bytes(&ma)
        .bytes(&ba)
        .done()
}

/// A GDEF version 1.0 with a glyph class definition (format 2 ranges of
/// single glyphs).
pub fn gdef(classes: &[(u16, u16)]) -> Vec<u8> {
    let mut classes = classes.to_vec();
    classes.sort();
    let mut cd = Buf::new().u16(2).u16(classes.len() as u16);
    for (g, c) in &classes {
        cd = cd.u16(*g).u16(*g).u16(*c);
    }
    Buf::new()
        .u32(0x0001_0000)
        .u16(12)
        .u16(0)
        .u16(0)
        .u16(0)
        .bytes(&cd.done())
        .done()
}

/// A font with `num_glyphs` glyphs, `cmap` mappings, advances (the last
/// one repeats) and extra tables.
pub fn font(
    num_glyphs: u16,
    map: &[(u32, u16)],
    advances: &[u16],
    extra: Vec<(&'static [u8; 4], Vec<u8>)>,
) -> Vec<u8> {
    let groups: Vec<(u32, u32, u32)> = map.iter().map(|&(c, g)| (c, c, u32::from(g))).collect();
    let mut sorted = groups.clone();
    sorted.sort();
    let long: Vec<(u16, i16)> = advances.iter().map(|&a| (a, 0)).collect();
    let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap(&[(3, 10, cmap12_13(12, &sorted))])),
        (b"head", head(HeadSpec::default())),
        (b"hhea", hhea(advances.len() as u16)),
        (b"hmtx", hmtx(&long, &[])),
        (b"maxp", maxp05(num_glyphs)),
    ];
    tables.extend(extra);
    tables.sort_by_key(|t| *t.0);
    sfnt(&tables)
}
