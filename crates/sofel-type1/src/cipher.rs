//! Adobe Type 1 eexec and charstring cipher.

use alloc::vec::Vec;

use crate::noalloc::cipher::{CHARSTRING_SEED, EEXEC_SEED, step};

fn decrypt(data: &[u8], seed: u16, discard: usize) -> Option<Vec<u8>> {
    if data.len() < discard {
        return None;
    }
    let mut state = seed;
    let mut plain = Vec::with_capacity(data.len().saturating_sub(discard));
    for (index, &cipher) in data.iter().enumerate() {
        let byte = step(&mut state, cipher);
        if index >= discard {
            plain.push(byte);
        }
    }
    Some(plain)
}

/// Decrypt an eexec segment, discarding its first four random bytes.
///
/// Allocates the decrypted bytes (ADR 0094).
pub fn decrypt_eexec(data: &[u8]) -> Option<Vec<u8>> {
    decrypt(data, EEXEC_SEED, 4)
}

/// Decrypt a charstring using `len_iv` random prefix bytes.
///
/// A negative `len_iv` identifies a plaintext charstring.
///
/// Allocates the decrypted bytes, or a copy of `data` when `len_iv` is negative (ADR 0094).
pub fn decrypt_charstring(data: &[u8], len_iv: i32) -> Option<Vec<u8>> {
    if len_iv < 0 {
        return Some(data.to_vec());
    }
    let discard = usize::try_from(len_iv).ok()?;
    decrypt(data, CHARSTRING_SEED, discard)
}
