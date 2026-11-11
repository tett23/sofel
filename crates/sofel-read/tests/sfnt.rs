#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
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

#[test]
fn inv1_dfont_faces() {
    let first = sfnt(&[(b"maxp", maxp05(3))]);
    let second = sfnt(&[(b"maxp", maxp05(4))]);
    let file = dfont(&[(b"sfnt", vec![(14116, first.clone()), (3, second.clone())])]);
    let FontFile::Dfont(faces) = FontFile::new(&file).unwrap() else {
        panic!("dfont");
    };
    assert_eq!(faces.num_fonts(), 2);
    assert_eq!(
        faces.font(0).unwrap().table(Tag::new(b"maxp")),
        Some(&maxp05(3)[..])
    );
    assert_eq!(
        faces.font(1).unwrap().table(Tag::new(b"maxp")),
        Some(&maxp05(4)[..])
    );
    let duplicate = dfont(&[
        (b"sfnt", vec![(14116, first)]),
        (b"sfnt", vec![(3, second)]),
    ]);
    let FontFile::Dfont(faces) = FontFile::new(&duplicate).unwrap() else {
        panic!("dfont");
    };
    assert_eq!(faces.num_fonts(), 1);
    assert_eq!(
        faces.font(0).unwrap().table(Tag::new(b"maxp")),
        Some(&maxp05(3)[..])
    );
}

#[test]
fn inv2_dfont_face_index() {
    for count in [0, 1, 2] {
        let sfnt = sfnt(&[(b"maxp", maxp05(3))]);
        let file = if count == 0 {
            dfont(&[(b"FOND", vec![(0, vec![1])])])
        } else {
            dfont(&[(b"sfnt", (0..count).map(|id| (id, sfnt.clone())).collect())])
        };
        let FontFile::Dfont(faces) = FontFile::new(&file).unwrap() else {
            panic!("dfont");
        };
        assert_eq!(faces.num_fonts(), u32::from(count));
        assert!(faces.font(u32::from(count)).is_err());
    }
}

#[test]
fn inv3_dfont_bounds() {
    let body = sfnt(&[(b"maxp", maxp05(3))]);
    let file = dfont(&[(b"sfnt", vec![(14116, body)])]);
    let FontFile::Dfont(faces) = FontFile::new(&file).unwrap() else {
        panic!("dfont");
    };
    assert!(faces.font(0).is_ok()); // body ends at the declared data boundary
    for lost in [1, 4, 16, 28] {
        assert!(FontFile::new(&file[..file.len() - lost]).is_err());
    }
    let mut long = file.clone();
    long[256..260].copy_from_slice(&((file.len() as u32) + 1).to_be_bytes());
    let FontFile::Dfont(faces) = FontFile::new(&long).unwrap() else {
        panic!("dfont");
    };
    assert!(faces.font(0).is_err());

    let mut first = sfnt(&[(b"maxp", maxp05(3))]);
    let outside_first = first.len() as u32 + 4;
    first[20..24].copy_from_slice(&outside_first.to_be_bytes());
    let adjacent = dfont(&[(
        b"sfnt",
        vec![(1, first), (2, sfnt(&[(b"maxp", maxp05(4))]))],
    )]);
    let FontFile::Dfont(faces) = FontFile::new(&adjacent).unwrap() else {
        panic!("dfont");
    };
    assert_eq!(faces.font(0).unwrap().table(Tag::new(b"maxp")), None);

    let mut wrong_map = file.clone();
    wrong_map[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    assert!(FontFile::new(&wrong_map).is_err());
    let mut wrong_data = file.clone();
    wrong_data[8..12].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    assert!(FontFile::new(&wrong_data).is_err());
    let map_offset = u32::from_be_bytes(file[4..8].try_into().unwrap()) as usize;
    let mut wrong_list = file.clone();
    wrong_list[map_offset + 36..map_offset + 38].copy_from_slice(&0xFFFFu16.to_be_bytes());
    assert!(FontFile::new(&wrong_list).is_err());
    let mut wrong_resource = file.clone();
    wrong_resource[map_offset + 43..map_offset + 46].copy_from_slice(&[0xFF; 3]);
    let FontFile::Dfont(faces) = FontFile::new(&wrong_resource).unwrap() else {
        panic!("dfont");
    };
    assert!(faces.font(0).is_err());
}
