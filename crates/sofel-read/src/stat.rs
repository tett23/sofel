//! The `STAT` (style attributes) table, versions 1.1 and 1.2 (ADR 0022).
//!
//! As HarfBuzz's `STAT::sanitize`, the view needs major version 1, a
//! minor version above 0 and the 20-byte header, and reads design axis
//! records of 8 bytes whatever `designAxisSize` says. An axis value
//! table that does not fit is skipped ([`Stat::axis_value`] gives
//! `None`), as HarfBuzz neuters its offset.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/stat>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Fixed, Kind, ReadError, Tag};

/// The `STAT` tag.
pub const TAG: Tag = Tag::new(b"STAT");

/// The size of the header.
const HEADER: usize = 20;
/// The size of a design axis record as HarfBuzz reads it.
const AXIS_RECORD: usize = 8;

/// A design axis record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DesignAxis {
    /// `axisTag`.
    pub tag: Tag,
    /// `axisNameID`.
    pub name_id: u16,
    /// `axisOrdering`.
    pub ordering: u16,
}

/// An axis value table.
#[derive(Clone, Copy, Debug)]
pub enum AxisValue<'a> {
    /// Format 1: a single value.
    Format1 {
        /// `axisIndex`.
        axis_index: u16,
        /// `flags`.
        flags: u16,
        /// `valueNameID`.
        name_id: u16,
        /// `value`.
        value: Fixed,
    },
    /// Format 2: a nominal value and a range.
    Format2 {
        /// `axisIndex`.
        axis_index: u16,
        /// `flags`.
        flags: u16,
        /// `valueNameID`.
        name_id: u16,
        /// `nominalValue`.
        nominal: Fixed,
        /// `rangeMinValue`.
        min: Fixed,
        /// `rangeMaxValue`.
        max: Fixed,
    },
    /// Format 3: a value and its style-linked value.
    Format3 {
        /// `axisIndex`.
        axis_index: u16,
        /// `flags`.
        flags: u16,
        /// `valueNameID`.
        name_id: u16,
        /// `value`.
        value: Fixed,
        /// `linkedValue`.
        linked: Fixed,
    },
    /// Format 4: a combination of values on several axes.
    Format4 {
        /// `flags`.
        flags: u16,
        /// `valueNameID`.
        name_id: u16,
        /// The `(axisIndex, value)` records.
        records: &'a [[u8; 6]],
    },
    /// Another format, which HarfBuzz accepts and ignores.
    Unknown {
        /// The format.
        format: u16,
    },
}

impl AxisValue<'_> {
    /// The format.
    pub fn format(&self) -> u16 {
        match self {
            AxisValue::Format1 { .. } => 1,
            AxisValue::Format2 { .. } => 2,
            AxisValue::Format3 { .. } => 3,
            AxisValue::Format4 { .. } => 4,
            AxisValue::Unknown { format } => *format,
        }
    }

    /// `valueNameID`, `None` for an unknown format.
    pub fn name_id(&self) -> Option<u16> {
        match *self {
            AxisValue::Format1 { name_id, .. }
            | AxisValue::Format2 { name_id, .. }
            | AxisValue::Format3 { name_id, .. }
            | AxisValue::Format4 { name_id, .. } => Some(name_id),
            AxisValue::Unknown { .. } => None,
        }
    }

    /// The size of the table in bytes (2 for an unknown format).
    pub fn size(&self) -> usize {
        match self {
            AxisValue::Format1 { .. } => 12,
            AxisValue::Format2 { .. } => 20,
            AxisValue::Format3 { .. } => 16,
            AxisValue::Format4 { records, .. } => {
                8usize.saturating_add(records.len().saturating_mul(6))
            }
            AxisValue::Unknown { .. } => 2,
        }
    }

    /// The `(axisIndex, value)` pairs the table refers to: one for
    /// formats 1 to 3 (format 2 gives its nominal value), the records of
    /// format 4, none for an unknown format.
    pub fn axis_values(&self) -> impl Iterator<Item = (u16, Fixed)> + '_ {
        let single = match *self {
            AxisValue::Format1 {
                axis_index, value, ..
            }
            | AxisValue::Format3 {
                axis_index, value, ..
            } => Some((axis_index, value)),
            AxisValue::Format2 {
                axis_index,
                nominal,
                ..
            } => Some((axis_index, nominal)),
            _ => None,
        };
        let records: &[[u8; 6]] = match self {
            AxisValue::Format4 { records, .. } => records,
            _ => &[],
        };
        single.into_iter().chain(records.iter().map(|r| {
            (
                u16::from_be_bytes([r[0], r[1]]),
                Fixed(i32::from_be_bytes([r[2], r[3], r[4], r[5]])),
            )
        }))
    }
}

/// The axis value table at the start of `d`, if it fits.
fn axis_value_at(d: &[u8]) -> Option<AxisValue<'_>> {
    let format = be::u16_at(d, 0)?;
    let u = |at: usize| be::u16_at(d, at);
    let fixed = |at: usize| be::i32_at(d, at).map(Fixed);
    Some(match format {
        1 => AxisValue::Format1 {
            axis_index: u(2)?,
            flags: u(4)?,
            name_id: u(6)?,
            value: fixed(8)?,
        },
        2 => AxisValue::Format2 {
            axis_index: u(2)?,
            flags: u(4)?,
            name_id: u(6)?,
            nominal: fixed(8)?,
            min: fixed(12)?,
            max: fixed(16)?,
        },
        3 => AxisValue::Format3 {
            axis_index: u(2)?,
            flags: u(4)?,
            name_id: u(6)?,
            value: fixed(8)?,
            linked: fixed(12)?,
        },
        4 => AxisValue::Format4 {
            flags: u(4)?,
            name_id: u(6)?,
            records: be::chunks_at::<6>(d, 8, usize::from(u(2)?))?,
        },
        _ => AxisValue::Unknown { format },
    })
}

/// A view over the `STAT` table.
#[derive(Clone, Copy, Debug)]
pub struct Stat<'a> {
    data: &'a [u8],
    axes: &'a [[u8; AXIS_RECORD]],
    values_at: usize,
    value_offsets: &'a [[u8; 2]],
}

/// What the header says the table needs.
fn needed(data: &[u8]) -> usize {
    let axes_at = be::u32_at(data, 8).map_or(0, to_usize);
    let axes = be::u16_at(data, 6).map_or(0, usize::from);
    let values_at = be::u32_at(data, 14).map_or(0, to_usize);
    let values = be::u16_at(data, 12).map_or(0, usize::from);
    axes_at
        .saturating_add(axes.saturating_mul(AXIS_RECORD))
        .max(values_at.saturating_add(values.saturating_mul(2)))
        .max(HEADER)
}

impl<'a> Stat<'a> {
    /// A view over the header, the design axes and the axis value
    /// offsets.
    ///
    /// Fails with [`ReadError::UnknownVersion`] unless the version is 1.1
    /// or later in major version 1, and with [`ReadError::Truncated`]
    /// when the header or the arrays do not fit.
    pub fn new(data: &'a [u8]) -> Result<Stat<'a>, ReadError> {
        let truncated = ReadError::truncated(needed(data), data.len());
        let version = be::u32_at(data, 0).ok_or(truncated)?;
        if version >> 16 != 1 || version & 0xFFFF == 0 {
            return Err(ReadError::UnknownVersion(version));
        }
        data.first_chunk::<HEADER>().ok_or(truncated)?;
        let axes_at = be::u32_at(data, 8).map_or(0, to_usize);
        let axes = be::u16_at(data, 6).map_or(0, usize::from);
        let values_at = be::u32_at(data, 14).map_or(0, to_usize);
        let values = be::u16_at(data, 12).map_or(0, usize::from);
        Ok(Stat {
            data,
            axes: be::chunks_at::<AXIS_RECORD>(data, axes_at, axes).ok_or(truncated)?,
            values_at,
            value_offsets: be::chunks_at::<2>(data, values_at, values).ok_or(truncated)?,
        })
    }

    /// `designAxisSize`.
    pub fn design_axis_size(&self) -> u16 {
        be::u16_at(self.data, 4).unwrap_or(0)
    }

    /// `designAxisCount`.
    pub fn design_axis_count(&self) -> usize {
        self.axes.len()
    }

    /// Design axis `index`.
    pub fn design_axis(&self, index: usize) -> Option<DesignAxis> {
        let a = self.axes.get(index)?;
        Some(DesignAxis {
            tag: Tag(*be::array_at::<4>(a, 0)?),
            name_id: be::u16_at(a, 4)?,
            ordering: be::u16_at(a, 6)?,
        })
    }

    /// The design axes in order.
    pub fn design_axes(&self) -> impl Iterator<Item = DesignAxis> + '_ {
        (0..self.design_axis_count()).filter_map(|i| self.design_axis(i))
    }

    /// `axisValueCount`.
    pub fn axis_value_count(&self) -> usize {
        self.value_offsets.len()
    }

    /// The offset of axis value `index`, from the start of the offset
    /// array (0 for none).
    pub fn axis_value_offset(&self, index: usize) -> Option<u16> {
        self.value_offsets
            .get(index)
            .map(|o| u16::from_be_bytes(*o))
    }

    /// Axis value `index` and its bytes; `None` when its offset is 0 or
    /// it does not fit.
    pub fn axis_value(&self, index: usize) -> Option<(AxisValue<'a>, &'a [u8])> {
        let off = self.axis_value_offset(index)?;
        if off == 0 {
            return None;
        }
        let at = self.values_at.saturating_add(usize::from(off));
        let d = be::tail_at(self.data, at)?;
        let v = axis_value_at(d)?;
        Some((v, d.get(..v.size())?))
    }

    /// `elidedFallbackNameID`.
    pub fn elided_fallback_name_id(&self) -> u16 {
        be::u16_at(self.data, 18).unwrap_or(0)
    }
}

/// Checks a `STAT` table: an unknown version and truncation (and nothing
/// else then) and axis value tables that do not fit (at their offset).
/// Axis indices are not checked: HarfBuzz reads an out-of-range one as
/// an axis with tag 0.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let stat = match Stat::new(data) {
        Ok(s) => s,
        Err(ReadError::UnknownVersion(found)) => return emit(0, Kind::UnknownVersion { found }),
        Err(_) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed(data)),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let values_at = be::u32_at(data, 14).map_or(0, to_usize);
    for i in 0..stat.axis_value_count() {
        let off = stat.axis_value_offset(i).unwrap_or(0);
        if off == 0 {
            continue;
        }
        let at = values_at.saturating_add(usize::from(off));
        if stat.axis_value(i).is_none() {
            emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: to_u32(at),
                    length: 2,
                    available: to_u32(data.len()),
                },
            );
        }
    }
}
