//! Static dictionary in RFC 7932, Appendix A, with the word transforms from Appendix B.
use alloc::vec::Vec;
include!("dictionary/tables.rs");
const WORDS: &[u8] = include_bytes!("dictionary/words.bin");
const START: [usize; 21] = [
    0, 4096, 9216, 21504, 35840, 44032, 53248, 63488, 74752, 87040, 93696, 100864, 104704, 106752,
    108928, 113536, 115968, 118528, 119872, 121280, 122016,
];
const WIDTH: [u8; 21] = [
    10, 10, 11, 11, 10, 10, 10, 10, 10, 9, 9, 8, 7, 7, 8, 7, 7, 6, 6, 5, 5,
];

fn affix(index: u8) -> Option<&'static [u8]> {
    AFFIXES.get(usize::from(index)..)?.split(|&b| b == 0).next()
}

pub(super) fn append(out: &mut Vec<u8>, length: usize, id: usize, end: usize) -> Option<()> {
    let slot = length.checked_sub(4)?;
    let width = *WIDTH.get(slot)?;
    let index = id & (1usize << width).checked_sub(1)?;
    let start = START.get(slot)?.checked_add(index.checked_mul(length)?)?;
    let mut word = WORDS.get(start..start.checked_add(length)?)?;
    let &[prefix, op, suffix] = RULES.get(id >> width)?;
    let prefix = affix(prefix)?;
    let suffix = affix(suffix)?;
    match op {
        1..=9 => word = word.get(..word.len().saturating_sub(usize::from(op)))?,
        12..=20 => word = word.get(usize::from(op.checked_sub(11)?).min(word.len())..)?,
        _ => {}
    }
    let size = prefix
        .len()
        .checked_add(word.len())?
        .checked_add(suffix.len())?;
    if out.len().checked_add(size)? > end {
        return None;
    }
    out.extend_from_slice(prefix);
    let at = out.len();
    out.extend_from_slice(word);
    if op == 10 || op == 11 {
        let mut rest = out.get_mut(at..)?;
        while let Some(&first) = rest.first() {
            let step = match first {
                0..=191 => {
                    if first.is_ascii_lowercase() {
                        *rest.first_mut()? ^= 32;
                    }
                    1
                }
                192..=223 => {
                    *rest.get_mut(1)? ^= 32;
                    2
                }
                _ => {
                    *rest.get_mut(2)? ^= 5;
                    3
                }
            };
            if op == 10 {
                break;
            }
            rest = rest.get_mut(step..)?;
        }
    }
    out.extend_from_slice(suffix);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0065_i2_dictionary_transform_output_limit() {
        let mut bytes = Vec::new();
        // Rule 4 capitalizes the first letter and appends a space.
        assert_eq!(append(&mut bytes, 4, 4 << 10, 4), None);
        assert!(bytes.is_empty());
        assert_eq!(append(&mut bytes, 4, 4 << 10, 5), Some(()));
        assert_eq!(bytes, b"Time ");
        assert_eq!(append(&mut bytes, 4, 121 << 10, 32), None);
        assert_eq!(bytes, b"Time ");
    }
}
