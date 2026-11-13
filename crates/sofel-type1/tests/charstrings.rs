use sofel_type1::charstrings::Charstrings;

#[test]
fn adr0065_i1_type1_indexes_binary_charstrings()
-> Result<(), sofel_type1::charstrings::CharstringError> {
    let private = b"/Subrs 1 array\ndup 0 6 RD abcdef NP\n/CharStrings 2 dict dup begin\n/.notdef 5 RD xyz12 ND\n/A 4 RD 1234 ND\nend\n";
    let table = Charstrings::parse(private)?;
    assert_eq!(table.glyph_count(), 2);
    assert_eq!(table.glyph(b"A"), Some(&b"1234"[..]));
    assert_eq!(table.glyph(b".notdef"), Some(&b"xyz12"[..]));
    assert_eq!(table.subroutine(0), Some(&b"abcdef"[..]));
    assert_eq!(table.subroutine(1), None);
    Ok(())
}

#[test]
fn adr0065_i2_type1_rejects_truncated_binary_charstring() {
    let private = b"/CharStrings 1 dict dup begin\n/A 10 RD abc ND\n";
    assert!(Charstrings::parse(private).is_err());
    let private = b"/CharStrings 999999 dict dup begin\n";
    assert!(Charstrings::parse(private).is_err());
}
