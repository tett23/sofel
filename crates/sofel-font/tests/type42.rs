#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use sofel_font::Font;
use sofel_type1::type42::Type42Source;

fn wrap(sfnt: &[u8]) -> Vec<u8> {
    let mut out = b"%!PS-TrueTypeFont-1.0\n/FontType 42 def\n/sfnts [\n".to_vec();
    let cut = (sfnt.len() / 2) & !1;
    for chunk in [&sfnt[..cut], &sfnt[cut..]] {
        out.push(b'<');
        for byte in chunk {
            out.extend_from_slice(format!("{byte:02X}").as_bytes());
        }
        out.extend_from_slice(b">\n");
    }
    out.extend_from_slice(b"] def\n");
    out
}

#[test]
fn adr0065_i1_type42_embedded_sfnt_opens_as_font() {
    let inner = build::sample_font();
    let outer = wrap(&inner);
    let source = Type42Source::parse(&outer, inner.len()).unwrap();
    let actual = Font::from_type42(&source, 0).unwrap();
    let expected = Font::new(&inner, 0).unwrap();
    assert_eq!(actual.num_glyphs(), expected.num_glyphs());
    assert_eq!(actual.units_per_em(), expected.units_per_em());
    assert_eq!(
        actual.nominal_glyph('A' as u32),
        expected.nominal_glyph('A' as u32)
    );
}
