//! Preprocessing ([`Prepared`]): what HarfBuzz's `hb_subset_preprocess`
//! attaches to a face (`hb_subset_accelerator_t`,
//! `src/hb-subset-accelerator.hh`), computed once and shared by the plans
//! made with [`Plan::with_prepared`](crate::Plan::with_prepared).
//!
//! HarfBuzz preprocesses by subsetting the face with `keep_everything`
//! and attaching the accelerator to the result, which later subsets read
//! instead of the source. sofel keeps the source font and computes the
//! accelerator's contents directly from it: the character mapping the
//! `keep_everything` plan collects, its reverse, whether a glyph has a
//! `seac`, the parsed CFF programs of every glyph (with their hints
//! marked) and the characters of each cmap subtable. A plan made with
//! them takes HarfBuzz's accelerator paths and gives the same bytes as
//! one made without.

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;

use crate::cmap_accel::{CmapAccel, CmapTable};
use crate::input::{Flags, Input, SetKind};
use crate::plan::{populate_unicodes, unicode_closure};
use crate::set::RangeSet;
use crate::tables::cff::{self, CffCache, SeacFinder};
use crate::tables::cmap;

/// A font's preprocessed data (`hb_subset_preprocess`): immutable,
/// cheap to clone (the data is shared), and shareable between threads.
/// Use it only with the font it was made from; [`Plan::with_prepared`]
/// refuses a font whose glyph count or character-mapping and CFF table
/// lengths differ.
///
/// [`Plan::with_prepared`]: crate::Plan::with_prepared
#[derive(Clone)]
pub struct Prepared(Arc<Accel>);

/// The tables a [`Prepared`] value was made from, as far as it checks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Key {
    num_glyphs: u32,
    cmap: Option<usize>,
    dmap: Option<usize>,
    cff: Option<usize>,
}

impl Key {
    pub(crate) fn of(font: &Font<'_>) -> Key {
        let len = |t: &[u8; 4]| font.table(Tag::new(t)).map(<[u8]>::len);
        Key {
            num_glyphs: font.num_glyphs(),
            cmap: len(b"cmap"),
            dmap: len(b"DMAP"),
            cff: len(b"CFF "),
        }
    }
}

/// The contents of `hb_subset_accelerator_t`.
pub(crate) struct Accel {
    pub key: Key,
    /// `unicode_to_gid`: the `keep_everything` plan's characters and
    /// their glyphs.
    pub unicode_to_gid: BTreeMap<u32, u32>,
    /// `gid_to_unicodes`: the reverse of `unicode_to_gid`.
    pub gid_to_unicodes: BTreeMap<u32, Vec<u32>>,
    /// `unicodes`: the characters of `unicode_to_gid` and the variation
    /// selectors.
    pub unicodes: RangeSet,
    /// Whether a glyph has a `seac` (`has_seac`).
    pub has_seac: bool,
    /// The parsed CFF programs (`cff1_accel`'s `cff_accelerator`).
    pub cff: Option<CffCache>,
    /// The characters of each kept cmap subtable by its offset
    /// (`cmap_cache`, `SubtableUnicodesCache::create_filled_cache`).
    pub cmap_sets: BTreeMap<u32, RangeSet>,
}

impl Prepared {
    /// Preprocesses `font` (`hb_subset_preprocess`). This takes about as
    /// long as parsing every glyph's CFF program and reading the whole
    /// character mapping; it never fails: what cannot be computed (a CFF
    /// table HarfBuzz does not read, a program that does not parse) is
    /// left out, and plans then compute it as without preprocessing.
    pub fn new(font: &Font<'_>) -> Prepared {
        let everything = Input::new().keep_everything();
        let cmap = CmapAccel::new(font);
        let unicodes = unicode_closure(
            everything.set(SetKind::Unicodes),
            !everything.flags().has(Flags::NO_BIDI_CLOSURE),
        );
        let u = populate_unicodes(
            &cmap,
            &unicodes,
            everything.set(SetKind::Glyphs),
            font.num_glyphs(),
            None,
        );
        let mut gid_to_unicodes: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (&cp, &gid) in &u.codepoint_to_glyph {
            gid_to_unicodes.entry(gid).or_default().push(cp);
        }
        // _populate_gids_to_retain with every glyph kept: the accelerator
        // still records whether any glyph has a seac.
        let has_seac = SeacFinder::new(font)
            .is_some_and(|f| (0..font.num_glyphs()).any(|g| f.components(g).is_some()));
        let cmap_sets = font
            .table(Tag::new(b"cmap"))
            .and_then(CmapTable::new)
            .map(|t| cmap::filled_cache(&t))
            .unwrap_or_default();
        Prepared(Arc::new(Accel {
            key: Key::of(font),
            unicode_to_gid: u.codepoint_to_glyph,
            gid_to_unicodes,
            unicodes: u.unicodes,
            has_seac,
            cff: cff::prepare(font),
            cmap_sets,
        }))
    }

    pub(crate) fn accel(&self) -> &Accel {
        &self.0
    }

    pub(crate) fn cff(&self) -> Option<&CffCache> {
        self.0.cff.as_ref()
    }
}

impl core::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let a = &self.0;
        f.debug_struct("Prepared")
            .field("num_glyphs", &a.key.num_glyphs)
            .field("unicodes", &a.unicode_to_gid.len())
            .field("has_seac", &a.has_seac)
            .field("cff", &a.cff.is_some())
            .finish()
    }
}
