//! The Adobe Type 1 encryption (Adobe Type 1 Font Format, chapter 7).

/// The initial key of the `eexec` part.
pub(crate) const EEXEC_SEED: u16 = 55_665;
/// The initial key of a charstring.
pub(crate) const CHARSTRING_SEED: u16 = 4_330;

/// Decrypt one byte and advance the key.
pub(crate) fn step(state: &mut u16, cipher: u8) -> u8 {
    let byte = cipher ^ (*state >> 8) as u8;
    *state = u16::from(cipher)
        .wrapping_add(*state)
        .wrapping_mul(52_845)
        .wrapping_add(22_719);
    byte
}
