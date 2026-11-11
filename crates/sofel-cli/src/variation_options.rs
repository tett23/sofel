//! Shared parsing of variation options for the command-line tools.

use sofel_outline::font::var::Variation;

/// Parses design coordinates in command-line order, including repeated axes.
pub fn parse_variations(input: &str) -> Option<Vec<Variation>> {
    if input.is_empty() {
        return Some(Vec::new());
    }
    input.split(',').map(Variation::parse).collect()
}

/// Parses a zero-based named instance number.
pub fn parse_named_instance(input: &str) -> Option<u16> {
    input.parse().ok()
}

/// Location settings shared by the paint and render argument parsers.
#[derive(Default)]
pub(crate) struct LocationOptions {
    named: Option<u16>,
    variations: Vec<Variation>,
}

impl LocationOptions {
    pub(crate) fn consume(
        &mut self,
        argument: Option<&str>,
        next: &mut impl FnMut(&str) -> Result<String, String>,
    ) -> Result<bool, String> {
        let Some(argument) = argument else {
            return Ok(false);
        };
        let (name, inline) = argument
            .split_once('=')
            .map_or((argument, None), |(n, v)| (n, Some(v)));
        if !matches!(name, "--variations" | "--named-instance") {
            return Ok(false);
        }
        let text = match inline {
            Some(s) => s.to_owned(),
            None => next(name)?,
        };
        match name {
            "--variations" => self
                .variations
                .extend(parse_variations(&text).ok_or("bad --variations")?),
            _ => self.named = Some(parse_named_instance(&text).ok_or("bad --named-instance")?),
        }
        Ok(true)
    }

    pub(crate) fn resolve<'d>(
        &self,
        font: &sofel_outline::font::Font<'d>,
    ) -> sofel_outline::font::var::Location<'d> {
        sofel_outline::font::var::Location::new(font, self.named, &self.variations)
    }
}
