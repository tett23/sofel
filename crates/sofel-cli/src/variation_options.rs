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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::{parse_named_instance, parse_variations};

    #[test]
    fn adr0027_i_cli_variations_keep_order_and_reject_invalid_values() {
        let variations = parse_variations("wght=500,wdth=75,wght=700").unwrap();
        assert_eq!(variations.len(), 3);
        assert_eq!(variations[0].value, 500.0);
        assert_eq!(variations[2].value, 700.0);
        assert_eq!(parse_variations("").unwrap().len(), 0);
        assert!(parse_variations("wght=500,").is_none());
        assert!(parse_variations("wght=bad").is_none());
    }

    #[test]
    fn adr0027_i_cli_named_instance_boundaries() {
        assert_eq!(parse_named_instance("0"), Some(0));
        assert_eq!(parse_named_instance("65535"), Some(u16::MAX));
        assert_eq!(parse_named_instance("65536"), None);
        assert_eq!(parse_named_instance("-1"), None);
    }
}
