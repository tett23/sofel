//! Every face and feature structure in the fixed extra corpus
//! (ADR 0031, I04-I05). Missing files are an error, including on wasm.

#![allow(clippy::all, clippy::unwrap_used, clippy::panic)]

use sofel_conformance::extra::Corpus;
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::fvar::Fvar;
use sofel_font::read::sfnt::FontFile;
use sofel_font::read::{Severity, Tag, validate};
use sofel_font::read::{avar::Avar, colr::Colr, math::Math, sbix::Sbix, svg::Svg};

#[test]
fn i01_all_locked_sources_and_licenses_are_present() {
    let root = paths::testdata().join("extra-fonts");
    let mut count: usize = 0;
    for line in include_str!("../../../fonts.lock")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let parts = line.split('\t').collect::<Vec<_>>();
        assert_eq!(parts.len(), 5);
        let expected = parts.get(3).unwrap().parse::<u64>().unwrap();
        let path = root.join(parts.get(4).unwrap());
        let actual = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run scripts/fetch-fonts.sh", path.display()))
            .len();
        assert_eq!(actual, expected, "{}", path.display());
        count = count.saturating_add(1);
    }
    assert_eq!(count, 24);
}

#[test]
fn i05_every_face_opens_and_validates() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let root = paths::testdata().join("extra-fonts");
    let mut face_count = 0;
    let mut errors = Vec::new();
    for entry in &corpus.fonts {
        let path = root.join(&entry.path);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run scripts/fetch-fonts.sh", path.display()));
        let file = FontFile::new(&bytes).unwrap();
        let count = match file {
            FontFile::Sfnt(_) => 1,
            FontFile::Collection(c) => c.num_fonts(),
            FontFile::Dfont(d) => d.num_fonts(),
        };
        assert_eq!(count, if entry.id == "source-han" { 45 } else { 1 });
        for face in 0..count {
            let sfnt = match file {
                FontFile::Sfnt(s) => s,
                FontFile::Collection(c) => c.font(face).unwrap(),
                FontFile::Dfont(d) => d.font(face).unwrap(),
            };
            let font = Font::new(&bytes, face).unwrap();
            assert!(font.num_glyphs() > 0, "{}#{face}", entry.id);
            for record in sfnt.records() {
                assert!(
                    record.data(sfnt.file()).is_some(),
                    "{}#{face} table {:?}",
                    entry.id,
                    record.tag
                );
            }
            let required: &[(&str, &[u8; 4])] = &[
                ("cff", b"CFF "),
                ("cff2", b"CFF2"),
                ("math", b"MATH"),
                ("colr", b"COLR"),
                ("sbix", b"sbix"),
                ("svg", b"SVG "),
                ("variable", b"fvar"),
                ("avar2", b"avar"),
                ("vertical", b"vmtx"),
            ];
            for &(feature, tag) in required {
                if entry.features.iter().any(|name| name == feature) {
                    assert!(
                        font.table(Tag::new(tag)).is_some(),
                        "{}#{face}: missing {feature}",
                        entry.id
                    );
                }
            }
            validate::sfnt(&sfnt, &mut |d| {
                if d.severity == Severity::Error {
                    errors.push(format!(
                        "{}#{face}: {:?} {} at {}",
                        entry.id,
                        d.table,
                        d.kind.id(),
                        d.offset
                    ));
                }
            });
            face_count += 1;
        }
    }
    assert_eq!(face_count, 57);
    assert!(
        errors.is_empty(),
        "{} errors: {}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn i05_math_color_bitmap_svg_and_avar2_views() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let root = paths::testdata().join("extra-fonts");
    for entry in &corpus.fonts {
        let bytes = std::fs::read(root.join(&entry.path)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        match entry.id.as_str() {
            "stix-math" => {
                let math = Math::new(font.table(Tag::new(b"MATH")).unwrap()).unwrap();
                assert!(math.constants().unwrap().is_some());
                let info = math.glyph_info().unwrap().unwrap();
                assert!(info.italics_correction().unwrap().is_some());
                let variants = math.variants().unwrap().unwrap();
                assert!(variants.vert_glyph_count() > 0);
                assert!(variants.vert_coverage().unwrap().is_some());
            }
            "color-cff" => {
                let colr = Colr::new(font.table(Tag::new(b"COLR")).unwrap()).unwrap();
                assert_eq!(colr.version(), 1);
                assert!(colr.num_base_glyph_paints() > 0);
                assert!(colr.base_glyph_paint_record(0).is_some());
            }
            "color-sbix" => {
                let sbix = Sbix::new(font.table(Tag::new(b"sbix")).unwrap()).unwrap();
                assert!(sbix.num_strikes() > 0);
                let strike = sbix.strike(0).unwrap();
                assert!((0..font.num_glyphs()).any(|glyph| strike.glyph(glyph).is_some()));
            }
            "color-svg" => {
                let svg = Svg::new(font.table(Tag::new(b"SVG ")).unwrap()).unwrap();
                assert!(svg.num_documents() > 0);
                for i in 0..svg.num_documents() {
                    let record = svg.record(i).unwrap();
                    assert!(record.start_glyph <= record.end_glyph);
                    assert!(svg.document(i).is_some());
                }
            }
            "amstelvar" => {
                let avar = Avar::new(font.table(Tag::new(b"avar")).unwrap()).unwrap();
                assert_eq!(avar.major_version(), 2);
                assert_eq!(avar.axis_count(), 92);
                let (map, store) = avar.v2_offsets().unwrap();
                assert!(map > 0 && store > 0);
            }
            "source-serif" => assert!(font.table(Tag::new(b"BASE")).is_some()),
            _ => {}
        }
    }
}

#[test]
fn i05_existing_variable_colr_has_its_license_and_axes() {
    let root = paths::testdata();
    let bytes =
        std::fs::read(root.join("harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf"))
            .unwrap();
    assert_eq!(bytes.len(), 53_096);
    assert!(
        root.join("extra-fonts/licenses/color-license.txt")
            .is_file()
    );
    let font = Font::new(&bytes, 0).unwrap();
    let colr = Colr::new(font.table(Tag::new(b"COLR")).unwrap()).unwrap();
    assert_eq!(colr.version(), 1);
    assert!(colr.num_base_glyph_paints() > 0);
    assert!(font.table(Tag::new(b"CPAL")).is_some());
    assert!(font.table(Tag::new(b"HVAR")).is_some());
    assert!(font.table(Tag::new(b"gvar")).is_some());
    let fvar = Fvar::new(font.table(Tag::new(b"fvar")).unwrap()).unwrap();
    assert_eq!(fvar.axis_count(), 44);
}
