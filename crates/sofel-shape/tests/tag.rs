//! Every example of HarfBuzz 15.1.0's `test/api/test-ot-tag.c`
//! (ADR 0020, テスト設計「タグ」), in the same order.

#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_font::read::Tag;
use sofel_shape::{
    Language, MAX_TAGS_PER_LANGUAGE, MAX_TAGS_PER_SCRIPT, OT_TAG_DEFAULT_LANGUAGE,
    OT_TAG_DEFAULT_SCRIPT, Script, ot_tag_to_language, ot_tag_to_script,
    ot_tags_from_script_and_language, ot_tags_from_script_and_language_max,
    ot_tags_to_script_and_language,
};

/// `hb_tag_from_string (s, -1)`: padded with spaces, 0 for "".
fn tag(s: &str) -> Tag {
    if s.is_empty() {
        return Tag::from_u32(0);
    }
    let mut b = [b' '; 4];
    s.bytes().take(4).enumerate().for_each(|(i, c)| b[i] = c);
    Tag(b)
}

/// `hb_script_from_string`; `None` is `HB_SCRIPT_INVALID`.
fn sc(s: &str) -> Option<Script> {
    Script::from_iso15924(s.as_bytes())
}

/// `hb_language_from_string (s, -1)`; `None` is `HB_LANGUAGE_INVALID`.
fn lang(s: Option<&str>) -> Option<Language> {
    s.and_then(|s| Language::new(s.as_bytes()))
}

fn test_simple_tags(s: &str, script: Option<Script>) {
    let t = tag(s);
    let (tags, _) = ot_tags_from_script_and_language_max(script, None, 2, 0);
    match tags.first() {
        Some(&first) => assert_eq!(first, t, "{s}"),
        None => assert_eq!(Tag(*b"DFLT"), t, "{s}"),
    }
    assert_eq!(ot_tag_to_script(t), script, "{s}");
}

fn test_script_tags_from_language(s: Option<&str>, lang_s: Option<&str>, script: Option<Script>) {
    let (tags, _) = ot_tags_from_script_and_language_max(script, lang(lang_s).as_ref(), 1, 0);
    if !tags.is_empty() {
        assert_eq!(tags.len(), 1, "{lang_s:?}");
        assert_eq!(Some(tags[0]), s.map(tag), "{lang_s:?}");
    }
}

fn test_indic_tags(s1: &str, s2: &str, s3: &str, script: Option<Script>) {
    let (tags, _) = ot_tags_from_script_and_language_max(script, None, 3, 0);
    assert_eq!(tags, [tag(s1), tag(s2), tag(s3)]);
    for s in [s1, s2, s3] {
        assert_eq!(ot_tag_to_script(tag(s)), script, "{s}");
    }
}

#[test]
fn ot_tag_script_degenerate() {
    assert_eq!(OT_TAG_DEFAULT_SCRIPT, Tag(*b"DFLT"));

    // Hiragana, Katakana, and their collective ISO 15924 script all map to 'kana'.
    test_simple_tags("kana", sc("Kana"));

    let (t, _) = ot_tags_from_script_and_language_max(sc("Hira"), None, 2, 0);
    assert_eq!(t, [tag("kana")]);

    let (t, _) = ot_tags_from_script_and_language_max(sc("Hrkt"), None, 2, 0);
    assert_eq!(t, [tag("kana")]);
    assert_eq!(ot_tag_to_script(tag("kana")), sc("Kana"));

    test_simple_tags("DFLT", None);

    // Spaces are replaced.
    assert_eq!(ot_tag_to_script(tag("be  ")), sc("Beee"));
}

#[test]
fn ot_tag_script_simple() {
    // Arbitrary non-existent script.
    test_simple_tags("wwyz", sc("wWyZ"));

    test_simple_tags("zyyy", Some(Script::COMMON));
    test_simple_tags("zinh", Some(Script::INHERITED));
    test_simple_tags("zzzz", Some(Script::UNKNOWN));

    test_simple_tags("arab", sc("Arab"));
    test_simple_tags("copt", sc("Copt"));
    test_simple_tags("kana", sc("Kana"));
    test_simple_tags("latn", sc("Latn"));

    test_simple_tags("math", sc("Zmth"));

    // These are trickier since their OT script tags have space.
    test_simple_tags("lao ", sc("Laoo"));
    test_simple_tags("yi  ", sc("Yiii"));
    test_simple_tags("nko ", sc("Nkoo"));
    test_simple_tags("vai ", sc("Vaii"));

    test_simple_tags("mtei", sc("Mtei"));
    test_simple_tags("mand", sc("Mand"));
}

#[test]
fn ot_tag_script_from_language() {
    let copt = sc("Copt");
    test_script_tags_from_language(None, None, None);
    test_script_tags_from_language(None, Some("en"), None);
    test_script_tags_from_language(Some("copt"), Some("en"), copt);
    test_script_tags_from_language(None, Some("x-hbsc"), None);
    test_script_tags_from_language(Some("copt"), Some("x-hbsc"), copt);
    test_script_tags_from_language(None, Some("x-hbsc-"), None);
    test_script_tags_from_language(None, Some("x-hbsc-1"), None);
    test_script_tags_from_language(None, Some("x-hbsc-1a"), None);
    test_script_tags_from_language(None, Some("x-hbsc-1a2b3c4x"), None);
    test_script_tags_from_language(Some("2lon"), Some("x-hbsc-326c6f6e67"), None);
    test_script_tags_from_language(Some("abc "), Some("x-hbscabc"), None);
    test_script_tags_from_language(Some("deva"), Some("x-hbscdeva"), None);
    test_script_tags_from_language(Some("dev2"), Some("x-hbscdev2"), None);
    test_script_tags_from_language(Some("dev3"), Some("x-hbscdev3"), None);
    test_script_tags_from_language(Some("dev3"), Some("x-hbsc-64657633"), None);
    test_script_tags_from_language(Some("copt"), Some("x-hbotpap0-hbsccopt"), None);
    test_script_tags_from_language(None, Some("en-x-hbsc"), None);
    test_script_tags_from_language(Some("copt"), Some("en-x-hbsc"), copt);
    test_script_tags_from_language(Some("abc "), Some("en-x-hbscabc"), None);
    test_script_tags_from_language(Some("deva"), Some("en-x-hbscdeva"), None);
    test_script_tags_from_language(Some("dev2"), Some("en-x-hbscdev2"), None);
    test_script_tags_from_language(Some("dev3"), Some("en-x-hbscdev3"), None);
    test_script_tags_from_language(Some("dev3"), Some("en-x-hbsc-64657633"), None);
    test_script_tags_from_language(Some("copt"), Some("en-x-hbotpap0-hbsccopt"), None);
}

#[test]
fn ot_tag_script_indic() {
    test_indic_tags("bng3", "bng2", "beng", sc("Beng"));
    test_indic_tags("dev3", "dev2", "deva", sc("Deva"));
    test_indic_tags("gjr3", "gjr2", "gujr", sc("Gujr"));
    test_indic_tags("gur3", "gur2", "guru", sc("Guru"));
    test_indic_tags("knd3", "knd2", "knda", sc("Knda"));
    test_indic_tags("mlm3", "mlm2", "mlym", sc("Mlym"));
    test_indic_tags("ory3", "ory2", "orya", sc("Orya"));
    test_indic_tags("tml3", "tml2", "taml", sc("Taml"));
    test_indic_tags("tel3", "tel2", "telu", sc("Telu"));
}

fn test_tag_from_language(tag_s: &str, lang_s: Option<&str>) {
    let (_, tags) = ot_tags_from_script_and_language_max(None, lang(lang_s).as_ref(), 0, 1);
    match tags.first() {
        Some(&t) => assert_eq!(t, tag(tag_s), "{lang_s:?}"),
        None => assert_eq!(tag(tag_s), Tag(*b"dflt"), "{lang_s:?}"),
    }
}

fn test_tag_to_language(tag_s: &str, lang_s: Option<&str>) {
    assert_eq!(ot_tag_to_language(tag(tag_s)), lang(lang_s), "{tag_s}");
}

fn test_language_two_way(tag_s: &str, lang_s: Option<&str>) {
    test_tag_from_language(tag_s, lang_s);
    test_tag_to_language(tag_s, lang_s);
}

fn test_tags_to_script_and_language(
    script_tag_s: &str,
    lang_tag_s: &str,
    script_s: &str,
    lang_s: Option<&str>,
) {
    let (script, language) =
        ot_tags_to_script_and_language(tag(script_tag_s), Some(tag(lang_tag_s)));
    assert_eq!(
        script.map_or(Tag::from_u32(0), |s| Tag(s.tag())),
        tag(script_s),
        "{script_tag_s} {lang_tag_s}"
    );
    assert_eq!(
        language.as_ref().map(Language::as_str),
        lang_s,
        "{script_tag_s} {lang_tag_s}"
    );
}

#[test]
fn ot_tags_to_script_and_language_examples() {
    test_tags_to_script_and_language("DFLT", "ENG", "", Some("en-x-hbsc-44464c54"));
    test_tags_to_script_and_language("latn", "ENG", "Latn", Some("en"));
    test_tags_to_script_and_language("deva", "MAR", "Deva", Some("mr-x-hbsc-64657661"));
    test_tags_to_script_and_language("dev2", "MAR", "Deva", Some("mr-x-hbsc-64657632"));
    test_tags_to_script_and_language("dev3", "MAR", "Deva", Some("mr"));
    test_tags_to_script_and_language("qaa", "QTZ0", "Qaaa", Some("x-hbot-51545a30-hbsc-71616120"));
    // The default language tag maps to no language.
    test_tags_to_script_and_language("DFLT", "dflt", "", None);
}

#[test]
fn no_language_tag_is_the_default_language_tag() {
    assert_eq!(
        ot_tags_to_script_and_language(tag("latn"), None),
        ot_tags_to_script_and_language(tag("latn"), Some(OT_TAG_DEFAULT_LANGUAGE))
    );
    assert_eq!(
        ot_tags_to_script_and_language(tag("DFLT"), None),
        (None, None)
    );
}

#[test]
fn ot_tag_language() {
    assert_eq!(OT_TAG_DEFAULT_LANGUAGE, Tag(*b"dflt"));
    test_language_two_way("dflt", None);

    test_language_two_way("ALT", Some("alt"));

    test_language_two_way("ARA", Some("ar"));

    test_language_two_way("AZE", Some("az"));
    test_tag_from_language("AZE", Some("az-ir"));
    test_tag_from_language("AZE", Some("az-az"));

    test_language_two_way("ENG", Some("en"));
    test_tag_from_language("ENG", Some("en_US"));

    test_language_two_way("CJA", Some("cja-x-hbot-434a4120"));
    test_language_two_way("CJM", Some("cjm-x-hbot-434a4d20"));
    test_tag_from_language("CJM", Some("cjm"));
    test_language_two_way("EVN", Some("eve"));

    test_language_two_way("HAL", Some("cfm"));
    test_tag_from_language("HAL", Some("flm"));

    test_language_two_way("HYE0", Some("hy"));
    test_language_two_way("HYE", Some("hyw"));

    for l in [
        "bgr", "cbl", "cka", "cmr", "cnb", "cnh", "cnk", "cnw", "csh", "csy", "ctd", "czt", "dao",
        "hlt", "mrh", "pck", "sez", "tcp", "tcz", "yos", "zom",
    ] {
        test_tag_from_language("QIN", Some(l));
    }
    test_tag_to_language("QIN", Some("bgr"));

    test_language_two_way("FAR", Some("fa"));
    test_tag_from_language("FAR", Some("fa_IR"));

    test_language_two_way("MNK", Some("man"));

    test_language_two_way("SWA", Some("aii"));

    test_language_two_way("SYR", Some("syr"));
    test_tag_from_language("SYR", Some("amw"));
    test_tag_from_language("SYR", Some("cld"));
    test_tag_from_language("SYR", Some("syc"));

    test_language_two_way("TUA", Some("tru"));

    test_tag_from_language("ZHS", Some("zh"));
    test_tag_from_language("ZHS", Some("zh-cn"));
    test_tag_from_language("ZHS", Some("zh-sg"));
    test_tag_from_language("ZHTM", Some("zh-mo"));
    test_tag_from_language("ZHTM", Some("zh-hant-mo"));
    test_tag_from_language("ZHS", Some("zh-hans-mo"));
    test_language_two_way("ZHH", Some("zh-HK"));
    test_tag_from_language("ZHH", Some("zH-HanT-hK"));
    test_tag_from_language("ZHS", Some("zH-HanS-hK"));
    test_tag_from_language("ZHT", Some("zh-tw"));
    test_language_two_way("ZHS", Some("zh-Hans"));
    test_language_two_way("ZHT", Some("zh-Hant"));
    test_tag_from_language("ZHS", Some("zh-xx"));

    test_tag_from_language("ZHS", Some("zh-Hans-TW"));

    test_tag_from_language("ZHH", Some("yue"));
    test_tag_from_language("ZHH", Some("yue-Hant"));
    test_tag_from_language("ZHS", Some("yue-Hans"));

    test_language_two_way("ABC", Some("abc-x-hbot-41424320"));
    test_language_two_way("ABCD", Some("x-hbot-41424344"));
    test_tag_from_language("ABC", Some("asdf-asdf-wer-x-hbotabc-zxc"));
    test_tag_from_language("ABC", Some("asdf-asdf-wer-x-hbotabc"));
    test_tag_from_language("ABCD", Some("asdf-asdf-wer-x-hbotabcd"));
    test_tag_from_language("ABC", Some("asdf-asdf-wer-x-hbot-41424320-zxc"));
    test_tag_from_language("ABC", Some("asdf-asdf-wer-x-hbot-41424320"));
    test_tag_from_language("ABCD", Some("asdf-asdf-wer-x-hbot-41424344"));

    test_tag_from_language("dflt", Some("asdf-asdf-wer-x-hbot"));
    test_tag_from_language("dflt", Some("asdf-asdf-wer-x-hbot-zxc"));
    test_tag_from_language("dflt", Some("asdf-asdf-wer-x-hbot-zxc-414243"));
    test_tag_from_language("dflt", Some("asdf-asdf-wer-x-hbot-414243"));
    test_tag_from_language("dflt", Some("asdf-asdf-wer-x-hbot-4142432"));

    test_tag_from_language("dflt", Some("xy"));
    test_tag_from_language("XYZ", Some("xyz"));
    test_tag_from_language("XYZ", Some("xyz-qw"));

    // Invalid input: the answer only must not crash or loop.
    test_tag_from_language("IPPH", Some("-fonipa"));

    // "-fonipa" as a substring but not as the subtag "fonipa".
    test_tag_from_language("ENG", Some("en-fonipax"));
    test_tag_from_language("ENG", Some("en-x-fonipa"));
    test_tag_from_language("ENG", Some("en-a-fonipa"));
    test_tag_from_language("ENG", Some("en-a-qwe-b-fonipa"));

    // International Phonetic Alphabet.
    test_tag_from_language("IPPH", Some("en-fonipa"));
    test_tag_from_language("IPPH", Some("en-fonipax-fonipa"));
    test_tag_from_language("IPPH", Some("rm-CH-fonipa-sursilv-x-foobar"));
    test_language_two_way("IPPH", Some("und-fonipa"));
    test_tag_from_language("IPPH", Some("zh-fonipa"));

    // North American Phonetic Alphabet.
    test_tag_from_language("APPH", Some("en-fonnapa"));
    test_tag_from_language("APPH", Some("chr-fonnapa"));
    test_language_two_way("APPH", Some("und-fonnapa"));

    // Khutsuri Georgian.
    test_tag_from_language("KGE", Some("ka-Geok"));
    test_language_two_way("KGE", Some("und-Geok"));

    // Irish Traditional.
    test_tag_from_language("IRT", Some("ga-Latg"));
    test_language_two_way("IRT", Some("ghc"));

    // Moldavian.
    test_language_two_way("MOL", Some("ro-MD"));

    // Polytonic Greek.
    test_language_two_way("PGR", Some("el-polyton"));
    test_tag_from_language("PGR", Some("el-CY-polyton"));

    // Estrangela, Western and Eastern Syriac.
    for (t, s) in [("SYRE", "Syre"), ("SYRJ", "Syrj"), ("SYRN", "Syrn")] {
        test_tag_from_language(t, Some(&format!("aii-{s}")));
        test_tag_from_language(t, Some(&format!("de-{s}")));
        test_tag_from_language(t, Some(&format!("syr-{s}")));
        test_language_two_way(t, Some(&format!("und-{s}")));
    }

    // x-hbot overrides the base language.
    for l in [
        "fa-x-hbotabc-hbot-41686121-zxc",
        "fa-ir-x-hbotabc-hbot-41686121-zxc",
        "zh-x-hbotabc-hbot-41686121-zxc",
        "zh-cn-x-hbotabc-hbot-41686121-zxc",
        "zh-xy-x-hbotabc-hbot-41686121-zxc",
        "xyz-xy-x-hbotabc-hbot-41686121-zxc",
    ] {
        test_tag_from_language("ABC", Some(l));
    }
    for l in [
        "fa-x-hbot-41686121-hbotabc-zxc",
        "fa-ir-x-hbot-41686121-hbotabc-zxc",
        "zh-x-hbot-41686121-hbotabc-zxc",
        "zh-cn-x-hbot-41686121-hbotabc-zxc",
        "zh-xy-x-hbot-41686121-hbotabc-zxc",
        "xyz-xy-x-hbot-41686121-hbotabc-zxc",
    ] {
        test_tag_from_language("Aha!", Some(l));
    }

    // Invalid x-hbot.
    test_tag_from_language("dflt", Some("x-hbot"));
    test_tag_from_language("dflt", Some("x-hbot-"));
    test_tag_from_language("dflt", Some("x-hbot-1"));
    test_tag_from_language("dflt", Some("x-hbot-1a"));
    test_tag_from_language("dflt", Some("x-hbot-1a2b3c4x"));
    test_tag_from_language("2lon", Some("x-hbot-326c6f6e67"));

    // Unnormalized BCP 47 tags.
    test_tag_from_language("ARA", Some("ar-aao"));
    test_tag_from_language("JBO", Some("art-lojban"));
    test_tag_from_language("KOK", Some("kok-gom"));
    test_tag_from_language("LTZ", Some("i-lux"));
    test_tag_from_language("MNG", Some("drh"));
    test_tag_from_language("MOR", Some("ar-ary"));
    test_tag_from_language("MOR", Some("ar-ary-DZ"));
    test_tag_from_language("NOR", Some("no-bok"));
    test_tag_from_language("NYN", Some("no-nyn"));
    test_tag_from_language("ZHS", Some("i-hak"));
    test_tag_from_language("ZHS", Some("zh-guoyu"));
    test_tag_from_language("ZHS", Some("zh-min"));
    test_tag_from_language("ZHS", Some("zh-min-nan"));
    test_tag_from_language("ZHS", Some("zh-xiang"));

    // BCP 47 tags that look similar to unrelated language system tags.
    test_tag_from_language("SQI", Some("als"));
    test_tag_from_language("dflt", Some("far"));

    // A UN M.49 region code, not an extended language subtag.
    test_tag_from_language("ARA", Some("ar-001"));

    // An invalid tag.
    test_tag_from_language("TRK", Some("tr@foo=bar"));
}

fn test_tags(
    script: Option<Script>,
    lang_s: &str,
    script_count: usize,
    language_count: usize,
    expected_scripts: &[&str],
    expected_languages: &[&str],
) {
    let l = lang(Some(lang_s));
    let (s, t) =
        ot_tags_from_script_and_language_max(script, l.as_ref(), script_count, language_count);
    let tags = |v: &[&str]| v.iter().map(|x| tag(x)).collect::<Vec<_>>();
    assert_eq!(s, tags(expected_scripts), "{lang_s}");
    assert_eq!(t, tags(expected_languages), "{lang_s}");
}

#[test]
fn ot_tag_full() {
    let (ms, ml) = (MAX_TAGS_PER_SCRIPT, MAX_TAGS_PER_LANGUAGE);
    test_tags(None, "en", ms, ml, &[], &["ENG"]);
    test_tags(None, "en-x-hbscdflt", ms, ml, &["DFLT"], &["ENG"]);
    test_tags(sc("Latn"), "en", ms, ml, &["latn"], &["ENG"]);
    test_tags(sc("Hrkt"), "und", ms, 0, &["kana"], &[]);
    test_tags(sc("Latn"), "en", 0, 0, &[], &[]);
    test_tags(None, "und-fonnapa", ms, ml, &[], &["APPH"]);
    test_tags(None, "en-fonnapa", ms, ml, &[], &["APPH"]);
    test_tags(None, "x-hbot1234-hbsc5678", ms, ml, &["5678"], &["1234"]);
    test_tags(None, "x-hbsc5678-hbot1234", ms, ml, &["5678"], &["1234"]);
    test_tags(
        sc("Mlym"),
        "ml",
        ms,
        ml,
        &["mlm3", "mlm2", "mlym"],
        &["MAL", "MLR"],
    );
    test_tags(sc("Mlym"), "ml", 1, 1, &["mlm3"], &["MAL"]);
    test_tags(sc("Mymr"), "und", ms, 0, &["mym2", "mymr"], &[]);
    test_tags(None, "xyz", ms, ml, &[], &["XYZ"]);
    test_tags(None, "xy", ms, ml, &[], &[]);
}

#[test]
fn the_default_counts_are_harfbuzz_maxima() {
    assert_eq!((MAX_TAGS_PER_SCRIPT, MAX_TAGS_PER_LANGUAGE), (3, 3));
    let l = lang(Some("ml"));
    assert_eq!(
        ot_tags_from_script_and_language(sc("Mlym"), l.as_ref()),
        ot_tags_from_script_and_language_max(sc("Mlym"), l.as_ref(), 3, 3)
    );
}

#[test]
fn script_tags_preserve_harfbuzz_values() {
    assert_eq!(
        ot_tag_to_script(tag("LATN")).map(Script::tag),
        Some(*b"LATN")
    );
    assert_eq!(
        ot_tag_to_script(tag("aran")).map(Script::tag),
        Some(*b"Aran")
    );
    assert_eq!(ot_tag_to_script(tag("abc2")), Some(Script::UNKNOWN));
    // The language still carries the script tag HarfBuzz would give.
    let (_, l) = ot_tags_to_script_and_language(tag("aran"), Some(tag("ARA")));
    assert_eq!(l.as_ref().map(Language::as_str), Some("ar"));
}

#[test]
fn inv1_script_tag_bytes() {
    let cases: [(&str, Option<&[u8; 4]>); 9] = [
        ("DFLT", None),
        ("latn", Some(b"Latn")),
        ("aran", Some(b"Aran")),
        ("qaai", Some(b"Qaai")),
        ("ab!d", Some(b"Ab!d")),
        ("LATN", Some(b"LATN")),
        ("dev3", Some(b"Deva")),
        ("abc2", Some(b"Zzzz")),
        ("zzzz", Some(b"Zzzz")),
    ];
    for (source, expected) in cases {
        assert_eq!(
            ot_tag_to_script(tag(source)).map(Script::tag).as_ref(),
            expected,
            "{source}"
        );
    }
}

#[test]
fn inv2_script_with_language() {
    for source in [
        "DFLT", "latn", "aran", "qaai", "ab!d", "LATN", "dev3", "abc2", "zzzz",
    ] {
        let alone = ot_tag_to_script(tag(source));
        for language in [None, Some(tag("ARA"))] {
            let (script, _) = ot_tags_to_script_and_language(tag(source), language);
            assert_eq!(script, alone, "{source} {language:?}");
        }
    }
    assert_eq!(
        ot_tags_to_script_and_language(tag("aran"), Some(tag("ARA")))
            .0
            .map(Script::tag),
        Some(*b"Aran")
    );
}

#[test]
fn api_guide_example() {
    // docs/api.md
    let ja = Language::new(b"ja_JP.UTF-8").unwrap();
    assert_eq!(ja.as_str(), "ja-jp");
    let deva = Script::from_iso15924(b"Deva");
    let mr = Language::new(b"mr");
    let (scripts, languages) = ot_tags_from_script_and_language(deva, mr.as_ref());
    assert_eq!(scripts, [Tag(*b"dev3"), Tag(*b"dev2"), Tag(*b"deva")]);
    assert_eq!(languages, [Tag(*b"MAR ")]);
    let (script, language) = ot_tags_to_script_and_language(Tag(*b"dev2"), Some(Tag(*b"MAR ")));
    assert_eq!(script, deva);
    assert_eq!(language.unwrap().as_str(), "mr-x-hbsc-64657632");
}
