//! HarfBuzz's character remappings of the cmap accelerator
//! (`src/hb-ot-cmap-table.hh`), for crates that look up cmap subtables
//! themselves, such as `sofel-subset` (ADR 0022).

/// HarfBuzz's `unicode_to_macroman`: the MacRoman code of a character,
/// looked up by the low 16 bits of the code point as HarfBuzz does.
pub fn unicode_to_macroman(c: u32) -> Option<u32> {
    crate::unicode_to_macroman(c)
}

/// HarfBuzz's `_hb_arabic_pua_simp_map`: the private-use code of a
/// character in Simplified Arabic symbol fonts, or 0.
pub fn arabic_pua_simp_map(c: u32) -> u32 {
    crate::tables::arabic_pua::simp_map(c)
}

/// HarfBuzz's `_hb_arabic_pua_trad_map`: the private-use code of a
/// character in Traditional Arabic symbol fonts, or 0.
pub fn arabic_pua_trad_map(c: u32) -> u32 {
    crate::tables::arabic_pua::trad_map(c)
}
