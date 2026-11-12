//! VARC table views over HarfBuzz's fixed reference fonts (sofel ADR 0067).

#![allow(clippy::all)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_read::Tag;
use sofel_read::varc::Varc;

#[test]
fn adr0067_i3_varc_reference_records_are_bounded() {
    for name in [
        "varc-ac00-ac01.ttf",
        "varc-ac01-conditional.ttf",
        "varc-short-cff2.otf",
    ] {
        let path = paths::testdata().join("harfbuzz/test/api/fonts").join(name);
        let data = std::fs::read(path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        let varc = Varc::new(font.table(Tag::new(b"VARC")).unwrap()).unwrap();
        assert!(varc.record_count() > 0);
        for (first, last) in varc.coverage().ranges() {
            for glyph in first.0..=last.0 {
                let parts: Vec<_> = varc
                    .components(sofel_read::GlyphId(glyph))
                    .unwrap()
                    .collect();
                assert!(parts.iter().all(Option::is_some));
                for part in parts.into_iter().flatten() {
                    if let Some(index) = part.axis_values_var_index {
                        let store = varc.variation_store().unwrap();
                        let mut values = vec![0.0; part.axis_values_count];
                        assert!(store.add_delta(index, &[0], &mut values).is_some());
                        assert!(store.add_delta(index, &[16384], &mut values).is_some());
                    }
                    if let Some(index) = part.transform_var_index {
                        let store = varc.variation_store().unwrap();
                        let count = (part.flags & 0x6f70).count_ones() as usize;
                        let mut values = vec![0.0; count];
                        assert!(store.add_delta(index, &[0], &mut values).is_some());
                        assert!(store.add_delta(index, &[16384], &mut values).is_some());
                    }
                }
            }
        }
    }
}
