//! Per-load FreeType module properties for hinting (ADR 0086).

/// The PostScript-style stem darkening settings of one FreeType module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostScriptHintProperties {
    /// Whether the module skips stem darkening.
    pub no_stem_darkening: bool,
    /// Four (stem width, darkening amount) pairs in FreeType's units.
    pub darkening_parameters: [i32; 8],
}

impl Default for PostScriptHintProperties {
    fn default() -> Self {
        Self {
            no_stem_darkening: true,
            darkening_parameters: [500, 400, 1000, 275, 1667, 275, 2333, 0],
        }
    }
}

impl PostScriptHintProperties {
    fn validate(self) -> Result<(), HintPropertyError> {
        let points = self.darkening_parameters.as_chunks::<2>().0;
        if points
            .iter()
            .any(|p| p[0] < 0 || !(0..=500).contains(&p[1]))
            || !points.iter().map(|point| point[0]).is_sorted()
        {
            return Err(HintPropertyError::InvalidDarkeningParameters);
        }
        Ok(())
    }
}

/// Module settings supplied to a hinting load, independently for each call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HintProperties {
    /// Automatic hinter stem darkening.
    pub autofitter: PostScriptHintProperties,
    /// CFF stem darkening.
    pub cff: PostScriptHintProperties,
    /// Type 1 stem darkening.
    pub type1: PostScriptHintProperties,
    /// CID-keyed Type 1 stem darkening.
    pub t1cid: PostScriptHintProperties,
    /// Maximum ppem for the automatic hinter to increase x-height; zero disables it.
    pub increase_x_height: u32,
    /// FreeType's AF_Script index for uncovered glyphs.
    pub fallback_script: Option<u32>,
    /// FreeType's AF_Script index for default OpenType features.
    pub default_script: Option<u32>,
}

/// A FreeType module property rejected before a glyph is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintPropertyError {
    /// A darkening curve has a negative value, a y value above 500, or decreasing x values.
    InvalidDarkeningParameters,
    /// No default-coverage style exists for the requested fallback script.
    InvalidFallbackScript,
}

impl HintProperties {
    /// Checks the same darkening-parameter bounds and ordering as `FT_Property_Set`.
    pub fn validate(self) -> Result<(), HintPropertyError> {
        self.autofitter.validate()?;
        self.cff.validate()?;
        self.type1.validate()?;
        self.t1cid.validate()?;
        Ok(())
    }
}
