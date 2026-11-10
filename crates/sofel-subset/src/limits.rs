//! HarfBuzz's limits (`src/hb-limits.hh`), as used by the subsetter.

/// `HB_MAX_COMPOSITE_OPERATIONS_PER_GLYPH`.
pub const MAX_COMPOSITE_OPERATIONS_PER_GLYPH: i64 = 64;
/// `HB_SUBSET_GLYF_MAX_BYTES_FACTOR`.
pub const SUBSET_GLYF_MAX_BYTES_FACTOR: i64 = 16;
/// `HB_MAX_NESTING_LEVEL`.
pub const MAX_NESTING_LEVEL: u32 = 64;
/// `HB_SUBSET_CBDT_MAX_COPY_FACTOR`: at most this many times the source
/// CBDT is copied into the subset one.
pub const SUBSET_CBDT_MAX_COPY_FACTOR: i64 = 64;
