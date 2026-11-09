use core::fmt;

use crate::Tag;

/// How bad a [`Diagnostic`] is (ADR 0007).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Severity {
    /// The table or part cannot be built as a view, or reading it gives
    /// results the specification does not intend.
    Error,
    /// The data violates the specification, but reading is not affected.
    Warning,
}

/// One problem found by a validator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    /// The table. `sfnt` is the table directory, `ttcf` the collection header.
    pub table: Tag,
    /// Offset from the start of the table (or directory, or file).
    pub offset: u32,
    pub severity: Severity,
    pub kind: Kind,
}

impl Diagnostic {
    pub(crate) fn new(table: Tag, offset: u32, kind: Kind) -> Diagnostic {
        Diagnostic {
            table,
            offset,
            severity: kind.severity(),
            kind,
        }
    }
}

/// A value attached to a [`Kind`], for machine-readable output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailValue {
    Uint(u32),
    Int(i32),
    Tag(Tag),
}

/// What a [`Diagnostic`] is about. [`Kind::id`] and the keys passed by
/// [`Kind::details`] are part of the `sofel-validate --json` output
/// contract (ADR 0008).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Truncated {
        needed: u32,
        available: u32,
    },
    UnknownVersion {
        found: u32,
    },
    UnknownFormat {
        found: u32,
    },
    OffsetOutOfBounds {
        offset: u32,
        length: u32,
        available: u32,
    },
    RecordsNotSorted {
        index: u32,
    },
    DuplicateTable {
        tag: Tag,
    },
    ChecksumMismatch {
        stored: u32,
        computed: u32,
    },
    BadMagicNumber {
        expected: u32,
        found: u32,
    },
    UnitsPerEmOutOfRange {
        found: u32,
    },
    InvalidIndexToLocFormat {
        found: i32,
    },
    ZeroLongMetrics,
    MetricsExceedGlyphs {
        metrics: u32,
        glyphs: u32,
    },
    SegCountOdd {
        found: u32,
    },
    SearchParametersInvalid,
    LastSegmentNotFfff {
        found: u32,
    },
    StartAfterEnd {
        index: u32,
        start: u32,
        end: u32,
    },
    SegmentsNotSorted {
        index: u32,
    },
    SegmentsOverlap {
        index: u32,
    },
    RangeOffsetOutOfBounds {
        index: u32,
    },
    RangeOffsetFfff {
        index: u32,
    },
    GlyphIdOutOfRange {
        glyph: u32,
        num_glyphs: u32,
    },
    GroupsNotSorted {
        index: u32,
    },
    CodepointOutOfRange {
        codepoint: u32,
    },
    SelectorsNotSorted {
        index: u32,
    },
    SubHeaderKeyNotAligned {
        index: u32,
        found: u32,
    },
    SubHeaderRangeInvalid {
        index: u32,
    },
    Is32Mismatch {
        codepoint: u32,
    },
    LengthMismatch {
        declared: u32,
        available: u32,
    },
}

impl Kind {
    pub fn severity(&self) -> Severity {
        use Kind::*;
        match self {
            Truncated { .. }
            | UnknownVersion { .. }
            | UnknownFormat { .. }
            | OffsetOutOfBounds { .. }
            | InvalidIndexToLocFormat { .. }
            | ZeroLongMetrics
            | StartAfterEnd { .. }
            | SegmentsNotSorted { .. }
            | RangeOffsetOutOfBounds { .. }
            | RangeOffsetFfff { .. }
            | GroupsNotSorted { .. }
            | CodepointOutOfRange { .. }
            | SelectorsNotSorted { .. } => Severity::Error,
            RecordsNotSorted { .. }
            | DuplicateTable { .. }
            | ChecksumMismatch { .. }
            | BadMagicNumber { .. }
            | UnitsPerEmOutOfRange { .. }
            | MetricsExceedGlyphs { .. }
            | SegCountOdd { .. }
            | SearchParametersInvalid
            | LastSegmentNotFfff { .. }
            | SegmentsOverlap { .. }
            | GlyphIdOutOfRange { .. }
            | SubHeaderKeyNotAligned { .. }
            | SubHeaderRangeInvalid { .. }
            | Is32Mismatch { .. }
            | LengthMismatch { .. } => Severity::Warning,
        }
    }

    /// A stable snake_case identifier.
    pub fn id(&self) -> &'static str {
        use Kind::*;
        match self {
            Truncated { .. } => "truncated",
            UnknownVersion { .. } => "unknown_version",
            UnknownFormat { .. } => "unknown_format",
            OffsetOutOfBounds { .. } => "offset_out_of_bounds",
            RecordsNotSorted { .. } => "records_not_sorted",
            DuplicateTable { .. } => "duplicate_table",
            ChecksumMismatch { .. } => "checksum_mismatch",
            BadMagicNumber { .. } => "bad_magic_number",
            UnitsPerEmOutOfRange { .. } => "units_per_em_out_of_range",
            InvalidIndexToLocFormat { .. } => "invalid_index_to_loc_format",
            ZeroLongMetrics => "zero_long_metrics",
            MetricsExceedGlyphs { .. } => "metrics_exceed_glyphs",
            SegCountOdd { .. } => "seg_count_odd",
            SearchParametersInvalid => "search_parameters_invalid",
            LastSegmentNotFfff { .. } => "last_segment_not_ffff",
            StartAfterEnd { .. } => "start_after_end",
            SegmentsNotSorted { .. } => "segments_not_sorted",
            SegmentsOverlap { .. } => "segments_overlap",
            RangeOffsetOutOfBounds { .. } => "range_offset_out_of_bounds",
            RangeOffsetFfff { .. } => "range_offset_ffff",
            GlyphIdOutOfRange { .. } => "glyph_id_out_of_range",
            GroupsNotSorted { .. } => "groups_not_sorted",
            CodepointOutOfRange { .. } => "codepoint_out_of_range",
            SelectorsNotSorted { .. } => "selectors_not_sorted",
            SubHeaderKeyNotAligned { .. } => "sub_header_key_not_aligned",
            SubHeaderRangeInvalid { .. } => "sub_header_range_invalid",
            Is32Mismatch { .. } => "is32_mismatch",
            LengthMismatch { .. } => "length_mismatch",
        }
    }

    /// Passes each attached value with its key, in a fixed order.
    pub fn details(&self, f: &mut dyn FnMut(&'static str, DetailValue)) {
        use DetailValue::{Int, Uint};
        use Kind::*;
        match *self {
            Truncated { needed, available } => {
                f("needed", Uint(needed));
                f("available", Uint(available));
            }
            UnknownVersion { found }
            | UnknownFormat { found }
            | UnitsPerEmOutOfRange { found }
            | SegCountOdd { found }
            | LastSegmentNotFfff { found } => f("found", Uint(found)),
            OffsetOutOfBounds {
                offset,
                length,
                available,
            } => {
                f("offset", Uint(offset));
                f("length", Uint(length));
                f("available", Uint(available));
            }
            RecordsNotSorted { index }
            | SegmentsNotSorted { index }
            | SegmentsOverlap { index }
            | RangeOffsetOutOfBounds { index }
            | RangeOffsetFfff { index }
            | GroupsNotSorted { index }
            | SelectorsNotSorted { index }
            | SubHeaderRangeInvalid { index } => f("index", Uint(index)),
            DuplicateTable { tag } => f("tag", DetailValue::Tag(tag)),
            ChecksumMismatch { stored, computed } => {
                f("stored", Uint(stored));
                f("computed", Uint(computed));
            }
            BadMagicNumber { expected, found } => {
                f("expected", Uint(expected));
                f("found", Uint(found));
            }
            InvalidIndexToLocFormat { found } => f("found", Int(found)),
            ZeroLongMetrics | SearchParametersInvalid => {}
            MetricsExceedGlyphs { metrics, glyphs } => {
                f("metrics", Uint(metrics));
                f("glyphs", Uint(glyphs));
            }
            StartAfterEnd { index, start, end } => {
                f("index", Uint(index));
                f("start", Uint(start));
                f("end", Uint(end));
            }
            GlyphIdOutOfRange { glyph, num_glyphs } => {
                f("glyph", Uint(glyph));
                f("num_glyphs", Uint(num_glyphs));
            }
            CodepointOutOfRange { codepoint } | Is32Mismatch { codepoint } => {
                f("codepoint", Uint(codepoint))
            }
            SubHeaderKeyNotAligned { index, found } => {
                f("index", Uint(index));
                f("found", Uint(found));
            }
            LengthMismatch {
                declared,
                available,
            } => {
                f("declared", Uint(declared));
                f("available", Uint(available));
            }
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Kind::*;
        match self {
            Truncated { needed, available } => {
                write!(
                    f,
                    "data ends early: {needed} bytes needed, {available} available"
                )
            }
            UnknownVersion { found } => write!(f, "unknown version {found:#010x}"),
            UnknownFormat { found } => write!(f, "unknown subtable format {found}"),
            OffsetOutOfBounds {
                offset,
                length,
                available,
            } => write!(
                f,
                "data at offset {offset} with length {length} is outside the {available} available bytes"
            ),
            RecordsNotSorted { index } => {
                write!(f, "record {index} is not in ascending order")
            }
            DuplicateTable { tag } => write!(f, "table '{tag}' appears more than once"),
            ChecksumMismatch { stored, computed } => {
                write!(
                    f,
                    "checksum {stored:#010x} does not match the computed {computed:#010x}"
                )
            }
            BadMagicNumber { expected, found } => {
                write!(
                    f,
                    "magic number is {found:#010x}, expected {expected:#010x}"
                )
            }
            UnitsPerEmOutOfRange { found } => {
                write!(f, "unitsPerEm {found} is outside 16..=16384")
            }
            InvalidIndexToLocFormat { found } => {
                write!(f, "indexToLocFormat {found} is neither 0 nor 1")
            }
            ZeroLongMetrics => f.write_str("numberOfHMetrics is 0 but the font has glyphs"),
            MetricsExceedGlyphs { metrics, glyphs } => {
                write!(f, "{metrics} long metrics for only {glyphs} glyphs")
            }
            SegCountOdd { found } => write!(f, "segCountX2 {found} is odd"),
            SearchParametersInvalid => {
                f.write_str("searchRange, entrySelector or rangeShift is wrong")
            }
            LastSegmentNotFfff { found } => {
                write!(f, "the last segment ends at {found:#06x}, not 0xFFFF")
            }
            StartAfterEnd { index, start, end } => {
                write!(
                    f,
                    "range {index} starts at {start:#x} after its end {end:#x}"
                )
            }
            SegmentsNotSorted { index } => write!(f, "segment {index} is out of order"),
            SegmentsOverlap { index } => write!(f, "segment {index} overlaps the previous one"),
            RangeOffsetOutOfBounds { index } => {
                write!(
                    f,
                    "idRangeOffset of segment {index} points outside the glyph id array"
                )
            }
            RangeOffsetFfff { index } => {
                write!(f, "idRangeOffset of segment {index} is 0xFFFF")
            }
            GlyphIdOutOfRange { glyph, num_glyphs } => {
                write!(f, "glyph id {glyph} is not below numGlyphs {num_glyphs}")
            }
            GroupsNotSorted { index } => {
                write!(
                    f,
                    "group {index} is out of order or overlaps the previous one"
                )
            }
            CodepointOutOfRange { codepoint } => {
                write!(f, "code point {codepoint:#x} is beyond U+10FFFF")
            }
            SelectorsNotSorted { index } => {
                write!(f, "variation selector record {index} is out of order")
            }
            SubHeaderKeyNotAligned { index, found } => {
                write!(f, "subHeaderKeys[{index}] = {found} is not a multiple of 8")
            }
            SubHeaderRangeInvalid { index } => {
                write!(f, "sub-header {index} covers bytes beyond 0xFF")
            }
            Is32Mismatch { codepoint } => {
                write!(f, "is32 does not match the width of code {codepoint:#x}")
            }
            LengthMismatch {
                declared,
                available,
            } => {
                write!(
                    f,
                    "length is {declared}, but {available} bytes are available"
                )
            }
        }
    }
}
