//! FreeType slot routing for the public outline loaders (root ADR 0064).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use sofel_conformance::paths;
use sofel_font::{
    Font,
    read::{GlyphId, Tag, varc::Varc},
};
use sofel_outline::{AutoHintCache, HintMode, HintTarget, Outlines, Scratch};

#[test]
fn adr0064_i1_varc_public_loaders_share_slot_semantics() {
    for file in [
        "varc-static-gvar.ttf",
        "varc-short-cff2.otf",
        "varc-static-cff2.otf",
    ] {
        let bytes =
            std::fs::read(paths::testdata().join("freetype/tests/data").join(file)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let varc = Varc::new(font.table(Tag::new(b"VARC")).unwrap()).unwrap();
        let outlines = Outlines::new(font.default_instance());
        for glyph in (0..font.num_glyphs())
            .map(GlyphId)
            .filter(|&glyph| varc.coverage().get(glyph).is_some())
        {
            for ppem in [5, 16, 36] {
                for mode in [HintMode::NativeV35, HintMode::NativeV40, HintMode::Auto] {
                    let mut expected = Scratch::default();
                    let (info, advance) = outlines
                        .load_with_hint(glyph, ppem, mode, HintTarget::Normal, &mut expected)
                        .unwrap();
                    let mut actual = Scratch::default();
                    let loaded = match mode {
                        HintMode::NativeV35 => outlines.load_truetype_v35_with_advance(
                            glyph,
                            ppem,
                            HintTarget::Normal,
                            &mut actual,
                        ),
                        HintMode::NativeV40 => {
                            outlines.load_truetype_v40(glyph, ppem, HintTarget::Normal, &mut actual)
                        }
                        _ => outlines.load_auto_hinted(
                            glyph,
                            ppem,
                            HintTarget::Normal,
                            &mut AutoHintCache::new(&outlines),
                            &mut actual,
                        ),
                    }
                    .unwrap();
                    assert_eq!(loaded.1, advance, "{file} {glyph:?}");
                    assert_eq!(loaded.0.cbox, info.cbox);
                    assert_eq!(actual.points(), expected.points());
                    assert_eq!(actual.point_tags(), expected.point_tags());
                    assert_eq!(actual.contour_ends(), expected.contour_ends());
                }
            }
        }
    }
}
