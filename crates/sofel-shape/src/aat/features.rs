//! Cluster ranges and chain flags from HarfBuzz 15.1.0 hb-aat-map.cc.
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use sofel_font::read::feat::Feat;

use crate::{Feature, Language};
use sofel_font::read::ltag::Ltag;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Event {
    cluster: u32,
    start: bool,
    kind: u16,
    group: u32,
    order: usize,
    setting: u32,
}

#[derive(Default)]
pub(crate) struct Selections {
    events: Vec<Event>,
}

impl Selections {
    pub(crate) fn new(features: &[Feature], feat: Option<&[u8]>) -> Self {
        if features.is_empty() {
            return Self::default();
        }
        let mut events = Vec::new();
        let Some(names) = feat.and_then(Feat::new) else {
            return Self { events };
        };
        for (order, f) in features.iter().enumerate().filter(|(_, f)| f.start < f.end) {
            let tag = &f.tag.0;
            let (kind, setting, exclusive) = if tag == b"aalt" {
                if names.exclusive(17).is_none() {
                    continue;
                }
                (17, f.value, true)
            } else {
                let Ok(i) = MAPPINGS.binary_search_by_key(tag, |row| row.0) else {
                    continue;
                };
                let Some(&(_, kind, on, off)) = MAPPINGS.get(i) else {
                    continue;
                };
                let Some(is_exclusive) = names.exclusive(kind).or_else(|| {
                    (kind == 37 && on == 1)
                        .then(|| names.exclusive(3))
                        .flatten()
                }) else {
                    continue;
                };
                (
                    kind,
                    u32::from(if f.value == 0 { off } else { on }),
                    is_exclusive,
                )
            };
            let group = if exclusive { 0 } else { setting & !1 };
            let event = Event {
                cluster: f.start,
                start: true,
                kind,
                group,
                order,
                setting,
            };
            events.push(event);
            events.push(Event {
                cluster: f.end,
                start: false,
                ..event
            });
        }
        events.sort_unstable();
        Self { events }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Ranges(Vec<(u32, u32)>);

impl Ranges {
    pub(crate) fn any(&self, mask: u32) -> bool {
        self.0.iter().any(|(_, f)| f & mask != 0)
    }
    pub(crate) fn contains(&self, cluster: u32, mask: u32) -> bool {
        let end = self.0.partition_point(|(first, _)| *first <= cluster);
        end.checked_sub(1)
            .and_then(|i| self.0.get(i))
            .is_some_and(|(_, f)| f & mask != 0)
    }
}

pub(super) fn compile(
    default: u32,
    records: &[[u8; 12]],
    tags: Option<Ltag<'_>>,
    language: Option<&Language>,
    selections: &Selections,
    budget: &mut usize,
) -> Ranges {
    if records.is_empty() {
        return Ranges(alloc::vec![(0, default)]);
    }
    let rules: Vec<_> = records
        .iter()
        .map(|r| {
            let [a, b, c, d, e, f, g, h, i, j, k, l] = *r;
            let mut kind = u16::from_be_bytes([a, b]);
            let mut setting = u16::from_be_bytes([c, d]);
            if (kind, setting) == (3, 3) {
                kind = 37;
                setting = 1;
            }
            let lang = kind == 39
                && setting
                    .checked_sub(1)
                    .and_then(|i| tags?.tag(i))
                    .and_then(sofel_font::Language::new)
                    .is_some_and(|tag| {
                        let Some(language) = language else {
                            return false;
                        };
                        let mut bytes = language.as_str().bytes();
                        !tag.is_empty()
                            && tag.bytes().all(|b| bytes.next() == Some(b))
                            && matches!(bytes.next(), None | Some(b'-'))
                    });
            (
                kind,
                u32::from(setting),
                lang,
                u32::from_be_bytes([e, f, g, h]),
                u32::from_be_bytes([i, j, k, l]),
            )
        })
        .collect();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut active = BTreeMap::<(u16, u32), BTreeSet<(usize, u32)>>::new();
    let mut at = 0;
    let mut events = selections.events.iter().peekable();
    loop {
        while let Some(e) = events.next_if(|e| e.cluster == at) {
            let key = (e.kind, e.group);
            if e.start {
                active.entry(key).or_default().insert((e.order, e.setting));
            } else if let Some(group) = active.get_mut(&key) {
                group.remove(&(e.order, e.setting));
                if group.is_empty() {
                    active.remove(&key);
                }
            }
        }
        let cost = rules.len().saturating_add(active.len()).saturating_add(1);
        if !ranges.is_empty() && *budget < cost {
            break;
        }
        *budget = budget.saturating_sub(cost);
        let selected: BTreeSet<_> = active
            .iter()
            .filter_map(|(&(kind, _), group)| Some((kind, group.first()?.1)))
            .collect();
        let mut flags = default;
        for &(kind, setting, lang, on, off) in &rules {
            if lang || selected.contains(&(kind, setting)) {
                flags = (flags & off) | on;
            }
        }
        if ranges.last().is_none_or(|r| r.1 != flags) {
            ranges.push((at, flags));
        }
        let Some(next) = events.peek() else { break };
        if next.cluster == u32::MAX {
            break;
        }
        at = next.cluster;
    }
    Ranges(ranges)
}

// OpenType tags and AAT type/on/off selectors, from hb-aat-layout.cc.
const MAPPINGS: &[([u8; 4], u16, u16, u16)] = &[
    (*b"afrc", 11, 1, 0),
    (*b"c2pc", 38, 2, 0),
    (*b"c2sc", 38, 1, 0),
    (*b"calt", 36, 0, 1),
    (*b"case", 33, 0, 1),
    (*b"clig", 1, 18, 19),
    (*b"cpsp", 33, 2, 3),
    (*b"cswh", 36, 4, 5),
    (*b"dlig", 1, 4, 5),
    (*b"expt", 20, 10, 16),
    (*b"frac", 11, 2, 0),
    (*b"fwid", 22, 1, 7),
    (*b"halt", 22, 6, 7),
    (*b"hist", 40, 0, 1),
    (*b"hkna", 34, 0, 1),
    (*b"hlig", 1, 20, 21),
    (*b"hngl", 23, 1, 0),
    (*b"hojo", 20, 12, 16),
    (*b"hwid", 22, 2, 7),
    (*b"ital", 32, 2, 3),
    (*b"jp04", 20, 11, 16),
    (*b"jp78", 20, 2, 16),
    (*b"jp83", 20, 3, 16),
    (*b"jp90", 20, 4, 16),
    (*b"liga", 1, 2, 3),
    (*b"lnum", 21, 1, 2),
    (*b"mgrk", 15, 10, 11),
    (*b"nlck", 20, 13, 16),
    (*b"onum", 21, 0, 2),
    (*b"ordn", 10, 3, 0),
    (*b"palt", 22, 5, 7),
    (*b"pcap", 37, 2, 0),
    (*b"pkna", 22, 0, 7),
    (*b"pnum", 6, 1, 4),
    (*b"pwid", 22, 0, 7),
    (*b"qwid", 22, 4, 7),
    (*b"rlig", 1, 0, 1),
    (*b"ruby", 28, 2, 3),
    (*b"sinf", 10, 4, 0),
    (*b"smcp", 37, 1, 0),
    (*b"smpl", 20, 1, 16),
    (*b"ss01", 35, 2, 3),
    (*b"ss02", 35, 4, 5),
    (*b"ss03", 35, 6, 7),
    (*b"ss04", 35, 8, 9),
    (*b"ss05", 35, 10, 11),
    (*b"ss06", 35, 12, 13),
    (*b"ss07", 35, 14, 15),
    (*b"ss08", 35, 16, 17),
    (*b"ss09", 35, 18, 19),
    (*b"ss10", 35, 20, 21),
    (*b"ss11", 35, 22, 23),
    (*b"ss12", 35, 24, 25),
    (*b"ss13", 35, 26, 27),
    (*b"ss14", 35, 28, 29),
    (*b"ss15", 35, 30, 31),
    (*b"ss16", 35, 32, 33),
    (*b"ss17", 35, 34, 35),
    (*b"ss18", 35, 36, 37),
    (*b"ss19", 35, 38, 39),
    (*b"ss20", 35, 40, 41),
    (*b"subs", 10, 2, 0),
    (*b"sups", 10, 1, 0),
    (*b"swsh", 36, 2, 3),
    (*b"titl", 19, 4, 0),
    (*b"tnam", 20, 14, 16),
    (*b"tnum", 6, 0, 4),
    (*b"trad", 20, 0, 16),
    (*b"twid", 22, 3, 7),
    (*b"unic", 3, 14, 15),
    (*b"valt", 22, 5, 7),
    (*b"vert", 4, 0, 1),
    (*b"vhal", 22, 6, 7),
    (*b"vkna", 34, 2, 3),
    (*b"vpal", 22, 5, 7),
    (*b"vrt2", 4, 0, 1),
    (*b"vrtr", 4, 2, 3),
    (*b"zero", 14, 4, 5),
];
