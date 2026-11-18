//! Type 1 Multiple Masters axis metadata and design maps.

use crate::noalloc::mm::{AxisInfo, interpolate, points, scan};
use alloc::vec::Vec;

pub use crate::noalloc::mm::MmError;

/// One design axis, with coordinates in signed 16.16 fixed point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Axis<'a> {
    name: &'a [u8],
    map: Vec<(i32, i32)>,
    default: i32,
}

impl Axis<'_> {
    /// PostScript axis name.
    pub fn name(&self) -> &[u8] {
        self.name
    }
    /// Minimum design coordinate.
    pub fn minimum(&self) -> i32 {
        self.map.first().map_or(0, |point| point.0)
    }
    /// Maximum design coordinate.
    pub fn maximum(&self) -> i32 {
        self.map.last().map_or(0, |point| point.0)
    }
    /// Default design coordinate.
    pub fn default(&self) -> i32 {
        self.default
    }

    fn blend(&self, design: i32) -> Result<i32, MmError> {
        let design = design.clamp(self.minimum(), self.maximum());
        for pair in self.map.windows(2) {
            let (&left, &right) = (pair.first().ok_or(MmError)?, pair.get(1).ok_or(MmError)?);
            if design <= right.0 {
                return interpolate(design, left.0, right.0, left.1, right.1);
            }
        }
        self.map.last().map(|point| point.1).ok_or(MmError)
    }
}

/// Validated axis maps and default master weights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipleMasters<'a> {
    axes: Vec<Axis<'a>>,
    default_weights: Vec<i32>,
}

impl<'a> MultipleMasters<'a> {
    /// Parse MM dictionaries in the cleartext Type 1 program. A non-MM font returns `None`.
    ///
    /// Allocates the axes, their maps and the default weights (ADR 0094).
    pub fn parse(data: &'a [u8]) -> Result<Option<Self>, MmError> {
        let mut default_weights = Vec::new();
        let mut found = Vec::new();
        if !scan(
            data,
            &mut |weight| default_weights.push(weight),
            &mut |axis| found.push(axis),
        )? {
            return Ok(None);
        }
        let axes = found
            .into_iter()
            .map(|AxisInfo { name, map, default }| {
                Ok(Axis {
                    name,
                    map: points(map)?.collect::<Result<_, MmError>>()?,
                    default,
                })
            })
            .collect::<Result<_, MmError>>()?;
        Ok(Some(Self {
            axes,
            default_weights,
        }))
    }

    /// Number of design axes.
    pub fn axis_count(&self) -> usize {
        self.axes.len()
    }
    /// An axis by index.
    pub fn axis(&self, index: usize) -> Option<&Axis<'a>> {
        self.axes.get(index)
    }
    /// Default master weights in signed 16.16 fixed point.
    pub fn default_weights(&self) -> &[i32] {
        &self.default_weights
    }
    /// Convert design coordinates to normalized blend coordinates.
    ///
    /// Allocates the returned coordinates (ADR 0094).
    pub fn blend_coordinates(&self, design: &[i32]) -> Result<Vec<i32>, MmError> {
        if design.len() != self.axes.len() {
            return Err(MmError);
        }
        self.axes
            .iter()
            .zip(design)
            .map(|(axis, coordinate)| axis.blend(*coordinate))
            .collect()
    }

    /// Interpolation weights in Type 1 master order for design coordinates.
    ///
    /// Allocates the returned weights (ADR 0094).
    pub fn master_weights(&self, design: &[i32]) -> Result<Vec<i32>, MmError> {
        let blends = self.blend_coordinates(design)?;
        let mut weights = Vec::with_capacity(self.default_weights.len());
        for master in 0..self.default_weights.len() {
            let mut weight = 1 << 16;
            for (axis, blend) in blends.iter().enumerate() {
                let factor = if master & (1 << axis) == 0 {
                    (1i32 << 16).checked_sub(*blend).ok_or(MmError)?
                } else {
                    *blend
                };
                if factor <= 0 {
                    weight = 0;
                    break;
                }
                if factor >= 1 << 16 {
                    continue;
                }
                weight = i32::try_from(
                    i64::from(weight)
                        .checked_mul(i64::from(factor))
                        .and_then(|n| n.checked_add(32768))
                        .ok_or(MmError)?
                        >> 16,
                )
                .map_err(|_| MmError)?;
            }
            weights.push(weight);
        }
        Ok(weights)
    }
}
