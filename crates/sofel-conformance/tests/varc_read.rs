//! VARC table views over HarfBuzz's fixed reference fonts (sofel ADR 0067).

#![allow(clippy::all)]
#![allow(clippy::unwrap_used)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_read::sfnt::Sfnt;
use sofel_read::varc::{self, Varc};
use sofel_read::{Kind, Tag, validate};

#[test]
fn adr0067_i3_varc_validator_reports_bad_header() {
    let mut diagnostics = Vec::new();
    varc::validate(&[0; 23], &mut |d| diagnostics.push(d));
    assert!(
        matches!(diagnostics.as_slice(), [d] if d.table == varc::TAG && matches!(d.kind, Kind::Truncated { needed: 24, available: 23 }))
    );

    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-ac00-ac01.ttf");
    let mut bytes = std::fs::read(path).unwrap();
    let original = Sfnt::new(&bytes).unwrap();
    let offset = original
        .records()
        .find(|r| r.tag == varc::TAG)
        .unwrap()
        .offset as usize;
    diagnostics.clear();
    varc::validate(original.table(varc::TAG).unwrap(), &mut |d| {
        diagnostics.push(d)
    });
    assert!(diagnostics.is_empty());
    let mut invalid = original.table(varc::TAG).unwrap().to_vec();
    invalid.get_mut(4..8).unwrap().fill(0);
    varc::validate(&invalid, &mut |d| diagnostics.push(d));
    assert!(
        matches!(diagnostics.as_slice(), [d] if d.table == varc::TAG && d.kind.id() == "invalid_varc" && matches!(d.kind, Kind::InvalidVarc))
    );
    bytes
        .get_mut(offset..offset + 4)
        .unwrap()
        .copy_from_slice(&0x0002_0000u32.to_be_bytes());
    let changed = Sfnt::new(&bytes).unwrap();
    diagnostics.clear();
    validate::sfnt(&changed, &mut |d| diagnostics.push(d));
    assert!(diagnostics.iter().any(|d| d.table == varc::TAG
        && d.offset == 0
        && matches!(d.kind, Kind::UnknownVersion { found: 0x0002_0000 })));
}

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
