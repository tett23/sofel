//! Shaping plans with hand-built fonts (ADR 0020, テスト設計 計画):
//! script and language selection, feature order, mask bits (also when
//! they run out), stages and lookup order, and the global search for
//! `vert`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::bool_assert_comparison
)]

mod support;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_shape::{
    Direction, Feature, Language, ResolvedProperties, Script, ShapePlan, Shaper, layout_selection,
};
use support::{LangSpec, lang, layout, lookup, single_subst};

fn props(
    direction: Direction,
    script: Option<&[u8; 4]>,
    language: Option<&str>,
) -> ResolvedProperties {
    ResolvedProperties {
        direction,
        script: script.map(|s| Script::from_iso15924(s).unwrap()),
        language: language.and_then(|l| Language::new(l.as_bytes())),
    }
}

fn gsub_font(gsub: Vec<u8>) -> Vec<u8> {
    support::font(10, &[(0x61, 1)], &[500], vec![(b"GSUB", gsub)])
}

#[test]
fn script_and_language_selection() {
    let gsub = layout(
        &[
            (b"DFLT", vec![lang(None, &[0])]),
            (
                b"cyrl",
                vec![
                    lang(None, &[0]),
                    lang(Some(b"SRB "), &[0]),
                    lang(Some(b"dflt"), &[0]),
                ],
            ),
            (b"latn", vec![lang(None, &[0]), lang(Some(b"TRK "), &[0])]),
        ],
        &[(b"liga", vec![0])],
        &[lookup(1, 0, vec![single_subst(&[(1, 2)])])],
    );
    let data = gsub_font(gsub);
    let font = Font::new(&data, 0).unwrap();
    let t = |s: &[u8; 4]| Tag(*s);
    // Found script and language.
    let sel = layout_selection(&font, &[t(b"latn")], &[t(b"TRK ")]);
    assert_eq!(sel.found_script[0], true);
    assert_eq!(sel.script_index[0], Some(2));
    assert_eq!(sel.chosen_script[0], Some(t(b"latn")));
    assert_eq!(sel.language_index[0], Some(0));
    // Language falls back to `dflt`, then to the default language system.
    let sel = layout_selection(&font, &[t(b"cyrl")], &[t(b"RUS ")]);
    assert_eq!(sel.language_index[0], Some(1));
    let sel = layout_selection(&font, &[t(b"latn")], &[t(b"RUS ")]);
    assert_eq!(sel.language_index[0], None);
    // Script falls back to DFLT.
    let sel = layout_selection(&font, &[t(b"grek")], &[]);
    assert_eq!(
        (
            sel.found_script[0],
            sel.script_index[0],
            sel.chosen_script[0]
        ),
        (false, Some(0), Some(t(b"DFLT")))
    );
    // No GPOS.
    assert_eq!(sel.script_index[1], None);
    assert_eq!(sel.chosen_script[1], None);

    // Without DFLT: dflt, then latn.
    let gsub = layout(
        &[(b"latn", vec![lang(None, &[0])])],
        &[(b"liga", vec![0])],
        &[lookup(1, 0, vec![single_subst(&[(1, 2)])])],
    );
    let data = gsub_font(gsub);
    let font = Font::new(&data, 0).unwrap();
    let sel = layout_selection(&font, &[t(b"thai")], &[]);
    assert_eq!(
        (sel.found_script[0], sel.chosen_script[0]),
        (false, Some(t(b"latn")))
    );
    // The plan picks the tags from the script and language.
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ltr, Some(b"Latn"), None),
        &[],
    );
    assert_eq!(plan.chosen_scripts()[0], Some(t(b"latn")));
}

#[test]
fn shaper_categorization() {
    let data = gsub_font(layout(&[(b"DFLT", vec![lang(None, &[])])], &[], &[]));
    let font = Font::new(&data, 0).unwrap();
    let shaper = |d, s: &[u8; 4]| {
        ShapePlan::new(font.default_instance(), props(d, Some(s), None), &[]).shaper()
    };
    assert_eq!(shaper(Direction::Rtl, b"Arab"), Shaper::Arabic);
    assert_eq!(shaper(Direction::Ttb, b"Arab"), Shaper::Default);
    // Syriac with only DFLT in GSUB uses the default shaper.
    assert_eq!(shaper(Direction::Rtl, b"Syrc"), Shaper::Default);
    assert_eq!(shaper(Direction::Ltr, b"Thai"), Shaper::Thai);
    assert_eq!(shaper(Direction::Ltr, b"Hang"), Shaper::Hangul);
    assert_eq!(shaper(Direction::Rtl, b"Hebr"), Shaper::Hebrew);
    assert_eq!(shaper(Direction::Ltr, b"Khmr"), Shaper::Khmer);
    assert_eq!(shaper(Direction::Ltr, b"Qaag"), Shaper::MyanmarZawgyi);
    // Indic, Myanmar and USE scripts with DFLT use the default shaper.
    assert_eq!(shaper(Direction::Ltr, b"Deva"), Shaper::Default);
    assert_eq!(shaper(Direction::Ltr, b"Mymr"), Shaper::Default);
    assert_eq!(shaper(Direction::Ltr, b"Java"), Shaper::Default);
    assert_eq!(shaper(Direction::Ltr, b"Latn"), Shaper::Default);

    let data = gsub_font(layout(
        &[
            (b"dev2", vec![lang(None, &[])]),
            (b"dev3", vec![lang(None, &[])]),
            (b"java", vec![lang(None, &[])]),
            (b"mym2", vec![lang(None, &[])]),
        ],
        &[],
        &[],
    ));
    let font = Font::new(&data, 0).unwrap();
    let shaper = |s: &[u8; 4]| {
        ShapePlan::new(
            font.default_instance(),
            props(Direction::Ltr, Some(s), None),
            &[],
        )
        .shaper()
    };
    assert_eq!(shaper(b"Deva"), Shaper::Use, "dev3 is chosen first");
    assert_eq!(shaper(b"Java"), Shaper::Use);
    assert_eq!(shaper(b"Mymr"), Shaper::Myanmar);
}

/// A font whose GSUB has the given features, each with one lookup.
fn features_font(tags: &[&[u8; 4]]) -> Vec<u8> {
    let lookups = tags
        .iter()
        .enumerate()
        .map(|(i, _)| lookup(1, 0, vec![single_subst(&[(1, 2 + i as u16)])]))
        .collect();
    gsub_font(support::simple_layout(tags, lookups))
}

#[test]
fn mask_bits_and_lookup_order() {
    let data = features_font(&[b"liga", b"ccmp", b"frac", b"aalt", b"rand"]);
    let font = Font::new(&data, 0).unwrap();
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ltr, None, None),
        &[],
    );
    let masks = plan.feature_masks();
    let get = |t: &[u8; 4]| masks.iter().find(|m| m.0 == Tag(*t)).copied();
    // Global features with value 1 use the global bit.
    assert_eq!(get(b"liga").unwrap().1, 1 << 31);
    assert_eq!(get(b"ccmp").unwrap().1, 1 << 31);
    // frac, numr and dnom get one bit each after the glyph flags; numr
    // and dnom are not in the font and have no fallback, so frac takes
    // bit 4; rand needs 8 bits.
    assert_eq!(get(b"frac").unwrap().1, 1 << 4);
    assert_eq!(get(b"rand").unwrap().1, 0xFF << 5);
    assert!(get(b"numr").is_none());
    // aalt is not asked for.
    assert!(get(b"aalt").is_none());
    // kern has a fallback, so it stays without a lookup.
    assert_eq!(get(b"kern").unwrap(), (Tag(*b"kern"), 1 << 31, 31, false));
    assert_eq!(plan.global_mask(), (1 << 31) | (0xFF << 5));
    // Stage 0 has rvrn; then the rest, by lookup index.
    let stages = plan.stage_lookups(0);
    let lookups: Vec<u16> = stages.iter().flatten().map(|l| l.0).collect();
    assert_eq!(lookups, [0, 1, 2, 4]);
    assert_eq!(stages[0], []);
    let gpos_stages = plan.stage_lookups(1);
    assert!(gpos_stages.iter().all(Vec::is_empty));

    // A user feature with a range gets its own bit; aalt=3 needs two.
    let features = [
        Feature::parse(b"liga[2:4]").unwrap(),
        Feature::parse(b"aalt=3").unwrap(),
    ];
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ltr, None, None),
        &features,
    );
    let masks = plan.feature_masks();
    let get = |t: &[u8; 4]| masks.iter().find(|m| m.0 == Tag(*t)).copied();
    // Sorted by tag now: aalt, ccmp, ..., frac, liga, rand.
    assert_eq!(get(b"aalt").unwrap().1, 0b11 << 4);
    assert_eq!(get(b"frac").unwrap().1, 1 << 6);
    assert_eq!(get(b"liga").unwrap().1, 1 << 7);
    assert_eq!(get(b"rand").unwrap().1, 0xFF << 8);
    // liga stays on everywhere except where the user range turns it off.
    assert_ne!(plan.global_mask() & (1 << 7), 0);
    let (_, key_features, _) = plan.key();
    assert_eq!(key_features, features);
}

#[test]
fn bits_run_out() {
    // 30 features of value 255 need 8 bits each; 31 - 4 bits are free.
    let tags: Vec<[u8; 4]> = (0..30u8)
        .map(|i| [b'x', b'a' + i / 10, b'a' + i % 10, b'x'])
        .collect();
    let refs: Vec<&[u8; 4]> = tags.iter().collect();
    let data = features_font(&refs);
    let font = Font::new(&data, 0).unwrap();
    let features: Vec<Feature> = tags.iter().map(|t| Feature::new(Tag(*t), 255)).collect();
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ltr, None, None),
        &features,
    );
    let masks = plan.feature_masks();
    let mine: Vec<_> = masks.iter().filter(|m| m.0.0[0] == b'x').collect();
    // frac and rand are not in the font and are left without bits; three
    // 8-bit features fit at bits 4, 12 and 20, and the rest are dropped.
    assert_eq!(mine.len(), 3, "{masks:x?}");
    assert_eq!(mine[0].1, 0xFF << 4);
    assert_eq!(mine[1].1, 0xFF << 12);
    assert_eq!(mine[2].1, 0xFF << 20);
    assert_eq!(plan.stage_lookups(0).concat().len(), 3);
}

#[test]
fn vert_is_searched_globally() {
    let gsub = layout(
        &[
            (b"DFLT", vec![lang(None, &[])]),
            (
                b"kana",
                vec![LangSpec {
                    tag: None,
                    required: None,
                    features: vec![0],
                }],
            ),
        ],
        &[(b"vert", vec![0])],
        &[lookup(1, 0, vec![single_subst(&[(1, 2)])])],
    );
    let data = gsub_font(gsub);
    let font = Font::new(&data, 0).unwrap();
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ttb, Some(b"Latn"), None),
        &[],
    );
    let vert = plan
        .feature_masks()
        .into_iter()
        .find(|m| m.0 == Tag(*b"vert"));
    assert_eq!(vert.map(|v| v.3), Some(true));
    assert_eq!(plan.stage_lookups(0).concat(), [(0, 1 << 31)]);
    // Horizontal text does not ask for vert.
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ltr, Some(b"Latn"), None),
        &[],
    );
    assert!(plan.stage_lookups(0).concat().is_empty());
}

#[test]
fn required_feature_goes_first() {
    let gsub = layout(
        &[(
            b"DFLT",
            vec![LangSpec {
                tag: None,
                required: Some(1),
                features: vec![0],
            }],
        )],
        &[(b"liga", vec![1]), (b"xxxx", vec![0])],
        &[
            lookup(1, 0, vec![single_subst(&[(1, 2)])]),
            lookup(1, 0, vec![single_subst(&[(2, 3)])]),
        ],
    );
    let data = gsub_font(gsub);
    let font = Font::new(&data, 0).unwrap();
    let plan = ShapePlan::new(
        font.default_instance(),
        props(Direction::Ltr, None, None),
        &[],
    );
    // The required feature applies in stage 0 with the global bit.
    assert_eq!(plan.stage_lookups(0)[0], [(0, 1 << 31)]);
}

#[test]
fn adr0027_i_plan_key_tracks_selected_record_not_exact_coords() {
    use sofel_font::var::{Location, Variation};

    let mut gsub = layout(
        &[(b"latn", vec![lang(None, &[0])])],
        &[(b"liga", vec![0])],
        &[lookup(1, 0, vec![single_subst(&[(1, 2)])])],
    );
    let old_len = gsub.len();
    let script = u16::from_be_bytes([gsub[4], gsub[5]]) + 4;
    let feature = u16::from_be_bytes([gsub[6], gsub[7]]) + 4;
    let lookup = u16::from_be_bytes([gsub[8], gsub[9]]) + 4;
    gsub[0..4].copy_from_slice(&0x0001_0001u32.to_be_bytes());
    gsub[4..6].copy_from_slice(&script.to_be_bytes());
    gsub[6..8].copy_from_slice(&feature.to_be_bytes());
    gsub[8..10].copy_from_slice(&lookup.to_be_bytes());
    gsub.splice(10..10, (old_len as u32 + 4).to_be_bytes());
    // One record; an axis-zero inclusive range [0.25, 1.0].
    gsub.extend_from_slice(&[
        0, 1, 0, 0, 0, 0, 0, 1, // FeatureVariations header
        0, 0, 0, 16, 0, 0, 0, 0, // condition and null substitution offsets
        0, 1, 0, 0, 0, 6, // one condition
        0, 1, 0, 0, 0x10, 0, 0x40, 0, // format 1, axis 0
    ]);
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let data = support::font(
        10,
        &[(0x61, 1)],
        &[500],
        vec![(b"GSUB", gsub), (b"fvar", fvar)],
    );
    let font = Font::new(&data, 0).unwrap();
    let key = |v: f32| {
        let loc = Location::new(
            &font,
            None,
            &[Variation::parse(&format!("wght={v}")).unwrap()],
        );
        ShapePlan::new(
            font.instance(&loc),
            props(Direction::Ltr, Some(b"Latn"), None),
            &[],
        )
        .key()
        .2
    };
    assert_eq!(key(0.0)[0].index, None);
    assert_eq!(key(0.5)[0].index, Some(0));
    assert_eq!(key(0.75)[0].index, Some(0));
    assert_ne!(key(0.0), key(0.5));
}

#[test]
fn adr0046_i2_i4_plan_key_and_lookups_follow_condition_state() {
    use sofel_font::var::{Location, Variation};

    let mut gsub = layout(
        &[(b"latn", vec![lang(None, &[0])])],
        &[(b"liga", vec![0])],
        &[
            lookup(1, 0, vec![single_subst(&[(1, 2)])]),
            lookup(1, 0, vec![single_subst(&[(1, 3)])]),
        ],
    );
    let old_len = gsub.len();
    for at in [4usize, 6, 8] {
        let offset = u16::from_be_bytes([gsub[at], gsub[at + 1]]) + 4;
        gsub[at..at + 2].copy_from_slice(&offset.to_be_bytes());
    }
    gsub[0..4].copy_from_slice(&0x0001_0001u32.to_be_bytes());
    gsub.splice(10..10, (old_len as u32 + 4).to_be_bytes());
    gsub.extend_from_slice(&[
        0, 1, 0, 1, 0, 0, 0, 0, // FeatureVariations 1.1, no standard records
        0, 0, 0, 1, 0, 0, 0, 0, 0, 18, // one lookup variation record, feature 0
        0, 1, 0, 0, 0, 0, 0, 0, 0, 2, // FeatureLookupsTable, flags 0, two conditions
        0, 0, 0, 26, 0, 0, 0, 34, // first condition and lookup list offsets
        0, 0, 0, 38, 0, 0, 0, 46, // second condition and lookup list offsets
        0, 1, 0, 0, 0x10, 0, 0x30, 0, // axis 0 in [0.25, 0.75]
        0, 1, 0, 1, // lookup index 1
        0, 1, 0, 0, 0x20, 0, 0x40, 0, // axis 0 in [0.5, 1]
        0, 1, 0, 1, // same lookup despite a distinct condition state
    ]);
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let data = support::font(
        10,
        &[(0x61, 1)],
        &[500],
        vec![(b"GSUB", gsub), (b"fvar", fvar)],
    );
    let font = Font::new(&data, 0).unwrap();
    let plan = |value: &str| {
        let loc = Location::new(&font, None, &[Variation::parse(value).unwrap()]);
        ShapePlan::new(
            font.instance(&loc),
            props(Direction::Ltr, Some(b"Latn"), None),
            &[],
        )
    };
    let zero = plan("wght=0");
    let middle = plan("wght=0.3");
    let nearby = plan("wght=0.4");
    let later = plan("wght=0.6");
    let last = plan("wght=0.8");
    assert!(zero.key().2[0].lookup_conditions.is_empty());
    assert_eq!(middle.key().2[0].lookup_conditions, vec![44]);
    assert_eq!(middle.key().2, nearby.key().2);
    assert_ne!(zero.key().2, middle.key().2);
    assert_eq!(later.key().2[0].lookup_conditions, vec![44, 56]);
    assert_eq!(last.key().2[0].lookup_conditions, vec![56]);
    assert_ne!(middle.key().2, later.key().2);
    assert_ne!(later.key().2, last.key().2);
    assert_eq!(middle.stage_lookups(0), later.stage_lookups(0));
    assert!(zero.stage_lookups(0).concat().is_empty());
    assert_eq!(
        middle
            .stage_lookups(0)
            .concat()
            .iter()
            .map(|(i, _)| *i)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn i1_aat_complex_shaper_uses_dumber_only_when_morx_is_selected() {
    // One valid chain with default flags and no substitutions.
    let morx: Vec<_> = [0x0002_0000u32, 1, 0, 16, 0, 0]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect();
    let data = support::font(
        10,
        &[(0x61, 1)],
        &[500],
        vec![
            (b"morx", morx),
            (
                b"GSUB",
                layout(&[(b"DFLT", vec![lang(None, &[])])], &[], &[]),
            ),
        ],
    );
    let font = Font::new(&data, 0).unwrap();
    let selected =
        |d, s| ShapePlan::new(font.default_instance(), props(d, Some(s), None), &[]).shaper();
    assert_eq!(selected(Direction::Rtl, b"Arab"), Shaper::Dumber);
    assert_eq!(selected(Direction::Ltr, b"Latn"), Shaper::Default);
    assert_eq!(selected(Direction::Ttb, b"Arab"), Shaper::Default);
}

#[test]
fn adr0060_i2_morx_checks_coverage_and_counts_subtables_across_chains() {
    let selected = |version: u16, counts: &[u32], coverage: &[u8]| {
        let mut table = Vec::from(version.to_be_bytes());
        table.extend_from_slice(&[0; 2]);
        table.extend_from_slice(&(counts.len() as u32).to_be_bytes());
        for &n in counts {
            let mut chain = vec![0; 16];
            chain[12..16].copy_from_slice(&n.to_be_bytes());
            for _ in 0..n {
                // Type 3 is reserved and has only the extended header.
                chain.extend_from_slice(&[0, 0, 0, 12, 0, 0, 0, 3, 0, 0, 0, 0]);
            }
            chain.extend_from_slice(coverage);
            let len = chain.len() as u32;
            chain[4..8].copy_from_slice(&len.to_be_bytes());
            table.extend_from_slice(&chain);
        }
        let bytes = support::font(10, &[(0x61, 1)], &[500], vec![(b"morx", table)]);
        let font = Font::new(&bytes, 0).unwrap();
        ShapePlan::new(
            font.default_instance(),
            props(Direction::Rtl, Some(b"Arab"), None),
            &[],
        )
        .shaper()
            == Shaper::Dumber
    };
    assert!(selected(2, &[512, 512], &[]));
    assert!(!selected(2, &[512, 513], &[]));
    for offset in [0u32, u32::MAX] {
        assert!(selected(3, &[1], &offset.to_be_bytes()));
    }
    assert!(selected(3, &[1], &[0, 0, 0, 4, 0, 0]));
    assert!(!selected(3, &[1], &[0, 0, 0, 4, 0]));
    assert!(!selected(3, &[1], &[0, 0, 0]));
}
