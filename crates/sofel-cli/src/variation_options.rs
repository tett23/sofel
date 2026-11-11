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
