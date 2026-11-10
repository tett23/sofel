//! Where the test data, the UCD files, `ft-dump` and `hb-dump` are.
//! `SOFEL_TESTDATA`, `SOFEL_UCD`, `SOFEL_FT_DUMP` and `SOFEL_HB_DUMP`
//! override the defaults under `target/`.

use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn testdata() -> PathBuf {
    std::env::var_os("SOFEL_TESTDATA").map_or_else(|| root().join("target/testdata"), PathBuf::from)
}

/// The UCD files fetched by `scripts/fetch-ucd.sh` (ADR 0019); `SOFEL_UCD`
/// overrides `target/ucd`.
pub fn ucd() -> PathBuf {
    std::env::var_os("SOFEL_UCD").map_or_else(|| root().join("target/ucd"), PathBuf::from)
}

pub fn ft_dump() -> PathBuf {
    std::env::var_os("SOFEL_FT_DUMP")
        .map_or_else(|| root().join("target/ft-dump/ft-dump"), PathBuf::from)
}

pub fn hb_dump() -> PathBuf {
    std::env::var_os("SOFEL_HB_DUMP")
        .map_or_else(|| root().join("target/hb-dump/hb-dump"), PathBuf::from)
}

pub fn conformance_file(name: &str) -> PathBuf {
    root().join("conformance").join(name)
}

/// Fails with the steps to get what is missing.
pub fn require(path: &Path, what: &str) -> Result<(), String> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!(
            "{what} not found at {}.\nRun:\n  scripts/fetch-testdata.sh\n  tools/ft-dump/build.sh\n  tools/hb-dump/build.sh",
            path.display()
        ))
    }
}
