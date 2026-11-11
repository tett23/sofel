//! Shaping-time table admission, before GDEF blocklisting (sofel ADR 0059).
#![allow(clippy::indexing_slicing)]
use sofel_read::{
    Tag,
    layout::sanitize::{Admission, assess},
};

#[test]
fn inv1_layout_sanitization() {
    for (tag, header) in [(b"GSUB", 10u8), (b"GPOS", 10), (b"GDEF", 12)] {
        let tag = Tag::new(tag);
        assert_eq!(assess(tag, None), Admission::Absent);
        let mut data = vec![0; usize::from(header)];
        data[1] = 1;
        assert_eq!(assess(tag, Some(&data)), Admission::Accepted);
        data[5] = header;
        data.extend(if tag == Tag::new(b"GDEF") {
            &[0, 2, 0, 0][..]
        } else {
            &[0, 0][..]
        });
        assert_eq!(assess(tag, Some(&data)), Admission::Accepted);
        data.pop();
        assert_eq!(assess(tag, Some(&data)), Admission::Discarded);
    }
}

fn layout(ty: u16, words: &[u16]) -> Vec<u8> {
    [1, 0, 0, 0, 10, 1, 4, ty, 0, 1, 8]
        .into_iter()
        .chain(words.iter().copied())
        .flat_map(u16::to_be_bytes)
        .collect()
}

#[test]
fn inv1_pair_set_is_checked_but_device_offset_is_deferred() {
    let table = layout(2, &[1, 0, 0x10, 0, 1, 12, 1, 1, 0xffff]);
    let tag = Tag::new(b"GPOS");
    assert_eq!(assess(tag, Some(&table)), Admission::Accepted);
    assert_eq!(
        assess(tag, table.get(..table.len() - 1)),
        Admission::Discarded
    );
}

#[test]
fn inv1_context_class_def_must_fit() {
    let table = layout(5, &[2, 0, 0xffff, 0]);
    assert_eq!(
        assess(Tag::new(b"GSUB"), Some(&table)),
        Admission::Discarded
    );
}

#[test]
fn inv1_unknown_lookup_does_not_read_subtable_format() {
    for tag in [b"GSUB", b"GPOS"] {
        let table = layout(400, &[]);
        assert_eq!(assess(Tag::new(tag), Some(&table)), Admission::Accepted);
    }
}
