//! The script-specific shapers of ADR 0024, each a [`ShaperFuncs`] of
//! HarfBuzz's hooks (`hb_ot_shaper_t`).
//!
//! [`ShaperFuncs`]: crate::plan::ShaperFuncs

pub(crate) mod hangul;
pub(crate) mod hebrew;
pub(crate) mod thai;
