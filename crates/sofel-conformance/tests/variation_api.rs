//! Fixed expectations from HarfBuzz test-var-coords.c and test-ot-metrics-tt-var.c.
#![allow(clippy::unwrap_used)]

use sofel_conformance::paths;
use sofel_font::{
    Font,
    read::{GlyphId, Tag},
    var::Location,
};
use sofel_outline::HbExtents;

fn api_font(file: &str) -> Vec<u8> {
    std::fs::read(paths::testdata().join("harfbuzz/test/api/fonts").join(file)).unwrap()
}

#[test]
fn adr0027_var_coords_and_axis_infos() {
    let bytes = api_font("TestCFF2VF.otf");
    let font = Font::new(&bytes, 0).unwrap();
    let normalized = Location::from_normalized_2_14(&font, &[100, 0]);
    assert!(normalized.coords_design().first().unwrap().is_nan());
    assert_eq!(normalized.coords_normalized().first(), Some(&100));
    let design = Location::from_design(&font, &[206., 0.]);
    assert_eq!(design.coords_normalized().first(), Some(&-16117));
    assert_eq!(design.coords_design().first(), Some(&206.));

    let bytes = api_font("Estedad-VF.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        font.var_axes().map(|axis| axis.tag).collect::<Vec<_>>(),
        [Tag::new(b"wght"), Tag::new(b"wdth")]
    );
}

#[test]
fn adr0027_variable_truetype_extents_and_vertical_origin() {
    for (file, coords, glyph, expected) in [
        (
            "SourceSansVariable-Roman-nohvar-41,C1.ttf",
            None,
            2,
            [10, 846, 500, -846],
        ),
        (
            "SourceSansVariable-Roman-nohvar-41,C1.ttf",
            Some(500.),
            2,
            [0, 874, 551, -874],
        ),
        (
            "SourceSansVariable-Roman.anchor.ttf",
            None,
            2,
            [56, 672, 556, -684],
        ),
        (
            "SourceSansVariable-Roman.anchor.ttf",
            Some(500.),
            2,
            [50, 667, 592, -679],
        ),
        (
            "SourceSansVariable-Roman.modcomp.ttf",
            Some(800.),
            2,
            [19, 663, 519, -895],
        ),
        (
            "SourceSansVariable-Roman.modcomp.ttf",
            Some(800.),
            3,
            [19, 909, 519, -921],
        ),
        (
            "SourceSansVariable-Roman.modcomp.ttf",
            Some(800.),
            4,
            [19, 866, 519, -878],
        ),
    ] {
        let bytes = api_font(file);
        let font = Font::new(&bytes, 0).unwrap();
        let loc = Location::from_design(&font, coords.as_slice());
        let instance = font.instance(&loc);
        let boxes = HbExtents::new(instance);
        let ext = boxes.glyph_extents(GlyphId(glyph)).unwrap();
        assert_eq!(
            [ext.x_bearing, ext.y_bearing, ext.width, ext.height],
            expected
        );
    }
    let bytes = api_font("SourceSansVariable-Roman.modcomp.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    let loc = Location::from_design(&font, &[800.]);
    let instance = font.instance(&loc);
    let boxes = HbExtents::new(instance);
    assert_eq!(instance.v_advance(GlyphId(2)), -991);
    assert_eq!(
        instance.v_origin(GlyphId(2), |gid| boxes.glyph_extents(gid)),
        (292, 1013)
    );
}

#[test]
fn adr0027_variable_truetype_advances() {
    for (file, glyph, weight, base, varied) in [
        (
            "SourceSansVariable-Roman-nohvar-41,C1.ttf",
            2,
            500.,
            (520, -1257),
            (551, -1257),
        ),
        (
            "SourceSerifVariable-Roman-VVAR.abc.ttf",
            1,
            700.,
            (508, -1000),
            (531, -1012),
        ),
    ] {
        let bytes = api_font(file);
        let font = Font::new(&bytes, 0).unwrap();
        let loc = Location::from_design(&font, &[weight]);
        for (instance, pair) in [
            (font.default_instance(), base),
            (font.instance(&loc), varied),
        ] {
            assert_eq!(
                (
                    instance.advance(GlyphId(glyph)),
                    instance.v_advance(GlyphId(glyph))
                ),
                pair
            );
        }
    }
}

#[test]
fn adr0027_cff2_extents_at_design_and_named_locations() {
    let bytes = api_font("AdobeVFPrototype_vsindex.otf");
    let font = Font::new(&bytes, 0).unwrap();
    for (location, expected) in [
        (
            Location::from_design(&font, &[800., 50.]),
            [[12, 655, 651, -655], [8, 669, 648, -669]],
        ),
        (
            Location::named_instance(&font, 6),
            [[13, 652, 652, -652], [6, 675, 647, -675]],
        ),
    ] {
        let boxes = HbExtents::new(font.instance(&location));
        for (gid, expected) in [1, 2].into_iter().zip(expected) {
            let ext = boxes.glyph_extents(GlyphId(gid)).unwrap();
            assert_eq!(
                [ext.x_bearing, ext.y_bearing, ext.width, ext.height],
                expected
            );
        }
    }
    let bytes = api_font("cff2-private-blend-groups.otf");
    let font = Font::new(&bytes, 0).unwrap();
    for coordinate in [0., 0.5] {
        let loc = Location::from_design(&font, &[coordinate]);
        let ext = HbExtents::new(font.instance(&loc))
            .glyph_extents(GlyphId(1))
            .unwrap();
        assert_eq!(
            [ext.x_bearing, ext.y_bearing, ext.width, ext.height],
            [10, 110, 40, -90]
        );
    }
}

#[test]
fn adr0027_composites_take_last_component_metrics() {
    let bytes = api_font("a80ae54f8ad6577653d6bd370d6a81b37bdd7e79.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    let loc = Location::from_design(&font, &[900.]);
    for (instance, expected) in [
        (font.default_instance(), [700, 300, 500, 500, 500, 800]),
        (font.instance(&loc), [700, 300, 300, 700, 700, 800]),
    ] {
        for (gid, width) in (1..=6).zip(expected) {
            assert_eq!(instance.advance(GlyphId(gid)), width);
        }
    }
}

#[test]
fn adr0027_axes_without_named_instances_and_inferred_gvar_extents() {
    let bytes = api_font("Zycon.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    assert!(font.var_axes().next().is_some());
    assert_eq!(font.named_instances().count(), 0);

    let bytes = api_font("TestGVAREight.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_2_14(&font, &[100, 0, 0, 0, 0, 0]);
    assert!(
        HbExtents::new(font.instance(&location))
            .glyph_extents(GlyphId(4))
            .is_some()
    );
}

#[test]
fn adr0027_hvar_long_deltas_saturate_at_axis_maximum() {
    let bytes = api_font("hvar-long-deltas.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[1.]);
    assert_eq!(font.default_instance().advance(GlyphId(1)), 500);
    assert_eq!(font.instance(&location).advance(GlyphId(1)), u32::MAX);
}

#[test]
fn adr0027_cff2_default_and_design_extents() {
    let bytes = api_font("AdobeVFPrototype.abc.otf");
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[600., 50.]);
    for (instance, expected) in [
        (font.default_instance(), [46, 487, 455, -500]),
        (font.instance(&location), [38, 493, 480, -507]),
    ] {
        let ext = HbExtents::new(instance).glyph_extents(GlyphId(1)).unwrap();
        assert_eq!(
            [ext.x_bearing, ext.y_bearing, ext.width, ext.height],
            expected
        );
    }
}

#[test]
fn adr0027_baselines_match_harfbuzz_api_fixtures() {
    use sofel_font::Direction::{Horizontal, Vertical};

    for (file, tag, direction, script, expected) in [
        ("base.ttf", b"icfb", Vertical, b"hani", 46),
        ("base.ttf", b"idtp", Vertical, b"hani", 1000),
        ("base.ttf", b"math", Horizontal, b"hani", 271),
        ("base2.ttf", b"hang", Horizontal, b"beng", 622),
        ("base2.ttf", b"hang", Horizontal, b"bng2", 622),
        ("base2.ttf", b"hang", Horizontal, b"tibt", 600),
    ] {
        let data = api_font(file);
        let font = Font::new(&data, 0).unwrap();
        let instance = font.default_instance();
        let boxes = HbExtents::new(instance);
        let position =
            instance.baseline_with_fallback(Tag::new(tag), direction, Tag::new(script), |glyph| {
                boxes.glyph_extents(glyph)
            });
        assert_eq!(position, expected, "{file} {tag:?} {script:?}");
    }
}
