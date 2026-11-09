//! The `head` table.

use crate::be::to_u32;
use crate::{Diagnostic, Fixed, Kind, LongDateTime, ReadError, Tag};

pub const TAG: Tag = Tag::new(b"head");
pub const MAGIC_NUMBER: u32 = 0x5F0F_3CF5;
const LEN: usize = 54;

#[derive(Clone, Copy, Debug)]
pub struct Head<'a> {
    h: &'a [u8; LEN],
}

impl<'a> Head<'a> {
    pub fn new(data: &'a [u8]) -> Result<Head<'a>, ReadError> {
        let h = data
            .first_chunk::<LEN>()
            .ok_or(ReadError::truncated(LEN, data.len()))?;
        if u16::from_be_bytes([h[0], h[1]]) != 1 {
            return Err(ReadError::UnknownVersion(u32::from_be_bytes([
                h[0], h[1], h[2], h[3],
            ])));
        }
        Ok(Head { h })
    }

    pub fn major_version(&self) -> u16 {
        u16::from_be_bytes([self.h[0], self.h[1]])
    }
    pub fn minor_version(&self) -> u16 {
        u16::from_be_bytes([self.h[2], self.h[3]])
    }
    pub fn font_revision(&self) -> Fixed {
        Fixed(i32::from_be_bytes([
            self.h[4], self.h[5], self.h[6], self.h[7],
        ]))
    }
    pub fn checksum_adjustment(&self) -> u32 {
        u32::from_be_bytes([self.h[8], self.h[9], self.h[10], self.h[11]])
    }
    pub fn magic_number(&self) -> u32 {
        u32::from_be_bytes([self.h[12], self.h[13], self.h[14], self.h[15]])
    }
    pub fn flags(&self) -> u16 {
        u16::from_be_bytes([self.h[16], self.h[17]])
    }
    pub fn units_per_em(&self) -> u16 {
        u16::from_be_bytes([self.h[18], self.h[19]])
    }
    pub fn created(&self) -> LongDateTime {
        let h = self.h;
        LongDateTime(i64::from_be_bytes([
            h[20], h[21], h[22], h[23], h[24], h[25], h[26], h[27],
        ]))
    }
    pub fn modified(&self) -> LongDateTime {
        let h = self.h;
        LongDateTime(i64::from_be_bytes([
            h[28], h[29], h[30], h[31], h[32], h[33], h[34], h[35],
        ]))
    }
    pub fn x_min(&self) -> i16 {
        i16::from_be_bytes([self.h[36], self.h[37]])
    }
    pub fn y_min(&self) -> i16 {
        i16::from_be_bytes([self.h[38], self.h[39]])
    }
    pub fn x_max(&self) -> i16 {
        i16::from_be_bytes([self.h[40], self.h[41]])
    }
    pub fn y_max(&self) -> i16 {
        i16::from_be_bytes([self.h[42], self.h[43]])
    }
    pub fn mac_style(&self) -> u16 {
        u16::from_be_bytes([self.h[44], self.h[45]])
    }
    pub fn lowest_rec_ppem(&self) -> u16 {
        u16::from_be_bytes([self.h[46], self.h[47]])
    }
    pub fn font_direction_hint(&self) -> i16 {
        i16::from_be_bytes([self.h[48], self.h[49]])
    }
    pub fn index_to_loc_format(&self) -> i16 {
        i16::from_be_bytes([self.h[50], self.h[51]])
    }
    pub fn glyph_data_format(&self) -> i16 {
        i16::from_be_bytes([self.h[52], self.h[53]])
    }
}

pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: u32, kind: Kind| report(Diagnostic::new(TAG, offset, kind));
    let head = match Head::new(data) {
        Ok(h) => h,
        Err(ReadError::UnknownVersion(found)) => return emit(0, Kind::UnknownVersion { found }),
        Err(_) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(LEN),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    if head.magic_number() != MAGIC_NUMBER {
        emit(
            12,
            Kind::BadMagicNumber {
                expected: MAGIC_NUMBER,
                found: head.magic_number(),
            },
        );
    }
    if !(16..=16384).contains(&head.units_per_em()) {
        emit(
            18,
            Kind::UnitsPerEmOutOfRange {
                found: u32::from(head.units_per_em()),
            },
        );
    }
    if !matches!(head.index_to_loc_format(), 0 | 1) {
        emit(
            50,
            Kind::InvalidIndexToLocFormat {
                found: i32::from(head.index_to_loc_format()),
            },
        );
    }
}
