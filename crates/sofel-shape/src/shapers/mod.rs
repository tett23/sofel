//! The script-specific shapers of ADR 0024, each a [`ShaperFuncs`] of
//! HarfBuzz's hooks (`hb_ot_shaper_t`).
//!
//! [`ShaperFuncs`]: crate::plan::ShaperFuncs

pub(crate) mod arabic;
pub(crate) mod arabic_fallback;
pub(crate) mod hangul;
pub(crate) mod hebrew;
pub(crate) mod indic;
pub(crate) mod syllabic;
pub(crate) mod thai;
pub(crate) mod use_;
pub(crate) mod vowel_constraints;
