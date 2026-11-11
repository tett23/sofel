//! Stored and recomputed region scalars under the source-identity rule of ADR 0030.

use crate::{Bench, ffi::HbFont};
use sofel_font::{
    Font,
    read::{Tag, hvar::Hvar, mvar::Mvar},
    var::{HbF32, Instance, Location, StoreKind, Variation},
};
use std::path::Path;

const FONTS: [&str; 3] = [
    "RobotoFlex-Variable.ttf",
    "SourceSerifVariable-Roman.ttf",
    "AdobeVFPrototype.otf",
];

fn delta(instance: Instance<'_, '_, '_, '_>, store: StoreKind, index: u32) -> f32 {
    instance
        .item_delta::<HbF32>(store, (index >> 16) as u16, index as u16)
        .unwrap()
}

fn digest(values: impl Iterator<Item = f32>) -> u64 {
    values.fold(0u64, |acc, v| acc.rotate_left(7) ^ u64::from(v.to_bits()))
}

pub fn items(b: &mut Bench, testdata: &Path) {
    let hvar = [
        "HVAR item delta cached",
        "HVAR item delta copied font",
        "HVAR item delta different font",
    ];
    let mvar = "MVAR item delta static default";
    let dir = testdata.join("harfbuzz/test/subset/data/fonts");
    for (i, name) in FONTS.iter().enumerate() {
        let want_hvar = hvar.iter().any(|label| b.wanted(name, label));
        if !want_hvar && !b.wanted(name, mvar) {
            continue;
        }
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        if want_hvar {
            let variation = [Variation {
                tag: Tag::new(b"wght"),
                value: 500.0,
            }];
            let location = Location::new(&font, None, &variation);
            let copy = bytes.clone();
            let copied = Font::new(&copy, 0).unwrap();
            let source_bytes = std::fs::read(dir.join(FONTS[(i + 1) % FONTS.len()])).unwrap();
            let source = Font::new(&source_bytes, 0).unwrap();
            let foreign_location = Location::new(&source, None, &variation);
            // A location from another font supplies its normalized coordinates
            // in axis order. Construct an equivalent cache on the target.
            let normalized =
                Location::from_normalized_2_14(&font, foreign_location.coords_normalized());
            let table = Hvar::new(font.table(Tag::new(b"HVAR")).unwrap()).unwrap();
            let map = table.advance_map().unwrap();
            let indices: Vec<_> = (0..font.num_glyphs())
                .map(|g| map.map_or(g, |map| map.map(g)))
                .collect();
            let cached = font.instance(&location);
            for (label, selected, reference) in [
                (hvar[0], cached, copied.instance(&location)),
                (hvar[1], copied.instance(&location), cached),
                (
                    hvar[2],
                    font.instance(&foreign_location),
                    font.instance(&normalized),
                ),
            ] {
                if !b.wanted(name, label) {
                    continue;
                }
                for &index in &indices {
                    assert_eq!(
                        delta(selected, StoreKind::Hvar, index),
                        delta(reference, StoreKind::Hvar, index),
                        "{name}: {label}, index {index}"
                    );
                }
                b.item(
                    name,
                    label.into(),
                    &mut || digest(indices.iter().map(|&j| delta(selected, StoreKind::Hvar, j))),
                    None,
                    None,
                );
            }
        }
        if b.wanted(name, mvar)
            && let Some(bytes_mvar) = font.table(Tag::new(b"MVAR"))
        {
            let table = Mvar::new(bytes_mvar).unwrap();
            let records: Vec<_> = (0..table.record_count())
                .map(|i| table.record(i).unwrap())
                .collect();
            let hb = HbFont::open(&bytes).unwrap();
            let instance = font.default_instance();
            let indices: Vec<_> = records
                .iter()
                .map(|r| {
                    let index = (u32::from(r.outer) << 16) | u32::from(r.inner);
                    assert_eq!(
                        delta(instance, StoreKind::Mvar, index),
                        hb.metric_variation(r.tag.to_u32()),
                        "{name}: {}",
                        r.tag
                    );
                    index
                })
                .collect();
            b.item(
                name,
                mvar.into(),
                &mut || {
                    digest(
                        indices
                            .iter()
                            .map(|&index| delta(instance, StoreKind::Mvar, index)),
                    )
                },
                None,
                None,
            );
        }
    }
}
