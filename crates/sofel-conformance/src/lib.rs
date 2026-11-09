//! Conformance tests of sofel against FreeType and HarfBuzz (ADR 0005,
//! ADR 0008). The tests need the data fetched by
//! `scripts/fetch-testdata.sh` and the `ft-dump` built by
//! `tools/ft-dump/build.sh`; see [`paths`].

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

pub mod compare;
pub mod dump;
pub mod expect;
pub mod fonts;
pub mod outline;
pub mod paths;
