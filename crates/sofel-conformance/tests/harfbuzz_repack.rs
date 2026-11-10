//! sofel's repacker against HarfBuzz's (ADR 0022, 再配置).
//!
//! The test builds object graphs from a fixed seed: random graphs (16-,
//! 24- and 32-bit, signed and unsigned links measured from the head, the
//! tail or the start, with biases, and virtual links) and GSUB and GPOS
//! tables shaped like HarfBuzz's `test-repacker.cc` graphs with random
//! sizes (lookups to promote to extension lookups, and PairPos format 1
//! and 2, MarkBasePos and LigatureSubst subtables to split). Each graph is
//! given to `hb-dump --repack` twice: serialized with
//! `hb_serialize_context_t` and repacked as `_hb_subset_repack` does
//! (sofel: `Serializer` and `Packed::repack`), and through
//! `hb_subset_serialize_or_fail` (sofel: `serialize_objects`). The bytes
//! (their length and FNV-1a hash), or the failure, must be the same.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::process::Command;

use sofel_conformance::paths;
use sofel_read::Tag;
use sofel_write::{Link, Object, Serializer, Whence, serialize_objects};

const SEED: u64 = 0x5EED_0022_0004;
const RANDOM_GRAPHS: usize = 400;
const LAYOUT_GRAPHS: usize = 120;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// A graph: objects with 1-based link targets, the root last.
#[derive(Clone, Default)]
struct Graph {
    tag: [u8; 4],
    objs: Vec<Object>,
}

impl Graph {
    fn add(&mut self, bytes: Vec<u8>, links: &[(u32, u8, u32)], virt: &[u32]) -> u32 {
        self.objs.push(Object {
            bytes,
            real_links: links
                .iter()
                .filter(|l| l.2 != 0)
                .map(|&(position, width, objidx)| Link {
                    width,
                    position,
                    objidx,
                    ..Link::default()
                })
                .collect(),
            virtual_links: virt
                .iter()
                .filter(|&&v| v != 0)
                .map(|&objidx| Link {
                    objidx,
                    ..Link::default()
                })
                .collect(),
        });
        self.objs.len() as u32
    }
}

fn filler(rng: &mut Rng, len: usize) -> Vec<u8> {
    let b = rng.below(256) as u8;
    vec![b; len]
}

/// A random graph.
fn random_graph(rng: &mut Rng, with_link_kinds: bool) -> Graph {
    let tags: [&[u8; 4]; 4] = [b"\0\0\0\0", b"GSUB", b"GPOS", b"abcd"];
    let mut g = Graph {
        tag: *tags[rng.below(4) as usize],
        ..Graph::default()
    };
    let n = rng.range(2, 40) as usize;
    // Children per object: each object but the root gets one parent
    // among the later objects, plus extra links.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n - 1 {
        let p = rng.range(i as u64 + 1, n as u64 - 1) as usize;
        children[p].push(i);
    }
    for _ in 0..rng.below(n as u64 * 2) {
        let p = rng.range(1, n as u64 - 1) as usize;
        let c = rng.below(p as u64) as usize;
        children[p].push(c);
    }
    for (p, ch) in children.iter_mut().enumerate() {
        let k = ch.len();
        for i in (1..k).rev() {
            ch.swap(i, rng.below(i as u64 + 1) as usize);
        }
        let _ = p;
    }
    for (i, ch) in children.iter().enumerate() {
        let large = rng.chance(25);
        let head = if large {
            rng.range(4000, 40000)
        } else {
            rng.range(1, 24)
        } as usize;
        let mut bytes = filler(rng, head);
        let mut real = Vec::new();
        let mut virt = Vec::new();
        for &c in ch {
            if rng.chance(8) {
                virt.push(Link {
                    objidx: c as u32 + 1,
                    ..Link::default()
                });
                continue;
            }
            let width = match rng.below(10) {
                0..=6 => 2u8,
                7 => 3,
                _ => 4,
            };
            let mut l = Link {
                width,
                position: bytes.len() as u32,
                objidx: c as u32 + 1,
                ..Link::default()
            };
            if with_link_kinds {
                l.signed = width != 3 && rng.chance(10);
                l.whence = match rng.below(20) {
                    0 => Whence::Tail,
                    1 => Whence::Absolute,
                    _ => Whence::Head,
                };
                if rng.chance(5) {
                    l.bias = rng.below(3) as u32;
                }
            }
            real.push(l);
            bytes.extend(std::iter::repeat_n(0u8, usize::from(width)));
        }
        let tail = rng.below(8) as usize;
        bytes.extend(filler(rng, tail));
        let _ = i;
        g.objs.push(Object {
            bytes,
            real_links: real,
            virtual_links: virt,
        });
    }
    g
}

fn be16(v: u32) -> [u8; 2] {
    (v as u16).to_be_bytes()
}

fn coverage(g: &mut Graph, glyphs: &[u32]) -> u32 {
    // Format 1 or one range, as test-repacker.cc's add_coverage.
    let consecutive = glyphs.windows(2).all(|w| w[1] == w[0] + 1);
    let mut b = Vec::new();
    if consecutive && glyphs.len() > 2 {
        b.extend_from_slice(&[0, 2, 0, 1]);
        b.extend_from_slice(&be16(glyphs[0]));
        b.extend_from_slice(&be16(*glyphs.last().unwrap()));
        b.extend_from_slice(&[0, 0]);
    } else {
        b.extend_from_slice(&[0, 1]);
        b.extend_from_slice(&be16(glyphs.len() as u32));
        glyphs.iter().for_each(|&gl| b.extend_from_slice(&be16(gl)));
    }
    g.add(b, &[], &[])
}

fn lookup(g: &mut Graph, kind: u32, subtables: &[u32]) -> u32 {
    let mut head = vec![0, kind as u8, 0, 2];
    head.extend_from_slice(&be16(subtables.len() as u32));
    let mut bytes = head;
    let mut links = Vec::new();
    for &s in subtables {
        links.push((bytes.len() as u32, 2, s));
        bytes.extend_from_slice(&[0, 0]);
    }
    bytes.extend_from_slice(&[0, 0]);
    g.add(bytes, &links, &[])
}

fn extension(g: &mut Graph, child: u32, kind: u32) -> u32 {
    g.add(vec![0, 1, 0, kind as u8, 0, 0, 0, 0], &[(4, 4, child)], &[])
}

fn header(g: &mut Graph, rng: &mut Rng, lookups: &[u32]) {
    let v12 = rng.chance(20);
    let mut bytes = vec![0, lookups.len() as u8];
    let mut links = Vec::new();
    for &l in lookups {
        links.push((bytes.len() as u32, if v12 { 4 } else { 2 }, l));
        bytes.extend(std::iter::repeat_n(0u8, if v12 { 4 } else { 2 }));
    }
    let list = g.add(bytes, &links, &[]);
    if v12 {
        let mut h = vec![0u8; 26];
        h[1] = 1;
        h[3] = 2;
        g.add(h, &[(22, 4, list)], &[]);
    } else {
        g.add(vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0], &[(8, 2, list)], &[]);
    }
}

/// Lookups whose subtables need extension promotion.
fn extension_graph(rng: &mut Rng) -> Graph {
    let gsub = rng.chance(50);
    let mut g = Graph {
        tag: if gsub { *b"GSUB" } else { *b"GPOS" },
        ..Graph::default()
    };
    let ext = if gsub { 7 } else { 9 };
    let mut lookups = Vec::new();
    for _ in 0..rng.range(2, 8) {
        let kind = rng.range(1, 6) as u32;
        let is_ext = rng.chance(20);
        let mut subs = Vec::new();
        for _ in 0..rng.range(1, 3) {
            let size = rng.range(3000, 20000) as usize;
            let s = g.add(filler(rng, size), &[], &[]);
            subs.push(if is_ext {
                extension(&mut g, s, kind)
            } else {
                s
            });
        }
        if rng.chance(15) {
            subs.push(subs[0]);
        }
        lookups.push(lookup(&mut g, if is_ext { ext } else { kind }, &subs));
    }
    header(&mut g, rng, &lookups);
    g
}

fn pair_pos_1_graph(rng: &mut Rng) -> Graph {
    let mut g = Graph {
        tag: *b"GPOS",
        ..Graph::default()
    };
    let as_ext = rng.chance(50);
    let mut subs = Vec::new();
    for t in 0..rng.range(1, 2) as u32 {
        let n = rng.range(2, 8) as u32;
        let shared = rng.chance(20);
        let mut sets = Vec::new();
        for j in 0..n {
            if shared && j >= 2 {
                sets.push(sets[(j % 2) as usize]);
                continue;
            }
            let size = rng.range(8000, 30000) as usize;
            sets.push(g.add(filler(rng, size), &[], &[]));
        }
        let glyphs: Vec<u32> = (0..n).map(|k| t * 100 + k).collect();
        let cov = coverage(&mut g, &glyphs);
        let mut head = vec![0, 1];
        let mut links = vec![(2u32, 2u8, cov)];
        head.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        head.extend_from_slice(&be16(n));
        for &s in &sets {
            links.push((head.len() as u32, 2, s));
            head.extend_from_slice(&[0, 0]);
        }
        let pp = g.add(head, &links, &[]);
        subs.push(if as_ext { extension(&mut g, pp, 2) } else { pp });
    }
    let lk = lookup(&mut g, if as_ext { 9 } else { 2 }, &subs);
    header(&mut g, rng, &[lk]);
    g
}

fn class_def(g: &mut Graph, start: u32, count: u32) -> u32 {
    let mut b = vec![0, 1];
    b.extend_from_slice(&be16(start));
    b.extend_from_slice(&be16(count));
    for i in 1..=count {
        b.extend_from_slice(&be16(i));
    }
    g.add(b, &[], &[])
}

fn pair_pos_2_graph(rng: &mut Rng) -> Graph {
    let mut g = Graph {
        tag: *b"GPOS",
        ..Graph::default()
    };
    let as_ext = rng.chance(50);
    let class1 = rng.range(2, 8) as u32;
    let class2 = rng.range(200, 2500) as u32;
    let devices = rng.chance(30);
    let start = 5u32;
    let cd2 = class_def(&mut g, 11, class2 - 1);
    let cd1 = class_def(&mut g, start + 1, class1 - 1);
    let glyphs: Vec<u32> = (start..start + class1).collect();
    let cov = coverage(&mut g, &glyphs);
    let mut dev = Vec::new();
    if devices {
        for j in 0..class1 * class2 {
            dev.push(g.add(vec![(j >> 8) as u8, j as u8], &[], &[]));
        }
    }
    let f1 = [0x01u32, 0x05, 0x0B][rng.below(3) as usize];
    let f2 = if devices {
        0x24
    } else {
        [0x04u32, 0x00, 0x03][rng.below(3) as usize]
    };
    let mut b = vec![0, 2, 0, 0];
    b.extend_from_slice(&be16(f1));
    b.extend_from_slice(&be16(f2));
    b.extend_from_slice(&[0, 0, 0, 0]);
    b.extend_from_slice(&be16(class1));
    b.extend_from_slice(&be16(class2));
    let mut links = vec![(2u32, 2u8, cov), (8, 2, cd1), (10, 2, cd2)];
    let n1 = (f1 & 0xFF).count_ones();
    let n2 = (f2 & 0xFF).count_ones();
    let mut k = 0usize;
    for i in 0..class1 {
        for _ in 0..class2 {
            for _ in 0..n1 {
                b.extend_from_slice(&be16(i));
            }
            for v in 0..n2 {
                if devices && v == n2 - 1 {
                    links.push((b.len() as u32, 2, dev[k]));
                    k += 1;
                }
                b.extend_from_slice(&be16(i));
            }
        }
    }
    let mut subs = Vec::new();
    if rng.chance(50) {
        let size = rng.range(20000, 60000) as usize;
        subs.push(g.add(filler(rng, size), &[], &[]));
    }
    subs.push(g.add(b, &links, &[]));
    let subs: Vec<u32> = subs
        .into_iter()
        .map(|s| if as_ext { extension(&mut g, s, 2) } else { s })
        .collect();
    let lk = lookup(&mut g, if as_ext { 9 } else { 2 }, &subs);
    header(&mut g, rng, &[lk]);
    g
}

fn mark_base_graph(rng: &mut Rng) -> Graph {
    let mut g = Graph {
        tag: *b"GPOS",
        ..Graph::default()
    };
    let class_count = rng.range(4, 12) as u32;
    let mark_count = class_count * rng.range(1, 4) as u32;
    let base_count = rng.range(60, 140) as u32;
    let anchor_size = rng.range(60, 120) as usize;
    let mut base_anchors = Vec::new();
    for i in 0..class_count * base_count {
        base_anchors.push(g.add(
            vec![(i >> 8) as u8, i as u8].repeat(anchor_size / 2),
            &[],
            &[],
        ));
    }
    let marks: Vec<u32> = (0..mark_count)
        .map(|i| g.add(vec![0, 1, (i >> 8) as u8, i as u8], &[], &[]))
        .collect();
    let mut b = be16(base_count).to_vec();
    let mut links = Vec::new();
    for base in 0..base_count {
        for k in 0..class_count {
            links.push((
                b.len() as u32,
                2u8,
                base_anchors[(base * class_count + k) as usize],
            ));
            b.extend_from_slice(&[0, 0]);
        }
    }
    let base_array = g.add(b, &links, &[]);
    let mut b = be16(mark_count).to_vec();
    let mut links = Vec::new();
    for m in 0..mark_count {
        b.extend_from_slice(&be16(m % class_count));
        links.push((b.len() as u32, 2u8, marks[m as usize]));
        b.extend_from_slice(&[0, 0]);
    }
    let mark_array = g.add(b, &links, &[]);
    let mark_cov = coverage(&mut g, &(0..mark_count).collect::<Vec<_>>());
    let base_cov = coverage(&mut g, &(1000..1000 + base_count).collect::<Vec<_>>());
    let mut b = vec![0, 1, 0, 0, 0, 0];
    b.extend_from_slice(&be16(class_count));
    b.extend_from_slice(&[0, 0, 0, 0]);
    let mbp = g.add(
        b,
        &[
            (2, 2, mark_cov),
            (4, 2, base_cov),
            (8, 2, mark_array),
            (10, 2, base_array),
        ],
        &[],
    );
    let ext = extension(&mut g, mbp, 4);
    let lk = lookup(&mut g, 9, &[ext]);
    header(&mut g, rng, &[lk]);
    g
}

fn liga_graph(rng: &mut Rng) -> Graph {
    let mut g = Graph {
        tag: *b"GSUB",
        ..Graph::default()
    };
    let mut subs = Vec::new();
    for l in 0..rng.range(1, 2) as u32 {
        let sets = rng.range(1, 6) as u32;
        let per_set = rng.range(1, 3) as u32;
        let glyphs: Vec<u32> = (0..sets).map(|k| l * 10 + k).collect();
        let cov = coverage(&mut g, &glyphs);
        let mut set_ids = Vec::new();
        for _ in 0..sets {
            let mut ligas = Vec::new();
            for _ in 0..per_set {
                let size = rng.range(8000, 40000) as usize;
                ligas.push(g.add(filler(rng, size), &[], &[cov]));
            }
            let nulls = rng.chance(15);
            let mut b = be16(per_set + if nulls { 2 } else { 0 }).to_vec();
            let mut links = Vec::new();
            for &x in &ligas {
                links.push((b.len() as u32, 2u8, x));
                b.extend_from_slice(&[0, 0]);
            }
            if nulls {
                b.extend_from_slice(&[0, 0, 0, 0]);
            }
            set_ids.push(g.add(b, &links, &[cov]));
        }
        let mut b = vec![0, 1, 0, 0];
        b.extend_from_slice(&be16(sets));
        let mut links = vec![(2u32, 2u8, cov)];
        for &s in &set_ids {
            links.push((b.len() as u32, 2, s));
            b.extend_from_slice(&[0, 0]);
        }
        let ls = g.add(b, &links, &[]);
        subs.push(extension(&mut g, ls, 4));
    }
    let lk = lookup(&mut g, 7, &subs);
    header(&mut g, rng, &[lk]);
    g
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn encode(graphs: &[(Graph, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, graphs.len() as u32);
    for (g, kind) in graphs {
        put32(&mut out, u32::from_be_bytes(g.tag));
        put32(&mut out, *kind);
        put32(&mut out, g.objs.len() as u32);
        for o in &g.objs {
            put32(&mut out, o.bytes.len() as u32);
            out.extend_from_slice(&o.bytes);
            put32(&mut out, o.real_links.len() as u32);
            for l in &o.real_links {
                let whence = match l.whence {
                    Whence::Head => 0,
                    Whence::Tail => 1,
                    Whence::Absolute => 2,
                };
                out.extend_from_slice(&[l.width, u8::from(l.signed), whence, 0]);
                put32(&mut out, l.bias);
                put32(&mut out, l.position);
                put32(&mut out, l.objidx);
            }
            put32(&mut out, o.virtual_links.len() as u32);
            for l in &o.virtual_links {
                put32(&mut out, l.objidx);
            }
        }
    }
    out
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| {
        (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3)
    })
}

fn ours(g: &Graph, kind: u32) -> Option<Vec<u8>> {
    let tag = Tag(g.tag);
    if kind == 1 {
        return serialize_objects(tag, &g.objs).ok();
    }
    let mut c = Serializer::new();
    for (i, o) in g.objs.iter().enumerate() {
        c.push();
        c.extend(&o.bytes);
        for l in &o.real_links {
            c.add_link(
                l.position as usize,
                l.width,
                l.signed,
                l.objidx,
                l.whence,
                l.bias,
            );
        }
        for l in &o.virtual_links {
            c.add_virtual_link(l.objidx);
        }
        if c.pop_pack(false) != i as u32 + 1 {
            return None;
        }
    }
    c.end().ok()?.repack(tag).ok()
}

fn describe(r: &Option<Vec<u8>>) -> String {
    match r {
        Some(b) => format!("ok {} {:016x}", b.len(), fnv(b)),
        None => "fail".into(),
    }
}

#[test]
fn repacking_agrees_with_harfbuzz() {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let mut rng = Rng(SEED);
    let mut graphs: Vec<(Graph, u32)> = Vec::new();
    for i in 0..RANDOM_GRAPHS {
        let kind = (i % 2) as u32;
        graphs.push((random_graph(&mut rng, kind == 0), kind));
    }
    for i in 0..LAYOUT_GRAPHS {
        let g = match i % 6 {
            0 => extension_graph(&mut rng),
            1 => pair_pos_1_graph(&mut rng),
            2 => pair_pos_2_graph(&mut rng),
            3 => mark_base_graph(&mut rng),
            4 => liga_graph(&mut rng),
            _ => extension_graph(&mut rng),
        };
        graphs.push((g.clone(), 0));
        graphs.push((g, 1));
    }
    let dir = std::env::temp_dir().join(format!("sofel-repack-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("graphs.bin");
    std::fs::write(&file, encode(&graphs)).unwrap();
    let out = Command::new(&exe)
        .arg("--repack")
        .arg(&file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "hb-dump --repack failed");
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), graphs.len());

    let mut mismatches = Vec::new();
    let mut repacked = 0;
    let mut failed = 0;
    for (i, ((g, kind), line)) in graphs.iter().zip(&lines).enumerate() {
        let hb = line
            .strip_prefix(&format!("graph {i} "))
            .unwrap_or_else(|| panic!("bad line {line}"));
        let r = ours(g, *kind);
        if describe(&r) != hb {
            mismatches.push(format!(
                "graph {i} (kind {kind}, tag {:?}, {} objects): sofel {} harfbuzz {hb}",
                std::str::from_utf8(&g.tag).unwrap_or("?"),
                g.objs.len(),
                describe(&r)
            ));
        }
        match r {
            Some(b) if b.len() > 0xFFFF => repacked += 1,
            None => failed += 1,
            _ => {}
        }
    }
    eprintln!(
        "{} graphs: {} larger than 64 KiB, {} failing; {} mismatches",
        graphs.len(),
        repacked,
        failed,
        mismatches.len()
    );
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
