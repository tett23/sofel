//! Code that does not allocate (sofel ADR 0098): the readers shared by the
//! parsers and the validators of sofel ADR 0096.
//!
//! Nothing here names an allocating type or calls the crate's other
//! modules; `scripts/check-deps.sh` checks both. The parsers outside call
//! the readers here and keep what they pass, so a validator and its parser
//! read a format the same way.

/// BDF faces.
pub mod bdf;
/// PCF faces.
pub mod pcf;
/// Windows FNT and NE and PE32 FON faces.
pub mod winfnt;

/// One problem found by a validator: where it is and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The face in a FON collection, or `None` for a single face.
    pub face: Option<u32>,
    /// The byte offset in the validated file.
    pub offset: usize,
    /// A description for people.
    pub reason: &'static str,
}

/// The values of one glyph read by a scanner; its pixels are passed apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GlyphHead {
    pub(crate) encoding: Option<u32>,
    pub(crate) advance: i32,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) left: i16,
    pub(crate) top: i16,
}

/// What a BDF or FNT scanner passes on: the pixel bytes of a glyph, and
/// then the glyph.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Event {
    Pixel(u8),
    Glyph(GlyphHead),
}
