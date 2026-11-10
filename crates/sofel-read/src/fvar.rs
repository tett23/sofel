//! The `fvar` (font variations) table, version 1.0 (ADR 0022).
//!
//! The view accepts what HarfBuzz's `fvar::sanitize` accepts: major
//! version 1, `axisSize` 20, an `instanceSize` of at least
//! `axisCount × 4 + 4`, and axis and instance arrays inside the table.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/fvar>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Fixed, Kind, ReadError, Tag};

/// The `fvar` tag.
pub const TAG: Tag = Tag::new(b"fvar");

/// The size of the header.
const HEADER: usize = 16;
/// The size of an axis record (the only `axisSize` accepted).
const AXIS_SIZE: usize = 20;

/// A variation axis record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VariationAxis {
    /// `axisTag`.
    pub tag: Tag,
    /// `minValue`.
    pub min: Fixed,
    /// `defaultValue`.
    pub default: Fixed,
    /// `maxValue`.
    pub max: Fixed,
    /// `flags` (bit 0: hidden).
    pub flags: u16,
    /// `axisNameID`.
    pub name_id: u16,
}

impl VariationAxis {
    /// The axis value range with the order HarfBuzz enforces
    /// (`AxisRecord::get_coordinates`): `(min(min, default), default,
    /// max(max, default))`.
    pub fn ordered(&self) -> (Fixed, Fixed, Fixed) {
        (
            Fixed(self.min.0.min(self.default.0)),
            self.default,
            Fixed(self.max.0.max(self.default.0)),
        )
    }

    /// Whether the axis is hidden (flag bit 0).
    pub fn is_hidden(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// A named instance record.
#[derive(Clone, Copy, Debug)]
pub struct InstanceRecord<'a> {
    /// `subfamilyNameID`.
    pub subfamily_name_id: u16,
    /// `flags`.
    pub flags: u16,
    coordinates: &'a [[u8; 4]],
    /// `postScriptNameID`, when the record is long enough to have it.
    pub postscript_name_id: Option<u16>,
}

impl InstanceRecord<'_> {
    /// The number of coordinates (the axis count).
    pub fn num_coordinates(&self) -> usize {
        self.coordinates.len()
    }

    /// The coordinate of axis `index`.
    pub fn coordinate(&self, index: usize) -> Option<Fixed> {
        self.coordinates
            .get(index)
            .map(|c| Fixed(i32::from_be_bytes(*c)))
    }
}

/// A view over the `fvar` table.
#[derive(Clone, Copy, Debug)]
pub struct Fvar<'a> {
    data: &'a [u8],
    axes: &'a [[u8; AXIS_SIZE]],
    instances_at: usize,
    instance_count: usize,
    instance_size: usize,
}

/// What the header says the table needs.
fn needed(data: &[u8]) -> usize {
    let first = be::u16_at(data, 4).map_or(0, usize::from);
    let axes = be::u16_at(data, 8).map_or(0, usize::from);
    let instances = be::u16_at(data, 12).map_or(0, usize::from);
    let size = be::u16_at(data, 14).map_or(0, usize::from);
    first
        .saturating_add(axes.saturating_mul(AXIS_SIZE))
        .saturating_add(instances.saturating_mul(size))
        .max(HEADER)
}

impl<'a> Fvar<'a> {
    /// A view over the header, the axes and the instances.
    ///
    /// Fails with [`ReadError::UnknownVersion`] unless the major version
    /// is 1, with [`ReadError::InvalidValue`] when `axisSize` is not 20 or
    /// `instanceSize` is below `axisCount × 4 + 4`, and with
    /// [`ReadError::Truncated`] when the header or the arrays do not fit.
    pub fn new(data: &'a [u8]) -> Result<Fvar<'a>, ReadError> {
        let truncated = ReadError::truncated(needed(data), data.len());
        let h = data.first_chunk::<HEADER>().ok_or(truncated)?;
        let u16_of = |i: usize| be::u16_at(h, i).map_or(0, usize::from);
        if u16_of(0) != 1 {
            return Err(ReadError::UnknownVersion(u32::from_be_bytes([
                h[0], h[1], h[2], h[3],
            ])));
        }
        let first = u16_of(4);
        let axis_count = u16_of(8);
        let axis_size = u16_of(10);
        let instance_count = u16_of(12);
        let instance_size = u16_of(14);
        if axis_size != AXIS_SIZE || instance_size < axis_count.saturating_mul(4).saturating_add(4)
        {
            return Err(ReadError::InvalidValue);
        }
        let axes = be::chunks_at::<AXIS_SIZE>(data, first, axis_count).ok_or(truncated)?;
        let instances_at = first.saturating_add(axis_count.saturating_mul(AXIS_SIZE));
        be::slice_at(
            data,
            instances_at,
            instance_count.saturating_mul(instance_size),
        )
        .ok_or(truncated)?;
        Ok(Fvar {
            data,
            axes,
            instances_at,
            instance_count,
            instance_size,
        })
    }

    /// `axisCount`.
    pub fn axis_count(&self) -> usize {
        self.axes.len()
    }

    /// Axis `index`.
    pub fn axis(&self, index: usize) -> Option<VariationAxis> {
        let a = self.axes.get(index)?;
        let fixed = |at: usize| be::i32_at(a, at).map_or(Fixed(0), Fixed);
        Some(VariationAxis {
            tag: Tag(*be::array_at::<4>(a, 0)?),
            min: fixed(4),
            default: fixed(8),
            max: fixed(12),
            flags: be::u16_at(a, 16)?,
            name_id: be::u16_at(a, 18)?,
        })
    }

    /// The axes in order.
    pub fn axes(&self) -> impl Iterator<Item = VariationAxis> + '_ {
        (0..self.axis_count()).filter_map(|i| self.axis(i))
    }

    /// The index and record of the first axis tagged `tag`
    /// (`hb_ot_var_find_axis_info`).
    pub fn find_axis(&self, tag: Tag) -> Option<(usize, VariationAxis)> {
        self.axes().enumerate().find(|(_, a)| a.tag == tag)
    }

    /// `instanceCount`.
    pub fn instance_count(&self) -> usize {
        self.instance_count
    }

    /// `instanceSize`.
    pub fn instance_size(&self) -> usize {
        self.instance_size
    }

    /// Instance `index`.
    pub fn instance(&self, index: usize) -> Option<InstanceRecord<'a>> {
        if index >= self.instance_count {
            return None;
        }
        let at = self
            .instances_at
            .saturating_add(index.saturating_mul(self.instance_size));
        let rec = be::slice_at(self.data, at, self.instance_size)?;
        let n = self.axis_count();
        let coordinates = be::chunks_at::<4>(rec, 4, n)?;
        let ps_at = n.saturating_mul(4).saturating_add(4);
        let postscript_name_id = if self.instance_size >= ps_at.saturating_add(2) {
            be::u16_at(rec, ps_at)
        } else {
            None
        };
        Some(InstanceRecord {
            subfamily_name_id: be::u16_at(rec, 0)?,
            flags: be::u16_at(rec, 2)?,
            coordinates,
            postscript_name_id,
        })
    }
}

/// Checks an `fvar` table: an unknown version, a wrong `axisSize` or
/// `instanceSize` (at the field) and truncation. Axes whose minimum is
/// above the default or whose maximum is below it are not reported:
/// HarfBuzz reorders them (`AxisRecord::get_coordinates`).
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    match Fvar::new(data) {
        Ok(_) => {}
        Err(ReadError::UnknownVersion(found)) => emit(0, Kind::UnknownVersion { found }),
        Err(ReadError::InvalidValue) => {
            let axes = be::u16_at(data, 8).map_or(0, u32::from);
            let axis_size = be::u16_at(data, 10).map_or(0, u32::from);
            if to_usize(axis_size) != AXIS_SIZE {
                emit(
                    10,
                    Kind::HeaderSizeTooSmall {
                        found: axis_size,
                        minimum: to_u32(AXIS_SIZE),
                    },
                );
            } else {
                emit(
                    14,
                    Kind::HeaderSizeTooSmall {
                        found: be::u16_at(data, 14).map_or(0, u32::from),
                        minimum: axes.saturating_mul(4).saturating_add(4),
                    },
                );
            }
        }
        Err(_) => emit(
            0,
            Kind::Truncated {
                needed: to_u32(needed(data)),
                available: to_u32(data.len()),
            },
        ),
    }
}
