//! The BASE table and Device headers (ADR 0017), on tables built with
//! the ADR 0007 builders.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;

use sofel_read::base::{self, Base, BaseCoord};
use sofel_read::device::{self, Device};
use sofel_read::{DetailValue, Diagnostic, GlyphId, Kind, ReadError, Severity, Tag};
use support::build::*;

fn collect(data: &[u8]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    base::validate(data, &mut |d| out.push(d));
    out
}

fn kinds(data: &[u8]) -> Vec<(&'static str, Severity, u32)> {
    collect(data)
        .iter()
        .map(|d| (d.kind.id(), d.severity, d.offset))
        .collect()
}

fn t(s: &[u8; 4]) -> Tag {
    Tag::new(s)
}

/// Horizontal: romn, ideo, idtp for latn and DFLT; vertical: DFLT only.
fn two_axes() -> Vec<u8> {
    let h = AxisSpec {
        tags: Some(vec![*b"ideo", *b"idtp", *b"romn"]),
        scripts: Some(vec![
            (
                *b"DFLT",
                Some(ScriptSpec {
                    values: Some((
                        2,
                        vec![
                            Some(Coord::F1(-120)),
                            Some(Coord::F2(880, 3, 1)),
                            Some(Coord::F1(0)),
                        ],
                    )),
                    ..Default::default()
                }),
            ),
            (
                *b"latn",
                Some(ScriptSpec {
                    values: Some((
                        2,
                        vec![
                            Some(Coord::F3(-100, Some(device(9, 12, 2, &[0x1234])))),
                            Some(Coord::F4(900, 70_000, 2)),
                            None,
                        ],
                    )),
                    default_min_max: Some(MinMaxSpec {
                        min: Some(Coord::F1(-300)),
                        max: Some(Coord::F1(1100)),
                        features: vec![(*b"kern", Some(Coord::F1(-1)), None)],
                    }),
                    lang_sys: vec![
                        (
                            *b"ENG ",
                            Some(MinMaxSpec {
                                min: None,
                                max: Some(Coord::F1(1200)),
                                features: vec![],
                            }),
                        ),
                        (*b"TRK ", None),
                    ],
                }),
            ),
        ]),
    };
    let v = simple_axis(b"DFLT", &[(*b"ideo", 0), (*b"idtp", 1000)]);
    base(Some(&h), Some(&v))
}

#[test]
fn base_reads_both_axes() {
    let data = two_axes();
    let b = Base::new(&data).unwrap();
    assert_eq!(b.version(), base::VERSION_1_0);
    assert_eq!(b.item_var_store_offset(), None);

    let h = b.horiz_axis().unwrap().unwrap();
    let tags = h.base_tag_list().unwrap().unwrap();
    assert_eq!(tags.len(), 3);
    assert_eq!(tags.tag(1), Some(t(b"idtp")));
    assert_eq!(tags.tag(3), None);
    assert_eq!(tags.find(t(b"romn")), Some(2));
    assert_eq!(tags.find(t(b"math")), None);

    let scripts = h.base_script_list().unwrap().unwrap();
    assert_eq!(scripts.len(), 2);
    assert_eq!(scripts.find(t(b"latn")), Some(1));
    assert_eq!(scripts.find(t(b"hani")), None);
    assert!(matches!(scripts.script(2), Err(ReadError::OutOfBounds)));

    let dflt = scripts.script(0).unwrap().unwrap();
    let values = dflt.base_values().unwrap().unwrap();
    assert_eq!(values.default_baseline_index(), 2);
    assert_eq!(values.len(), 3);
    let c = values.coord(0).unwrap().unwrap();
    assert_eq!((c.format(), c.coordinate()), (1, -120));
    assert_eq!(c.reference_glyph(), None);
    let c = values.coord(1).unwrap().unwrap();
    assert_eq!((c.format(), c.coordinate()), (2, 880));
    assert_eq!(c.reference_glyph(), Some(GlyphId(3)));
    assert_eq!(c.coord_point(), Some(1));
    // Past the offsets: a null offset, as in HarfBuzz.
    assert!(values.coord(3).unwrap().is_none());
    assert!(dflt.default_min_max().unwrap().is_none());
    assert_eq!(dflt.num_lang_sys(), 0);

    let latn = scripts.script(1).unwrap().unwrap();
    let values = latn.base_values().unwrap().unwrap();
    let c = values.coord(0).unwrap().unwrap();
    assert_eq!((c.format(), c.coordinate()), (3, -100));
    let d = c.device().unwrap().unwrap();
    assert_eq!((d.start_size(), d.end_size(), d.delta_format()), (9, 12, 2));
    assert_eq!(d.variation_index(), None);
    let c = values.coord(1).unwrap().unwrap();
    assert_eq!((c.format(), c.coordinate()), (4, 900));
    assert_eq!(c.reference_glyph(), Some(GlyphId(70_000)));
    assert_eq!(c.coord_point(), Some(2));
    assert!(c.device().unwrap().is_none());
    assert!(values.coord(2).unwrap().is_none());

    let mm = latn.default_min_max().unwrap().unwrap();
    assert_eq!(mm.min_coord().unwrap().unwrap().coordinate(), -300);
    assert_eq!(mm.max_coord().unwrap().unwrap().coordinate(), 1100);
    assert_eq!(mm.num_features(), 1);
    assert_eq!(mm.find_feature(t(b"kern")), Some(0));
    assert_eq!(mm.feature_min_coord(0).unwrap().unwrap().coordinate(), -1);
    assert!(mm.feature_max_coord(0).unwrap().is_none());
    assert_eq!(latn.num_lang_sys(), 2);
    assert_eq!(latn.find_lang_sys(t(b"TRK ")), Some(1));
    assert_eq!(latn.find_lang_sys(t(b"dflt")), None);
    let eng = latn.lang_sys_min_max(0).unwrap().unwrap();
    assert!(eng.min_coord().unwrap().is_none());
    assert_eq!(eng.max_coord().unwrap().unwrap().coordinate(), 1200);
    assert!(latn.lang_sys_min_max(1).unwrap().is_none());

    let v = b.vert_axis().unwrap().unwrap();
    assert_eq!(v.base_tag_list().unwrap().unwrap().len(), 2);
    assert!(b.axis(true).unwrap().is_some());

    assert!(collect(&data).is_empty(), "{:?}", collect(&data));
}

#[test]
fn base_null_axes_and_lists() {
    let data = base(None, Some(&AxisSpec::default()));
    let b = Base::new(&data).unwrap();
    assert!(b.horiz_axis().unwrap().is_none());
    let v = b.vert_axis().unwrap().unwrap();
    assert!(v.base_tag_list().unwrap().is_none());
    assert!(v.base_script_list().unwrap().is_none());
    assert!(collect(&data).is_empty());
}

#[test]
fn base_version_1_1() {
    let store = [0u8, 1, 0, 0, 0, 0, 0, 0];
    let data = base11(
        Some(&simple_axis(b"DFLT", &[(*b"romn", 0)])),
        None,
        Some(&store),
    );
    let b = Base::new(&data).unwrap();
    assert_eq!(b.version(), base::VERSION_1_1);
    assert_eq!(b.item_var_store_offset(), Some((data.len() - 8) as u32));
    assert!(collect(&data).is_empty());

    // A version 1.1 header needs 12 bytes.
    let short = Buf::new().u32(0x0001_0001).u16(0).u16(0).done();
    assert_eq!(
        Base::new(&short).unwrap_err(),
        ReadError::Truncated {
            needed: 12,
            available: 8
        }
    );
    assert_eq!(kinds(&short), vec![("truncated", Severity::Error, 0)]);

    // An ItemVariationStore offset past the table.
    let mut far = base11(None, None, None);
    far[8..12].copy_from_slice(&100u32.to_be_bytes());
    assert_eq!(
        kinds(&far),
        vec![("offset_out_of_bounds", Severity::Error, 8)]
    );
}

#[test]
fn base_header_errors() {
    assert_eq!(
        Base::new(&[0, 1, 0]).unwrap_err(),
        ReadError::Truncated {
            needed: 8,
            available: 3
        }
    );
    assert_eq!(kinds(&[0, 1, 0]), vec![("truncated", Severity::Error, 0)]);
    let v2 = Buf::new().u32(0x0002_0000).u16(0).u16(0).done();
    assert_eq!(
        Base::new(&v2).unwrap_err(),
        ReadError::UnknownVersion(0x0002_0000)
    );
    let d = collect(&v2);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].kind, Kind::UnknownVersion { found: 0x0002_0000 });
}

#[test]
fn base_coord_formats_and_sizes() {
    for (bytes, format) in [
        (base_coord(&Coord::F1(5)), 1),
        (base_coord(&Coord::F2(5, 1, 2)), 2),
        (base_coord(&Coord::F3(5, None)), 3),
        (base_coord(&Coord::F4(5, 1, 2)), 4),
    ] {
        let c = BaseCoord::new(&bytes).unwrap();
        assert_eq!((c.format(), c.coordinate()), (format, 5));
        let short = &bytes[..bytes.len() - 1];
        assert_eq!(
            BaseCoord::new(short).unwrap_err(),
            ReadError::Truncated {
                needed: bytes.len(),
                available: bytes.len() - 1
            }
        );
    }
    assert_eq!(
        BaseCoord::new(&[0, 5, 0, 0]).unwrap_err(),
        ReadError::UnknownFormat(5)
    );
    let f3 = base_coord(&Coord::F3(5, None));
    let c = BaseCoord::new(&f3).unwrap();
    assert_eq!(c.device_offset(), Some(0));
    assert!(c.device().unwrap().is_none());
}

#[test]
fn device_headers() {
    // Format 1: 2-bit deltas, 8 to a word; sizes 9..=17 need 2 words.
    let d = device(9, 17, 1, &[0, 0]);
    assert_eq!(device::size(9, 17, 1), 10);
    let v = Device::new(&d).unwrap();
    assert_eq!(v.data().len(), 10);
    assert_eq!(
        Device::new(&d[..9]).unwrap_err(),
        ReadError::Truncated {
            needed: 10,
            available: 9
        }
    );
    // Format 2: 4 to a word; format 3: 2 to a word.
    assert_eq!(device::size(9, 17, 2), 12);
    assert_eq!(device::size(9, 17, 3), 16);
    // Start after end, and unknown formats: the header only.
    assert_eq!(device::size(17, 9, 1), 6);
    assert_eq!(device::size(0, 100, 7), 6);
    let var = device(3, 7, device::VARIATION_INDEX, &[]);
    let v = Device::new(&var).unwrap();
    assert_eq!(v.variation_index(), Some((3, 7)));
    assert_eq!(v.delta_format(), 0x8000);
    assert_eq!(
        Device::new(&var[..5]).unwrap_err(),
        ReadError::Truncated {
            needed: 6,
            available: 5
        }
    );
}

#[test]
fn base_bsearch_follows_harfbuzz_on_unsorted_tags() {
    // HarfBuzz probes (min + max) / 2: with [b, a], the middle of
    // [0, 1] is 0 ('b'), and 'a' < 'b' moves max below 0: not found.
    let axis = AxisSpec {
        tags: Some(vec![*b"romn", *b"ideo"]),
        ..Default::default()
    };
    let data = base(Some(&axis), None);
    let b = Base::new(&data).unwrap();
    let tags = b
        .horiz_axis()
        .unwrap()
        .unwrap()
        .base_tag_list()
        .unwrap()
        .unwrap();
    assert_eq!(tags.find(t(b"ideo")), None);
    assert_eq!(tags.find(t(b"romn")), Some(0));
    // The tag list starts at 8 + 4 (after the header and the axis):
    // the tag of index 1 is at 12 + 2 + 4.
    assert_eq!(kinds(&data), vec![("tags_not_sorted", Severity::Error, 18)]);
}

#[test]
fn base_validator_reports_order_counts_and_formats() {
    let axis = AxisSpec {
        tags: Some(vec![*b"ideo", *b"romn"]),
        scripts: Some(vec![
            (
                *b"latn",
                Some(ScriptSpec {
                    values: Some((2, vec![Some(Coord::F1(0))])),
                    default_min_max: Some(MinMaxSpec {
                        min: Some(Coord::Raw(vec![0, 9, 0, 0])),
                        max: None,
                        features: vec![(*b"liga", None, None), (*b"kern", None, None)],
                    }),
                    lang_sys: vec![(*b"TRK ", None), (*b"ENG ", None)],
                }),
            ),
            (*b"DFLT", None),
        ]),
    };
    let data = base(Some(&axis), None);
    let ds = collect(&data);
    let ids: Vec<_> = ds.iter().map(|d| d.kind.id()).collect();
    assert_eq!(
        ids,
        vec![
            "tags_not_sorted",
            "base_coord_count_mismatch",
            "default_baseline_out_of_range",
            "unknown_format",
            "tags_not_sorted",
            "tags_not_sorted",
        ],
        "{ds:?}"
    );
    let mismatch = ds
        .iter()
        .find(|d| d.kind.id() == "base_coord_count_mismatch")
        .unwrap();
    let mut details = Vec::new();
    mismatch.kind.details(&mut |k, v| details.push((k, v)));
    assert_eq!(
        details,
        vec![
            ("tags", DetailValue::Uint(2)),
            ("coords", DetailValue::Uint(1))
        ]
    );
    let out = ds
        .iter()
        .find(|d| d.kind.id() == "default_baseline_out_of_range")
        .unwrap();
    assert_eq!(out.severity, Severity::Warning);
    assert_eq!(
        out.kind,
        Kind::DefaultBaselineOutOfRange { index: 2, count: 2 }
    );
    assert_eq!(
        ds.iter()
            .find(|d| d.kind.id() == "unknown_format")
            .unwrap()
            .kind,
        Kind::UnknownFormat { found: 9 }
    );
}

#[test]
fn base_validator_reports_offsets_out_of_bounds() {
    let good = base(Some(&simple_axis(b"DFLT", &[(*b"romn", 0)])), None);
    // The horizontal axis offset at 4, pointing past the table.
    let mut far = good.clone();
    far[4..6].copy_from_slice(&200u16.to_be_bytes());
    let d = collect(&far);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].offset, 4);
    assert_eq!(
        d[0].kind,
        Kind::OffsetOutOfBounds {
            offset: 200,
            length: 0,
            available: good.len() as u32
        }
    );
    // Cut the last byte: the format 1 coordinate no longer fits.
    let cut = &good[..good.len() - 1];
    let d = collect(cut);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].kind.id(), "offset_out_of_bounds");
    assert_eq!(d[0].severity, Severity::Error);
    // A Device table that does not fit.
    let axis = AxisSpec {
        tags: Some(vec![*b"romn"]),
        scripts: Some(vec![(
            *b"DFLT",
            Some(ScriptSpec {
                values: Some((0, vec![Some(Coord::F3(1, Some(device(9, 40, 3, &[]))))])),
                ..Default::default()
            }),
        )]),
    };
    let d = collect(&base(Some(&axis), None));
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(matches!(
        d[0].kind,
        Kind::OffsetOutOfBounds { length: 38, .. }
    ));
}

#[test]
fn base_is_validated_with_the_font() {
    let mut tables = sample_tables();
    let mut bad = base(Some(&simple_axis(b"DFLT", &[(*b"romn", 0)])), None);
    bad[4..6].copy_from_slice(&200u16.to_be_bytes());
    tables.push((b"BASE", bad));
    tables.sort_by_key(|(t, _)| **t);
    let font = sfnt(&tables);
    let s = sofel_read::sfnt::Sfnt::new(&font).unwrap();
    let mut found = Vec::new();
    sofel_read::validate::sfnt(&s, &mut |d| {
        if d.table == base::TAG {
            found.push(d.kind.id());
        }
    });
    assert_eq!(found, vec!["offset_out_of_bounds"]);
}
