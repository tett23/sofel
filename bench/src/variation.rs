//! Non-default font locations in sofel and both reference libraries (ADR 0027).

use super::{Bench, Compare, FtFace, HbFont};
use sofel_font::{
    Font, GlyphScratch,
    read::{GlyphId, Tag},
    var::{AdvanceCache, Instance, Location, Variation},
};
use std::os::raw::c_long;
use std::path::Path;

const FONTS: [(&str, &str); 3] = [
    ("RobotoFlex-Variable", "RobotoFlex-Variable.ttf"),
    ("SourceSerifVariable-Roman", "SourceSerifVariable-Roman.ttf"),
    ("AdobeVFPrototype", "AdobeVFPrototype.otf"),
];

pub(super) fn items(bench: &mut Bench, data: &Path) {
    let weight = Variation {
        tag: Tag::new(b"wght"),
        value: 500.0,
    };
    let warm = "location with reference queries wght=499/500";
    let item = "location wght=500";
    for (name, file) in FONTS {
        if ![
            item,
            warm,
            "advance wght=500",
            "box wght=500",
            "outline unscaled wght=500",
            "outline 48px wght=500",
            "raster 24px wght=500",
            "ADR0049 advance",
        ]
        .iter()
        .any(|item| bench.wanted(name, item))
        {
            continue;
        }
        let bytes = std::fs::read(data.join("harfbuzz/test/subset/data/fonts").join(file)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let ft = FtFace::open(&bytes).unwrap();
        let hb = HbFont::open(&bytes).unwrap();
        let make_location = || Location::new(&font, None, &[weight]);
        let location = make_location();
        let mut design: Vec<c_long> = location
            .ft_design_coords()
            .iter()
            .map(|v| c_long::from(v.0))
            .collect();
        assert!(ft.set_design(&mut design));
        hb.set_weight(weight.value);
        let mut normalized = vec![0; design.len()];
        assert!(ft.normalized(&mut normalized));
        assert_eq!(
            normalized,
            location
                .ft_coords()
                .iter()
                .map(|v| c_long::from(v.0))
                .collect::<Vec<_>>(),
            "{name} FreeType normalized coordinates"
        );
        assert_eq!(
            hb.normalized(),
            location.coords_normalized(),
            "{name} HarfBuzz normalized coordinates"
        );
        bench.item(
            name,
            item.into(),
            &mut || u64::from(std::hint::black_box(make_location()).ft_error().is_none()),
            Some((&mut || u64::from(ft.set_design(&mut design)), Compare::Yes)),
            Some((
                &mut || {
                    hb.set_weight(weight.value);
                    1
                },
                Compare::Yes,
            )),
        );
        let instance = font.instance(&location);
        if bench.wanted(name, "box wght=500") {
            let boxes = sofel_outline::HbExtents::new(instance);
            bench.item(
                name,
                "box wght=500".into(),
                &mut || {
                    (0..font.num_glyphs())
                        .map(|g| super::box_sum(boxes.glyph_extents(sofel_font::read::GlyphId(g))))
                        .fold(0u64, u64::wrapping_add)
                },
                None,
                Some((
                    &mut || {
                        (0..font.num_glyphs())
                            .map(|g| {
                                hb.glyph_extents(g)
                                    .map_or(super::wrap(i64::MIN), super::wrap)
                            })
                            .fold(0u64, u64::wrapping_add)
                    },
                    Compare::Yes,
                )),
            );
        }
        if bench.wanted(name, warm) {
            let buffer = super::ffi::HbBuffer::new();
            let gid = hb.nominal_glyph(u32::from('A'));
            let alternate = Variation {
                value: 499.0,
                ..weight
            };
            let mut pair = [Location::new(&font, None, &[alternate]), make_location()].map(|loc| {
                loc.ft_design_coords()
                    .iter()
                    .map(|v| c_long::from(v.0))
                    .collect::<Vec<_>>()
            });
            // Each invocation changes coordinates, including across timing samples.
            let (mut ours_index, mut ft_index, mut hb_index) = (0, 0, 0);
            let weights = [alternate, weight];
            bench.item(
                name,
                warm.into(),
                &mut || {
                    let loc = Location::new(&font, None, &[weights[ours_index]]);
                    ours_index ^= 1;
                    u64::from(font.instance(&loc).advance(sofel_font::read::GlyphId(gid)))
                },
                Some((
                    &mut || {
                        assert!(ft.set_design(&mut pair[ft_index]));
                        ft_index ^= 1;
                        std::hint::black_box(ft.outline(gid, true));
                        ft.advance(gid) as u64
                    },
                    Compare::Yes,
                )),
                Some((
                    &mut || {
                        hb.set_weight(weights[hb_index].value);
                        hb_index ^= 1;
                        hb.query_variable_metrics();
                        std::hint::black_box(hb.glyph_extents(gid));
                        std::hint::black_box(buffer.shape_text(&hb, b"AV\n", 4));
                        hb.advance(gid) as u64
                    },
                    Compare::Yes,
                )),
            );
        }
        if bench.wanted(name, "advance wght=500") {
            let ours = |g| u64::from(instance.advance(sofel_font::read::GlyphId(g)));
            let theirs_ft = |g| ft.advance(g) as u64;
            let theirs_hb = |g| hb.advance(g) as u64;
            for g in 0..font.num_glyphs() {
                assert_eq!(ours(g), theirs_hb(g), "{name} glyph {g}, HarfBuzz advance");
                assert_eq!(ours(g), theirs_ft(g), "{name} glyph {g}, FreeType advance");
            }
            bench.item(
                name,
                "advance wght=500".into(),
                &mut || (0..font.num_glyphs()).map(ours).sum(),
                Some((
                    &mut || (0..font.num_glyphs()).map(theirs_ft).sum(),
                    Compare::Yes,
                )),
                Some((
                    &mut || (0..font.num_glyphs()).map(theirs_hb).sum(),
                    Compare::Yes,
                )),
            );
        }
        super::geometry_items(bench, name, instance, &ft, &hb, " wght=500", &[24]);
        if bench.adr0049_enabled() {
            let hb_cache = HbFont::open(&bytes).unwrap();
            hb_cache.set_weight(weight.value);
            advance_cache_items(bench, name, &font, instance, &hb_cache, "wght=500");
            let hb_default = HbFont::open(&bytes).unwrap();
            advance_cache_items(
                bench,
                name,
                &font,
                font.default_instance(),
                &hb_default,
                "default",
            );
        }
    }
}

fn advance_cache_items(
    bench: &mut Bench,
    name: &str,
    font: &Font<'_>,
    instance: Instance<'_, '_, '_, '_>,
    hb: &HbFont,
    location: &str,
) {
    let n = font.num_glyphs();
    let repeated = n.saturating_sub(1);
    for (pattern, ids) in [
        ("sequential", (0..n).collect::<Vec<_>>()),
        ("repeated", vec![repeated; usize::try_from(n).unwrap()]),
    ] {
        let reference = || {
            ids.iter()
                .map(|&glyph| u64::try_from(hb.advance(glyph)).unwrap())
                .fold(0u64, u64::wrapping_add)
        };
        bench.item(
            name,
            format!("ADR0049 advance {pattern} uncached {location}"),
            &mut || {
                let mut scratch = GlyphScratch::default();
                ids.iter()
                    .map(|&glyph| {
                        u64::from(instance.advance_with_scratch(GlyphId(glyph), &mut scratch))
                    })
                    .fold(0u64, u64::wrapping_add)
            },
            None,
            Some((&mut || reference(), Compare::Yes)),
        );
        bench.item(
            name,
            format!("ADR0049 advance {pattern} cached {location}"),
            &mut || {
                let cache = AdvanceCache::new(instance);
                let mut scratch = GlyphScratch::default();
                ids.iter()
                    .map(|&glyph| {
                        u64::from(cache.advance_with_scratch(GlyphId(glyph), &mut scratch))
                    })
                    .fold(0u64, u64::wrapping_add)
            },
            None,
            Some((&mut || reference(), Compare::Yes)),
        );
    }
    bench.item(
        name,
        format!("ADR0049 advance cache initialization {location}"),
        &mut || {
            let mut bytes = 0;
            for _ in 0..1000 {
                let cache = std::hint::black_box(Box::new(AdvanceCache::new(instance)));
                bytes += core::mem::size_of_val(&*cache) as u64;
            }
            bytes
        },
        None,
        None,
    );
}
