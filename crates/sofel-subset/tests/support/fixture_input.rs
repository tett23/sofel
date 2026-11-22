//! Optional export of synthetic source fonts for rebuilding reference fixtures.
pub fn export(name: &str, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(directory) = std::env::var_os("SOFEL_FIXTURE_INPUTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join(name), bytes)?;
    }
    Ok(())
}
