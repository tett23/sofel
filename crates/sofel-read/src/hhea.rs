//! The `hhea` and `HHEA` tables.

use crate::be::{self, to_u32};
use crate::{CountWidth, Diagnostic, Kind, ReadError, Tag};

pub const TAG: Tag = Tag::new(b"hhea");
pub const TAG_WIDE: Tag = Tag::new(b"HHEA");

const FIXED: usize = 34;

#[derive(Clone, Copy, Debug)]
pub struct Hhea<'a> {
    h: &'a [u8; FIXED],
    number_of_h_metrics: u32,
}

fn needed(width: CountWidth) -> usize {
    FIXED.saturating_add(width.bytes())
}

impl<'a> Hhea<'a> {
    /// `width` is `U16` for `hhea` and `U24` for `HHEA`.
    pub fn new(data: &'a [u8], width: CountWidth) -> Result<Hhea<'a>, ReadError> {
        let truncated = ReadError::truncated(needed(width), data.len());
        let h = data.first_chunk::<FIXED>().ok_or(truncated)?;
        if u16::from_be_bytes([h[0], h[1]]) != 1 {
            return Err(ReadError::UnknownVersion(u32::from_be_bytes([
                h[0], h[1], h[2], h[3],
            ])));
        }
        let number_of_h_metrics = match width {
            CountWidth::U16 => be::u16_at(data, FIXED).map(u32::from),
            CountWidth::U24 => be::u24_at(data, FIXED),
        }
        .ok_or(truncated)?;
        Ok(Hhea {
            h,
            number_of_h_metrics,
        })
    }

    pub fn major_version(&self) -> u16 {
        u16::from_be_bytes([self.h[0], self.h[1]])
    }
    pub fn minor_version(&self) -> u16 {
        u16::from_be_bytes([self.h[2], self.h[3]])
    }
    pub fn ascender(&self) -> i16 {
        i16::from_be_bytes([self.h[4], self.h[5]])
    }
    pub fn descender(&self) -> i16 {
        i16::from_be_bytes([self.h[6], self.h[7]])
    }
    pub fn line_gap(&self) -> i16 {
        i16::from_be_bytes([self.h[8], self.h[9]])
    }
    pub fn advance_width_max(&self) -> u16 {
        u16::from_be_bytes([self.h[10], self.h[11]])
    }
    pub fn min_left_side_bearing(&self) -> i16 {
        i16::from_be_bytes([self.h[12], self.h[13]])
    }
    pub fn min_right_side_bearing(&self) -> i16 {
        i16::from_be_bytes([self.h[14], self.h[15]])
    }
    pub fn x_max_extent(&self) -> i16 {
        i16::from_be_bytes([self.h[16], self.h[17]])
    }
    pub fn caret_slope_rise(&self) -> i16 {
        i16::from_be_bytes([self.h[18], self.h[19]])
    }
    pub fn caret_slope_run(&self) -> i16 {
        i16::from_be_bytes([self.h[20], self.h[21]])
    }
    pub fn caret_offset(&self) -> i16 {
        i16::from_be_bytes([self.h[22], self.h[23]])
    }
    pub fn metric_data_format(&self) -> i16 {
        i16::from_be_bytes([self.h[32], self.h[33]])
    }
    pub fn number_of_h_metrics(&self) -> u32 {
        self.number_of_h_metrics
    }
}

pub fn validate(data: &[u8], width: CountWidth, report: &mut dyn FnMut(Diagnostic)) {
    let tag = match width {
        CountWidth::U16 => TAG,
        CountWidth::U24 => TAG_WIDE,
    };
    let mut emit = |offset: u32, kind: Kind| report(Diagnostic::new(tag, offset, kind));
    match Hhea::new(data, width) {
        Ok(_) => {}
        Err(ReadError::UnknownVersion(found)) => emit(0, Kind::UnknownVersion { found }),
        Err(_) => emit(
            0,
            Kind::Truncated {
                needed: to_u32(needed(width)),
                available: to_u32(data.len()),
            },
        ),
    }
}
