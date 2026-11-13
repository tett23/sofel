use sofel_type1::pfm::{Pfm, PfmError};

fn put(data: &mut [u8], at: usize, bytes: &[u8]) -> Result<(), PfmError> {
    let end = at.checked_add(bytes.len()).ok_or(PfmError)?;
    data.get_mut(at..end)
        .ok_or(PfmError)?
        .copy_from_slice(bytes);
    Ok(())
}

fn sample() -> Result<Vec<u8>, PfmError> {
    let mut data = vec![0u8; 140];
    put(&mut data, 1, &[1])?;
    put(&mut data, 2, &140u32.to_le_bytes())?;
    put(&mut data, 99, &0u16.to_le_bytes())?;
    put(&mut data, 119, &18u16.to_le_bytes())?;
    put(&mut data, 133, &137u32.to_le_bytes())?;
    put(&mut data, 137, &0u16.to_le_bytes())?;
    Ok(data)
}

#[test]
fn adr0065_i1_pfm_optional_empty_kerning_table() -> Result<(), PfmError> {
    let pfm = Pfm::parse(&sample()?)?;
    assert_eq!(pfm.kerning(b'A', b'V'), 0);
    Ok(())
}

#[test]
fn adr0065_i1_pfm_reads_encoded_pair() -> Result<(), PfmError> {
    let mut data = sample()?;
    data.resize(143, 0);
    put(&mut data, 2, &143u32.to_le_bytes())?;
    put(&mut data, 137, &1u16.to_le_bytes())?;
    put(&mut data, 139, &[b'A', b'V', 0xb0, 0xff])?;
    let pfm = Pfm::parse(&data)?;
    assert_eq!(pfm.kerning(b'A', b'V'), -80);
    assert_eq!(pfm.kerning(b'A', b'A'), 0);
    Ok(())
}

#[test]
fn adr0065_i2_pfm_rejects_invalid_declared_length_and_truncation() -> Result<(), PfmError> {
    let mut data = sample()?;
    put(&mut data, 2, &141u32.to_le_bytes())?;
    assert!(Pfm::parse(&data).is_err());
    let mut data = sample()?;
    put(&mut data, 137, &2u16.to_le_bytes())?;
    assert!(Pfm::parse(&data).is_err());
    Ok(())
}
