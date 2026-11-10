//! Text shaping as HarfBuzz 15.1.0's OpenType shaper does it (ADR 0020):
//! a [`UnicodeBuffer`] of characters goes in, a [`GlyphBuffer`] of glyph
//! ids, clusters, flags and positions comes out.
//!
//! The caller builds a [`ShapePlan`] once per font, segment properties
//! and feature set, keeps it, and passes it to [`shape`] for every run of
//! text. Plans are immutable and may be shared between threads; buffers
//! are owned by one thread at a time and are handed back and forth so
//! that their memory is reused.
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372) and HarfRust 0.15.0, both
//! under the MIT license; see THIRD_PARTY_NOTICES.md.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod direction;
mod feature;
mod glyph;
mod language;

pub use direction::Direction;
pub use feature::Feature;
pub use glyph::{GlyphFlags, GlyphInfo, GlyphPosition};
pub use language::Language;
pub use sofel_unicode::Script;
