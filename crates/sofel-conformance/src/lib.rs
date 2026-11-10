//! Conformance tests of sofel against FreeType and HarfBuzz (ADR 0005,
//! ADR 0008). The tests need the data fetched by
//! `scripts/fetch-testdata.sh`, the `ft-dump` built by
//! `tools/ft-dump/build.sh` and the `hb-dump` built by
//! `tools/hb-dump/build.sh`, and the UCD files fetched by
//! `scripts/fetch-ucd.sh`; see [`paths`].

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

pub mod cff_info;
pub mod color;
pub mod compare;
pub mod dump;
pub mod expect;
pub mod fonts;
pub mod hb;
pub mod outline;
pub mod paths;
pub mod raster;
pub mod unicode;
