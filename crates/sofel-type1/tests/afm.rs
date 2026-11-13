use sofel_type1::afm::Afm;

const METRICS: &[u8] = b"StartFontMetrics 4.1\nFontName Example\nStartKernData\nStartKernPairs 3\nKPX A V -80\nKPY V A 12\nKP T o -30 4\nEndKernPairs\nEndKernData\nEndFontMetrics\n";

#[test]
fn adr0065_i1_afm_pair_kerning_and_absent_pair() -> Result<(), sofel_type1::afm::AfmError> {
    let afm = Afm::parse(METRICS)?;
    assert_eq!(afm.kerning(b"A", b"V"), (-80, 0));
    assert_eq!(afm.kerning(b"V", b"A"), (0, 12));
    assert_eq!(afm.kerning(b"T", b"o"), (-30, 4));
    assert_eq!(afm.kerning(b"A", b"A"), (0, 0));
    Ok(())
}

#[test]
fn adr0065_i2_afm_rejects_truncation_and_bad_pair_counts() {
    assert!(
        Afm::parse(b"StartFontMetrics 4.1\nStartKernPairs 1\nKPX A V -80\nEndKernPairs\n").is_err()
    );
    assert!(
        Afm::parse(
            b"StartFontMetrics 4.1\nStartKernPairs 2\nKPX A V -80\nEndKernPairs\nEndFontMetrics\n"
        )
        .is_err()
    );
    assert!(
        Afm::parse(
            b"StartFontMetrics 4.1\nStartKernPairs 999999999999999\nEndKernPairs\nEndFontMetrics\n"
        )
        .is_err()
    );
}
