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

use sofel_read::sfnt::{Collection, FontFile, Sfnt};
use sofel_read::{ReadError, Tag};

#[test]
fn finds_tables() {
    let font = sample_font();
    let s = Sfnt::new(&font).unwrap();
    assert_eq!(s.version(), 0x0001_0000);
    assert_eq!(s.num_tables(), 5);
    assert_eq!(s.table(Tag::new(b"head")).unwrap().len(), 54);
    assert_eq!(s.table(Tag::new(b"maxp")).unwrap(), &maxp05(28)[..]);
    assert_eq!(s.table(Tag::new(b"GSUB")), None);
    let tags: Vec<Tag> = s.records().map(|r| r.tag).collect();
    assert_eq!(tags.len(), 5);
    assert_eq!(tags[0], Tag::new(b"cmap"));
}

#[test]
fn accepts_each_sfnt_version() {
    for v in [0x0001_0000u32, 0x4F54_544F, 0x7472_7565, 0x7479_7031] {
        let font = sfnt_with_version(v, &[(b"maxp", maxp05(1))]);
        assert_eq!(Sfnt::new(&font).unwrap().version(), v);
    }
    let font = sfnt_with_version(0x1234_5678, &[(b"maxp", maxp05(1))]);
    assert!(matches!(
        Sfnt::new(&font),
        Err(ReadError::UnknownVersion(0x1234_5678))
    ));
}

#[test]
fn finds_tables_in_unsorted_directories() {
    let font = sfnt(&[(b"maxp", maxp05(3)), (b"head", head(HeadSpec::default()))]);
    let s = Sfnt::new(&font).unwrap();
    assert_eq!(s.table(Tag::new(b"head")).unwrap().len(), 54);
    assert_eq!(s.table(Tag::new(b"maxp")).unwrap(), &maxp05(3)[..]);
}

#[test]
fn rejects_truncated_directories() {
    let font = sample_font();
    assert!(matches!(
        Sfnt::new(&font[..11]),
        Err(ReadError::Truncated { .. })
    ));
    assert!(matches!(
        Sfnt::new(&font[..12 + 16 * 5 - 1]),
        Err(ReadError::Truncated { .. })
    ));
}

#[test]
fn out_of_bounds_tables_are_none() {
    let mut font = sfnt(&[(b"maxp", maxp05(3))]);
    // Set the maxp record's length far beyond the file.
    font[12 + 12..12 + 16].copy_from_slice(&0x0010_0000u32.to_be_bytes());
    let s = Sfnt::new(&font).unwrap();
    assert_eq!(s.table(Tag::new(b"maxp")), None);
}

#[test]
fn collections() {
    for v2 in [false, true] {
        let a = vec![(b"maxp", maxp05(3))];
        let b = vec![(b"maxp", maxp05(4)), (b"head", head(HeadSpec::default()))];
        let file = ttc(v2, &[a, b]);
        let c = Collection::new(&file).unwrap();
        assert_eq!(c.num_fonts(), 2);
        assert_eq!(c.major_version(), if v2 { 2 } else { 1 });
        let f0 = c.font(0).unwrap();
        let f1 = c.font(1).unwrap();
        assert_eq!(f0.table(Tag::new(b"maxp")).unwrap(), &maxp05(3)[..]);
        assert_eq!(f1.table(Tag::new(b"maxp")).unwrap(), &maxp05(4)[..]);
        assert_eq!(f1.num_tables(), 2);
        assert!(c.font(2).is_err());
    }
}

#[test]
fn rejects_non_collections() {
    let font = sample_font();
    assert!(matches!(
        Collection::new(&font),
        Err(ReadError::UnknownVersion(_))
    ));
    let file = ttc(false, &[vec![(b"maxp", maxp05(3))]]);
    assert!(matches!(
        Collection::new(&file[..15]),
        Err(ReadError::Truncated { .. })
    ));
}

#[test]
fn collection_offsets_out_of_bounds() {
    let mut file = ttc(false, &[vec![(b"maxp", maxp05(3))]]);
    file[12..16].copy_from_slice(&0x00FF_FFFFu32.to_be_bytes());
    let c = Collection::new(&file).unwrap();
    assert!(c.font(0).is_err());
}

#[test]
fn font_files() {
    let font = sample_font();
    assert!(matches!(FontFile::new(&font), Ok(FontFile::Sfnt(_))));
    let file = ttc(true, &[vec![(b"maxp", maxp05(3))]]);
    assert!(matches!(FontFile::new(&file), Ok(FontFile::Collection(_))));
    assert!(FontFile::new(b"wOFF0000").is_err());
}
