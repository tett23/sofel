//! The tables of ADR 0016: OS/2, name, post, vhea/vmtx and VORG, with
//! their validators.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;
use support::build::*;

use sofel_read::name::Name;
use sofel_read::os2::Os2;
use sofel_read::post::{self, Post};
use sofel_read::sfnt::Sfnt;
use sofel_read::vorg::Vorg;
use sofel_read::{
    CountWidth, DetailValue, Diagnostic, Fixed, GlyphId, ReadError, Severity, Tag, hhea, hmtx,
    name, os2, validate, vhea, vmtx, vorg,
};

fn collect(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    f(&mut |d| out.push(d));
    out
}

fn ids(ds: &[Diagnostic]) -> Vec<(&'static str, Severity, u32)> {
    ds.iter()
        .map(|d| (d.kind.id(), d.severity, d.offset))
        .collect()
}

fn details(d: &Diagnostic) -> Vec<(&'static str, DetailValue)> {
    let mut out = Vec::new();
    d.kind.details(&mut |k, v| out.push((k, v)));
    out
}

// ---------------------------------------------------------------- OS/2

#[test]
fn os2_lengths_by_version() {
    for (version, len) in [
        (0u16, 78usize),
        (1, 86),
        (2, 96),
        (3, 96),
        (4, 96),
        (5, 100),
    ] {
        let data = os2(Os2Spec {
            version,
            ..Default::default()
        });
        assert_eq!(data.len(), len, "version {version}");
        let o = Os2::new(&data).unwrap();
        assert_eq!(o.version(), version);
        assert!(matches!(
            Os2::new(&data[..len - 1]),
            Err(ReadError::Truncated { needed, available }) if needed == len && available == len - 1
        ));
    }
    assert!(matches!(
        Os2::new(&[0, 0, 0]),
        Err(ReadError::Truncated {
            needed: 78,
            available: 3
        })
    ));
}

#[test]
fn os2_fields() {
    let data = os2(Os2Spec {
        version: 5,
        ..Default::default()
    });
    let o = Os2::new(&data).unwrap();
    assert_eq!(o.x_avg_char_width(), 510);
    assert_eq!(o.us_weight_class(), 400);
    assert_eq!(o.us_width_class(), 5);
    assert_eq!(o.fs_type(), 8);
    assert_eq!(
        (
            o.y_subscript_x_size(),
            o.y_subscript_y_size(),
            o.y_subscript_x_offset(),
            o.y_subscript_y_offset()
        ),
        (650, 600, 10, 75)
    );
    assert_eq!(
        (
            o.y_superscript_x_size(),
            o.y_superscript_y_size(),
            o.y_superscript_x_offset(),
            o.y_superscript_y_offset()
        ),
        (651, 601, 11, 350)
    );
    assert_eq!((o.y_strikeout_size(), o.y_strikeout_position()), (50, 260));
    assert_eq!(o.s_family_class(), 0x0801);
    assert_eq!(o.panose(), [2, 11, 5, 3, 2, 2, 2, 2, 2, 4]);
    assert_eq!(o.ul_unicode_range(), [0x8000_0003, 0x4000_0000, 0x10, 0x1]);
    assert_eq!(o.ach_vend_id(), Tag::new(b"SOFL"));
    assert_eq!(o.fs_selection(), 0x40);
    assert_eq!(
        (o.us_first_char_index(), o.us_last_char_index()),
        (0x20, 0xFFFF)
    );
    assert_eq!(
        (
            o.s_typo_ascender(),
            o.s_typo_descender(),
            o.s_typo_line_gap()
        ),
        (750, -250, 100)
    );
    assert_eq!((o.us_win_ascent(), o.us_win_descent()), (900, 300));
    assert_eq!(o.ul_code_page_range(), Some([0x2000_0001, 0x8000_0000]));
    assert_eq!(o.sx_height(), Some(500));
    assert_eq!(o.s_cap_height(), Some(700));
    assert_eq!(o.us_default_char(), Some(0));
    assert_eq!(o.us_break_char(), Some(0x20));
    assert_eq!(o.us_max_context(), Some(3));
    assert_eq!(o.us_lower_optical_point_size(), Some(80));
    assert_eq!(o.us_upper_optical_point_size(), Some(720));
}

#[test]
fn os2_fields_past_the_version_are_none() {
    let data = os2(Os2Spec {
        version: 0,
        ..Default::default()
    });
    let o = Os2::new(&data).unwrap();
    assert_eq!(o.ul_code_page_range(), None);
    assert_eq!(o.sx_height(), None);
    assert_eq!(o.us_max_context(), None);
    assert_eq!(o.us_lower_optical_point_size(), None);

    let data = os2(Os2Spec {
        version: 1,
        ..Default::default()
    });
    let o = Os2::new(&data).unwrap();
    assert!(o.ul_code_page_range().is_some());
    assert_eq!(o.s_cap_height(), None);

    let data = os2(Os2Spec {
        version: 4,
        ..Default::default()
    });
    let o = Os2::new(&data).unwrap();
    assert_eq!(o.s_cap_height(), Some(700));
    assert_eq!(o.us_upper_optical_point_size(), None);
}

#[test]
fn os2_versions_above_5_read_the_version_5_fields() {
    let mut data = os2(Os2Spec {
        version: 5,
        ..Default::default()
    });
    data[1] = 9;
    let o = Os2::new(&data).unwrap();
    assert_eq!(o.version(), 9);
    assert_eq!(o.us_upper_optical_point_size(), Some(720));
    assert!(matches!(
        Os2::new(&data[..96]),
        Err(ReadError::Truncated {
            needed: 100,
            available: 96
        })
    ));
}

#[test]
fn os2_validator() {
    let good = os2(Os2Spec::default());
    assert!(collect(|r| os2::validate(&good, r)).is_empty());

    let ds = collect(|r| os2::validate(&good[..90], r));
    assert_eq!(ids(&ds), vec![("truncated", Severity::Error, 0)]);

    let mut long = good.clone();
    long.extend_from_slice(&[0, 0]);
    let ds = collect(|r| os2::validate(&long, r));
    assert_eq!(ids(&ds), vec![("length_mismatch", Severity::Warning, 0)]);
    assert_eq!(
        details(&ds[0]),
        vec![
            ("declared", DetailValue::Uint(96)),
            ("available", DetailValue::Uint(98))
        ]
    );

    let bad = os2(Os2Spec {
        weight_class: 1001,
        fs_selection: 0x0041,
        first_char: 0x100,
        last_char: 0x20,
        ..Default::default()
    });
    let ds = collect(|r| os2::validate(&bad, r));
    assert_eq!(
        ids(&ds),
        vec![
            ("weight_class_out_of_range", Severity::Warning, 4),
            ("fs_selection_conflict", Severity::Warning, 62),
            ("first_char_after_last", Severity::Warning, 64),
        ]
    );
    assert_eq!(details(&ds[0]), vec![("found", DetailValue::Uint(1001))]);
    assert_eq!(details(&ds[1]), vec![("found", DetailValue::Uint(0x41))]);
    assert_eq!(
        details(&ds[2]),
        vec![
            ("first", DetailValue::Uint(0x100)),
            ("last", DetailValue::Uint(0x20))
        ]
    );

    let zero = os2(Os2Spec {
        weight_class: 0,
        fs_selection: 0x0060,
        ..Default::default()
    });
    let ds = collect(|r| os2::validate(&zero, r));
    assert_eq!(
        ids(&ds),
        vec![
            ("weight_class_out_of_range", Severity::Warning, 4),
            ("fs_selection_conflict", Severity::Warning, 62),
        ]
    );
}

// ---------------------------------------------------------------- name

fn sample_name(format: u16) -> Vec<u8> {
    let ps = utf16be("Sofel-Regular");
    let records: Vec<NameRec<'_>> = vec![
        (1, 0, 0, 1, b"Sofel"),
        (1, 0, 0, 6, b"Sofel-Mac"),
        (3, 1, 0x409, 1, &[0, b'S']),
        (3, 1, 0x409, 6, &ps),
        (3, 1, 0x8000, 6, &ps),
    ];
    name(format, &records, &[b"en-US"])
}

#[test]
fn name_records_and_strings() {
    let data = sample_name(0);
    let n = Name::new(&data).unwrap();
    assert_eq!(n.format(), 0);
    assert_eq!(n.count(), 5);
    assert_eq!(n.storage_offset(), 6 + 12 * 5);
    assert_eq!(n.lang_tag_count(), 0);
    let r = n.record(1).unwrap();
    assert_eq!(
        (
            r.platform_id,
            r.encoding_id,
            r.language_id,
            r.name_id,
            r.length,
            r.offset
        ),
        (1, 0, 0, 6, 9, 5)
    );
    assert_eq!(n.string(&r), Some(&b"Sofel-Mac"[..]));
    assert_eq!(n.record(5), None);
    assert_eq!(n.records().count(), 5);
    assert_eq!(
        n.records().map(|r| r.name_id).collect::<Vec<_>>(),
        vec![1, 6, 1, 6, 6]
    );
    assert_eq!(
        n.string(&n.record(3).unwrap()),
        Some(&utf16be("Sofel-Regular")[..])
    );
}

#[test]
fn name_format_1_language_tags() {
    let data = sample_name(1);
    let n = Name::new(&data).unwrap();
    assert_eq!(n.format(), 1);
    assert_eq!(n.lang_tag_count(), 1);
    let t = n.lang_tag(0).unwrap();
    assert_eq!(n.lang_tag_string(&t), Some(&b"en-US"[..]));
    assert_eq!(n.lang_tag(1), None);
    // Strings come after the language tag records.
    assert_eq!(n.storage_offset(), 6 + 12 * 5 + 2 + 4);
    assert_eq!(n.string(&n.record(0).unwrap()), Some(&b"Sofel"[..]));
}

#[test]
fn name_out_of_range_strings_are_none() {
    let mut data = sample_name(0);
    // Record 1: offset past the storage area.
    let at = 6 + 12 + 10;
    data[at..at + 2].copy_from_slice(&0x7000u16.to_be_bytes());
    let n = Name::new(&data).unwrap();
    assert_eq!(n.string(&n.record(1).unwrap()), None);
    assert!(n.string(&n.record(0).unwrap()).is_some());
}

#[test]
fn name_errors() {
    let data = sample_name(0);
    assert!(matches!(
        Name::new(&data[..5]),
        Err(ReadError::Truncated { needed: 6, .. })
    ));
    assert!(matches!(
        Name::new(&data[..6 + 12 * 4]),
        Err(ReadError::Truncated { needed: 66, .. })
    ));
    let mut bad = data.clone();
    bad[1] = 2;
    assert!(matches!(Name::new(&bad), Err(ReadError::UnknownFormat(2))));
}

#[test]
fn name_validator() {
    assert!(collect(|r| name::validate(&sample_name(0), r)).is_empty());
    assert!(collect(|r| name::validate(&sample_name(1), r)).is_empty());

    let records: Vec<NameRec<'_>> = vec![
        (3, 1, 0x409, 6, b"abc"),
        (1, 0, 0, 1, b"Sofel"),
        (0, 3, 0, 1, b"x"),
    ];
    let mut data = name(0, &records, &[]);
    // Record 1 (the Mac one) runs past the table.
    let at = 6 + 12 + 8;
    data[at..at + 2].copy_from_slice(&100u16.to_be_bytes());
    let ds = collect(|r| name::validate(&data, r));
    assert_eq!(
        ids(&ds),
        vec![
            ("odd_length", Severity::Warning, 6),
            ("records_not_sorted", Severity::Warning, 18),
            ("offset_out_of_bounds", Severity::Error, 18),
            ("records_not_sorted", Severity::Warning, 30),
            ("odd_length", Severity::Warning, 30),
        ]
    );
    assert_eq!(
        details(&ds[0]),
        vec![
            ("index", DetailValue::Uint(0)),
            ("length", DetailValue::Uint(3))
        ]
    );

    let mut bad = sample_name(0);
    bad[1] = 7;
    assert_eq!(
        ids(&collect(|r| name::validate(&bad, r))),
        vec![("unknown_format", Severity::Error, 0)]
    );
    assert_eq!(
        ids(&collect(|r| name::validate(&bad[..4], r))),
        vec![("truncated", Severity::Error, 0)]
    );

    let mut tag_out = sample_name(1);
    let at = 6 + 12 * 5 + 2 + 2;
    tag_out[at..at + 2].copy_from_slice(&0x4000u16.to_be_bytes());
    assert_eq!(
        ids(&collect(|r| name::validate(&tag_out, r))),
        vec![("offset_out_of_bounds", Severity::Error, 68)]
    );
}

// ---------------------------------------------------------------- post

#[test]
fn post_header_fields() {
    for version in [0x0001_0000u32, 0x0003_0000] {
        let data = post_header(version).done();
        let p = Post::new(&data).unwrap();
        assert_eq!(p.version(), version);
        assert_eq!(p.italic_angle(), Fixed(-0x000C_8000));
        assert_eq!(p.underline_position(), -75);
        assert_eq!(p.underline_thickness(), 50);
        assert_eq!(p.is_fixed_pitch(), 1);
        assert_eq!(
            (
                p.min_mem_type42(),
                p.max_mem_type42(),
                p.min_mem_type1(),
                p.max_mem_type1()
            ),
            (2, 3, 4, 5)
        );
        assert_eq!(p.num_glyphs(), None);
        assert_eq!(p.glyph_name_index(GlyphId(0)), None);
    }
    assert!(matches!(
        Post::new(&post_header(0x0001_0000).done()[..31]),
        Err(ReadError::Truncated {
            needed: 32,
            available: 31
        })
    ));
    assert!(matches!(
        Post::new(&post_header(0x0001_8000).done()),
        Err(ReadError::UnknownVersion(0x0001_8000))
    ));
}

#[test]
fn post_version_2_names() {
    let data = post2(&[0, 3, 258, 259, 258], &["alpha", "beta"]);
    let p = Post::new(&data).unwrap();
    assert_eq!(p.version(), 0x0002_0000);
    assert_eq!(p.num_glyphs(), Some(5));
    assert_eq!(p.glyph_name_index(GlyphId(1)), Some(3));
    assert_eq!(p.glyph_name_index(GlyphId(3)), Some(259));
    assert_eq!(p.glyph_name_index(GlyphId(5)), None);
    let names: Vec<&[u8]> = p.names().collect();
    assert_eq!(names, vec![&b"alpha"[..], &b"beta"[..]]);
    assert!(matches!(
        Post::new(&data[..32 + 2 + 9]),
        Err(ReadError::Truncated { needed: 44, .. })
    ));
}

#[test]
fn post_names_stop_at_a_string_past_the_end() {
    let mut data = post2(&[258, 259], &["alpha", "beta"]);
    data.truncate(data.len() - 1);
    let p = Post::new(&data).unwrap();
    let names: Vec<&[u8]> = p.names().collect();
    assert_eq!(names, vec![&b"alpha"[..]]);
}

#[test]
fn post_version_2_5_and_4() {
    let data = post_header(0x0002_5000).u16(3).u8(1).u8(0xFF).u8(5).done();
    let p = Post::new(&data).unwrap();
    assert_eq!(p.num_glyphs(), Some(3));
    assert_eq!(p.v25_offset(GlyphId(0)), Some(1));
    assert_eq!(p.v25_offset(GlyphId(1)), Some(-1));
    assert_eq!(p.v25_offset(GlyphId(3)), None);
    assert!(matches!(
        Post::new(&data[..36]),
        Err(ReadError::Truncated { needed: 37, .. })
    ));

    let data = post_header(0x0004_0000).u16(0x41).u16(0xFFFF).done();
    let p = Post::new(&data).unwrap();
    assert_eq!(p.v4_code(GlyphId(0)), Some(0x41));
    assert_eq!(p.v4_code(GlyphId(1)), Some(0xFFFF));
    assert_eq!(p.v4_code(GlyphId(2)), None);
}

#[test]
fn mac_glyph_names() {
    assert_eq!(post::MAC_GLYPH_NAMES.len(), 258);
    assert_eq!(post::mac_glyph_name(0), Some(".notdef"));
    assert_eq!(post::mac_glyph_name(3), Some("space"));
    assert_eq!(post::mac_glyph_name(36), Some("A"));
    assert_eq!(post::mac_glyph_name(257), Some("dcroat"));
    assert_eq!(post::mac_glyph_name(258), None);
}

#[test]
fn post_validator() {
    assert!(collect(|r| post::validate(&post2(&[0, 258], &["a"]), r)).is_empty());
    assert!(collect(|r| post::validate(&post_header(0x0003_0000).done(), r)).is_empty());

    let data = post2(&[0, 258, 260], &["a", "b"]);
    let ds = collect(|r| post::validate(&data, r));
    assert_eq!(
        ids(&ds),
        vec![("name_index_out_of_range", Severity::Error, 38)]
    );
    assert_eq!(
        details(&ds[0]),
        vec![
            ("glyph", DetailValue::Uint(2)),
            ("index", DetailValue::Uint(260))
        ]
    );

    let mut short = post2(&[258], &["abc"]);
    short.truncate(short.len() - 1);
    let ds = collect(|r| post::validate(&short, r));
    assert_eq!(
        ids(&ds),
        vec![
            ("truncated", Severity::Error, 36),
            ("name_index_out_of_range", Severity::Error, 34),
        ]
    );

    let v25 = post_header(0x0002_5000).u16(1).u8(0).done();
    assert_eq!(
        ids(&collect(|r| post::validate(&v25, r))),
        vec![("deprecated_version", Severity::Warning, 0)]
    );
    assert_eq!(
        ids(&collect(|r| post::validate(&post_header(7).done(), r))),
        vec![("unknown_version", Severity::Error, 0)]
    );
    assert_eq!(
        ids(&collect(|r| post::validate(&[0, 1], r))),
        vec![("truncated", Severity::Error, 0)]
    );
}

// ---------------------------------------------------------- vhea, vmtx

#[test]
fn vhea_and_vmtx_use_the_hhea_and_hmtx_views() {
    assert_eq!(vhea::TAG, Tag::new(b"vhea"));
    assert_eq!(vhea::TAG_WIDE, Tag::new(b"VHEA"));
    assert_eq!(vmtx::TAG, Tag::new(b"vmtx"));
    assert_eq!(vmtx::TAG_WIDE, Tag::new(b"VMTX"));
    let (narrow, wide, metrics) = (
        hhea(2),
        hhea_wide(70_000),
        hmtx(&[(1000, 80), (900, 70)], &[60]),
    );
    let v = vhea::Vhea::new(&narrow, CountWidth::U16).unwrap();
    assert_eq!(v.ascender(), 800);
    assert_eq!(v.number_of_h_metrics(), 2);
    let wide = vhea::Vhea::new(&wide, CountWidth::U24).unwrap();
    assert_eq!(wide.number_of_h_metrics(), 70_000);
    let m = vmtx::Vmtx::new(&metrics, 2, 3).unwrap();
    assert_eq!(m.advance(GlyphId(2)), Some(900));
    assert_eq!(m.left_side_bearing(GlyphId(2)), Some(60));
}

#[test]
fn vhea_and_vmtx_validators_label_their_tags() {
    let mut bad = hhea(2);
    bad[1] = 2;
    let ds = collect(|r| vhea::validate(&bad, CountWidth::U16, r));
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].table, vhea::TAG);
    let ds = collect(|r| vhea::validate(&bad, CountWidth::U24, r));
    assert_eq!(ds[0].table, vhea::TAG_WIDE);
    let ds = collect(|r| hhea::validate_tagged(vhea::TAG, &bad, CountWidth::U16, r));
    assert_eq!(ds[0].table, vhea::TAG);

    let ds = collect(|r| vmtx::validate(&hmtx(&[(1, 2)], &[]), 1, 3, r));
    assert_eq!(ids(&ds), vec![("truncated", Severity::Error, 4)]);
    assert_eq!(ds[0].table, vmtx::TAG);
    let ds = collect(|r| hmtx::validate_tagged(vmtx::TAG_WIDE, &[], 1, 1, r));
    assert_eq!(ds[0].table, vmtx::TAG_WIDE);
}

// ---------------------------------------------------------------- VORG

#[test]
fn vorg_versions_and_lookup() {
    let data = vorg(1, 880, &[(3, 900), (7, -10), (100, 0)]);
    let v = Vorg::new(&data).unwrap();
    assert_eq!((v.major_version(), v.minor_version()), (1, 0));
    assert_eq!(v.default_vert_origin_y(), 880);
    assert_eq!(v.num_records(), 3);
    assert_eq!(v.record(1), Some((GlyphId(7), -10)));
    assert_eq!(v.record(3), None);
    assert_eq!(v.lookup(GlyphId(7)), Some(-10));
    assert_eq!(v.lookup(GlyphId(8)), None);
    assert_eq!(v.y_origin(GlyphId(3)), 900);
    assert_eq!(v.y_origin(GlyphId(100)), 0);
    assert_eq!(v.y_origin(GlyphId(4)), 880);

    let data = vorg(2, 1000, &[(5, 1), (70_000, 2)]);
    let v = Vorg::new(&data).unwrap();
    assert_eq!(v.major_version(), 2);
    assert_eq!(v.num_records(), 2);
    assert_eq!(v.y_origin(GlyphId(70_000)), 2);
    assert_eq!(v.y_origin(GlyphId(70_001)), 1000);
}

#[test]
fn vorg_errors() {
    let data = vorg(1, 880, &[(3, 900), (7, -10)]);
    assert!(matches!(
        Vorg::new(&data[..7]),
        Err(ReadError::Truncated { needed: 8, .. })
    ));
    assert!(matches!(
        Vorg::new(&data[..15]),
        Err(ReadError::Truncated { needed: 16, .. })
    ));
    assert!(matches!(
        Vorg::new(&vorg(3, 0, &[])),
        Err(ReadError::UnknownVersion(0x0003_0000))
    ));
    let data = vorg(2, 0, &[(1, 1)]);
    assert!(matches!(
        Vorg::new(&data[..13]),
        Err(ReadError::Truncated { needed: 14, .. })
    ));
}

#[test]
fn vorg_validator() {
    let good = vorg(1, 880, &[(1, 1), (2, 2)]);
    assert!(collect(|r| vorg::validate(&good, Some(3), r)).is_empty());

    let bad = vorg(1, 880, &[(5, 1), (2, 2), (2, 3), (9, 4)]);
    let ds = collect(|r| vorg::validate(&bad, Some(8), r));
    assert_eq!(
        ids(&ds),
        vec![
            ("glyphs_not_sorted", Severity::Error, 12),
            ("duplicate_glyph", Severity::Error, 16),
            ("glyph_id_out_of_range", Severity::Warning, 20),
        ]
    );
    assert_eq!(details(&ds[0]), vec![("index", DetailValue::Uint(1))]);
    assert_eq!(
        details(&ds[1]),
        vec![
            ("index", DetailValue::Uint(2)),
            ("glyph", DetailValue::Uint(2))
        ]
    );

    let wide = vorg(2, 0, &[(1, 1), (1, 2)]);
    assert_eq!(
        ids(&collect(|r| vorg::validate(&wide, None, r))),
        vec![("duplicate_glyph", Severity::Error, 14)]
    );
    assert_eq!(
        ids(&collect(|r| vorg::validate(&vorg(9, 0, &[]), None, r))),
        vec![("unknown_version", Severity::Error, 0)]
    );
    assert_eq!(
        ids(&collect(|r| vorg::validate(&good[..10], None, r))),
        vec![("truncated", Severity::Error, 0)]
    );
}

// ------------------------------------------------------- whole font

#[test]
fn whole_font_validation_covers_the_new_tables() {
    let mut tables = sample_tables();
    let mut bad_os2 = os2(Os2Spec::default());
    bad_os2.truncate(90);
    tables.push((b"OS/2", bad_os2));
    tables.push((b"VORG", vorg(1, 0, &[(40, 1)])));
    tables.push((b"name", vec![0, 9, 0, 0, 0, 6]));
    tables.push((b"post", post2(&[0, 300], &[])));
    let mut bad_vhea = hhea(2);
    bad_vhea[0] = 3;
    tables.push((b"vhea", bad_vhea));
    tables.push((b"vmtx", hmtx(&[(1, 2)], &[])));
    tables.sort_by_key(|(t, _)| **t);
    let font = sfnt(&tables);
    let s = Sfnt::new(&font).unwrap();
    let ds = collect(|r| validate::sfnt(&s, r));
    let mut found: Vec<(Tag, &str)> = ds.iter().map(|d| (d.table, d.kind.id())).collect();
    found.sort();
    assert_eq!(
        found,
        vec![
            (Tag::new(b"OS/2"), "truncated"),
            (Tag::new(b"VORG"), "glyph_id_out_of_range"),
            (Tag::new(b"name"), "unknown_format"),
            (Tag::new(b"post"), "name_index_out_of_range"),
            (Tag::new(b"vhea"), "unknown_version"),
        ]
    );

    // A vmtx checked against vhea's count and the glyph count.
    let mut tables = sample_tables();
    tables.push((b"vhea", hhea(2)));
    tables.push((b"vmtx", hmtx(&[(1, 2)], &[])));
    tables.sort_by_key(|(t, _)| **t);
    let font = sfnt(&tables);
    let s = Sfnt::new(&font).unwrap();
    let ds = collect(|r| validate::sfnt(&s, r));
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!((ds[0].table, ds[0].kind.id()), (vmtx::TAG, "truncated"));
}
