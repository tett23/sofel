//! Shaping plans with hand-built fonts (ADR 0020, テスト設計 計画):
//! script and language selection, feature order, mask bits (also when
//! they run out), stages and lookup order, and the global search for
//! `vert`.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

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
    let plan = ShapePlan::new(&font, props(Direction::Ltr, Some(b"Latn"), None), &[]);
    assert_eq!(plan.chosen_scripts()[0], Some(t(b"latn")));
}

#[test]
fn shaper_categorization() {
    let data = gsub_font(layout(&[(b"DFLT", vec![lang(None, &[])])], &[], &[]));
    let font = Font::new(&data, 0).unwrap();
    let shaper = |d, s: &[u8; 4]| ShapePlan::new(&font, props(d, Some(s), None), &[]).shaper();
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
    let shaper =
        |s: &[u8; 4]| ShapePlan::new(&font, props(Direction::Ltr, Some(s), None), &[]).shaper();
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
    let plan = ShapePlan::new(&font, props(Direction::Ltr, None, None), &[]);
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
    let plan = ShapePlan::new(&font, props(Direction::Ltr, None, None), &features);
    let masks = plan.feature_masks();
    let get = |t: &[u8; 4]| masks.iter().find(|m| m.0 == Tag(*t)).copied();
    // Sorted by tag now: aalt, ccmp, ..., frac, liga, rand.
    assert_eq!(get(b"aalt").unwrap().1, 0b11 << 4);
    assert_eq!(get(b"frac").unwrap().1, 1 << 6);
    assert_eq!(get(b"liga").unwrap().1, 1 << 7);
    assert_eq!(get(b"rand").unwrap().1, 0xFF << 8);
    // liga stays on everywhere except where the user range turns it off.
    assert_ne!(plan.global_mask() & (1 << 7), 0);
    let (_, key_features) = plan.key();
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
    let plan = ShapePlan::new(&font, props(Direction::Ltr, None, None), &features);
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
    let plan = ShapePlan::new(&font, props(Direction::Ttb, Some(b"Latn"), None), &[]);
    let vert = plan
        .feature_masks()
        .into_iter()
        .find(|m| m.0 == Tag(*b"vert"));
    assert_eq!(vert.map(|v| v.3), Some(true));
    assert_eq!(plan.stage_lookups(0).concat(), [(0, 1 << 31)]);
    // Horizontal text does not ask for vert.
    let plan = ShapePlan::new(&font, props(Direction::Ltr, Some(b"Latn"), None), &[]);
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
    let plan = ShapePlan::new(&font, props(Direction::Ltr, None, None), &[]);
    // The required feature applies in stage 0 with the global bit.
    assert_eq!(plan.stage_lookups(0)[0], [(0, 1 << 31)]);
}
