//! Adobe Type 1 eexec and charstring cipher.

use alloc::vec::Vec;

const EEXEC_SEED: u16 = 55_665;
const CHARSTRING_SEED: u16 = 4_330;

fn decrypt(data: &[u8], seed: u16, discard: usize) -> Option<Vec<u8>> {
    if data.len() < discard {
        return None;
    }
    let mut state = seed;
    let mut plain = Vec::with_capacity(data.len().saturating_sub(discard));
    for (index, &cipher) in data.iter().enumerate() {
        let byte = cipher ^ (state >> 8) as u8;
        state = u16::from(cipher)
            .wrapping_add(state)
            .wrapping_mul(52_845)
            .wrapping_add(22_719);
        if index >= discard {
            plain.push(byte);
        }
    }
    Some(plain)
}

/// Decrypt an eexec segment, discarding its first four random bytes.
pub fn decrypt_eexec(data: &[u8]) -> Option<Vec<u8>> {
    decrypt(data, EEXEC_SEED, 4)
}

/// Decrypt a charstring using `len_iv` random prefix bytes.
///
/// A negative `len_iv` identifies a plaintext charstring.
pub fn decrypt_charstring(data: &[u8], len_iv: i32) -> Option<Vec<u8>> {
    if len_iv < 0 {
        return Some(data.to_vec());
    }
    let discard = usize::try_from(len_iv).ok()?;
    decrypt(data, CHARSTRING_SEED, discard)
}
