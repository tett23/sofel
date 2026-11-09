//! Where the test data and `ft-dump` are. `SOFEL_TESTDATA` and
//! `SOFEL_FT_DUMP` override the defaults under `target/`.

use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn testdata() -> PathBuf {
    std::env::var_os("SOFEL_TESTDATA").map_or_else(|| root().join("target/testdata"), PathBuf::from)
}

pub fn ft_dump() -> PathBuf {
    std::env::var_os("SOFEL_FT_DUMP")
        .map_or_else(|| root().join("target/ft-dump/ft-dump"), PathBuf::from)
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
            "{what} not found at {}.\nRun:\n  scripts/fetch-testdata.sh\n  tools/ft-dump/build.sh",
            path.display()
        ))
    }
}
