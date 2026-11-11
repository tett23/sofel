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

#[cfg(test)]
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
