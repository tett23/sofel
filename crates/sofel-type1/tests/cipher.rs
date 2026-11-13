use sofel_type1::cipher::{decrypt_charstring, decrypt_eexec};

#[test]
fn adr0065_i1_type1_decrypts_eexec_and_charstring() {
    let eexec = [
        0xb8, 0x58, 0x8d, 0xa5, 0x42, 0x6e, 0x54, 0x79, 0x75, 0xa1, 0x11, 0x80, 0xdc,
    ];
    assert_eq!(decrypt_eexec(&eexec), Some(b"/FontName".to_vec()));
    let charstring = [0x51, 0x6a, 0x9f, 0x0c, 0x4f, 0xb0];
    assert_eq!(decrypt_charstring(&charstring, 4), Some(vec![0x8b, 0x0e]));
    assert_eq!(
        decrypt_charstring(&charstring, -1),
        Some(vec![0x51, 0x6a, 0x9f, 0x0c, 0x4f, 0xb0])
    );
}

#[test]
fn adr0065_i2_type1_rejects_short_random_prefix() {
    assert_eq!(decrypt_eexec(&[0xb8, 0x58, 0x8d]), None);
    assert_eq!(decrypt_charstring(&[0x51, 0x6a, 0x9f], 4), None);
}
