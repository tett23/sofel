//! Fixed extra-font corpus registration (ADR 0031, I04-I05).

#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_conformance::extra::{Corpus, sample_glyphs};

#[test]
fn i04_sample_boundaries_and_mandatory_glyphs() {
    assert_eq!(sample_glyphs(0, &[]).unwrap(), Vec::<u32>::new());
    assert_eq!(sample_glyphs(1, &[0]).unwrap(), vec![0]);
    assert_eq!(sample_glyphs(2, &[0, 1]).unwrap(), vec![0, 1]);
    assert_eq!(sample_glyphs(129, &[0, 128]).unwrap().len(), 128);
    assert_eq!(sample_glyphs(129, &[0, 128]).unwrap().first(), Some(&0));
    assert_eq!(sample_glyphs(129, &[0, 128]).unwrap().last(), Some(&128));
    assert!(sample_glyphs(129, &(0..129).collect::<Vec<_>>()).is_err());
    assert!(sample_glyphs(200, &(1..=128).collect::<Vec<_>>()).is_err());
    assert!(sample_glyphs(5, &[5]).is_err());
}

#[test]
fn i04_registry_rejects_duplicate_id_and_hash() {
    let lock = "font\thttps://example.invalid/a\t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\t1\tfonts/a.ttf\nlicense\thttps://example.invalid/l\tabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789\t1\tlicenses/l.txt\n";
    let row = "a\tfont\t-\tfonts/a.ttf\t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\tlicense\tshape\n";
    assert_eq!(Corpus::parse(lock, row).unwrap().fonts.len(), 1);
    assert!(Corpus::parse(lock, &(row.to_string() + row)).is_err());
    assert!(Corpus::parse(&(lock.to_string() + "copy\thttps://example.invalid/b\t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\t1\tfonts/b.ttf\n"), row).is_err());
    assert!(Corpus::parse(lock, "").is_err());
}

#[test]
fn i05_every_registered_font_has_a_role() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    assert_eq!(corpus.fonts.len(), 13);
    assert!(corpus.fonts.iter().all(|f| !f.features.is_empty()));
    assert_eq!(
        corpus
            .fonts
            .iter()
            .filter(|f| f.features.contains(&"ttc".to_string()))
            .count(),
        1
    );
}
