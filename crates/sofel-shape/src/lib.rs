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

mod buffer;
mod diff;
mod digest;
mod direction;
mod fallback;
mod feature;
mod font;
mod glyph;
mod language;
mod layout;
#[allow(dead_code)] // Used by the shapers of ADR 0024 as they land.
mod machine;
mod normalize;
mod plan;
mod props;
mod scale;
mod shape;
mod shapers;
mod tables;
mod tag;
pub mod unicode;
mod utf;
#[rustfmt::skip]
mod tag_table;
mod serialize;

pub use buffer::{
    BufferFlags, ClusterLevel, GlyphBuffer, MAX_LEN_DEFAULT, MAX_LEN_FACTOR, MAX_LEN_MIN,
    MAX_OPS_DEFAULT, MAX_OPS_FACTOR, MAX_OPS_MIN, REPLACEMENT_CHARACTER, UnicodeBuffer,
};
pub use diff::{DiffFlags, diff, diff_unicode};
pub use direction::Direction;
pub use feature::Feature;
pub use glyph::{GlyphFlags, GlyphInfo, GlyphPosition};
pub use language::Language;
pub use plan::{LayoutSelection, ShapePlan, Shaper, layout_selection};
pub use props::{ResolvedProperties, SegmentProperties, script_horizontal_direction};
pub use scale::Scale;
pub use serialize::{
    DeserializeError, EmptyFont, SerializeFlags, SerializeFont, SerializeFormat,
    deserialize_glyphs, deserialize_unicode, glyph_from_string, glyph_to_string, serialize_glyphs,
    serialize_glyphs_range, serialize_invalid, serialize_unicode, serialize_unicode_range,
};
pub use shape::{shape, shape_fallback, shape_with_extents};
pub use sofel_unicode::Script;
pub use tag::{
    MAX_TAGS_PER_LANGUAGE, MAX_TAGS_PER_SCRIPT, OT_TAG_DEFAULT_LANGUAGE, OT_TAG_DEFAULT_SCRIPT,
    ot_tag_to_language, ot_tag_to_script, ot_tags_from_script_and_language,
    ot_tags_from_script_and_language_max, ot_tags_to_script_and_language,
};
