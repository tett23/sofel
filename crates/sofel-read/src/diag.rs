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
    /// How bad it is; always [`Kind::severity`] of `kind`.
    pub severity: Severity,
    /// What the problem is.
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
    /// An unsigned value (counts, offsets, indices, code points).
    Uint(u32),
    /// A signed value.
    Int(i32),
    /// A table tag.
    Tag(Tag),
}

/// What a [`Diagnostic`] is about. [`Kind::id`] and the keys passed by
/// [`Kind::details`] are part of the `sofel-validate --json` output
/// contract (ADR 0008).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The data ends before a structure is complete.
    Truncated {
        /// Bytes needed.
        needed: u32,
        /// Bytes available.
        available: u32,
    },
    /// An unknown table or file version.
    UnknownVersion {
        /// The value found.
        found: u32,
    },
    /// An unknown subtable format.
    UnknownFormat {
        /// The value found.
        found: u32,
    },
    /// An offset and length point outside the available data.
    OffsetOutOfBounds {
        /// The offset.
        offset: u32,
        /// The length.
        length: u32,
        /// Bytes available.
        available: u32,
    },
    /// A record (table directory or `cmap` encoding record) is not in ascending order.
    RecordsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A table tag appears more than once in the table directory.
    DuplicateTable {
        /// The duplicated tag.
        tag: Tag,
    },
    /// A table checksum does not match the computed one.
    ChecksumMismatch {
        /// The stored value.
        stored: u32,
        /// The computed value.
        computed: u32,
    },
    /// `magicNumber` in `head` is not 0x5F0F3CF5.
    BadMagicNumber {
        /// The expected value.
        expected: u32,
        /// The value found.
        found: u32,
    },
    /// `unitsPerEm` in `head` is outside 16..=16384.
    UnitsPerEmOutOfRange {
        /// The value found.
        found: u32,
    },
    /// `indexToLocFormat` in `head` is neither 0 nor 1.
    InvalidIndexToLocFormat {
        /// The value found.
        found: i32,
    },
    /// `numberOfHMetrics` is 0 but the font has glyphs.
    ZeroLongMetrics,
    /// There are more long metrics than glyphs.
    MetricsExceedGlyphs {
        /// The number of long metrics.
        metrics: u32,
        /// The number of glyphs.
        glyphs: u32,
    },
    /// `segCountX2` in a `cmap` format 4 subtable is odd.
    SegCountOdd {
        /// The value found.
        found: u32,
    },
    /// `searchRange`, `entrySelector` or `rangeShift` is wrong.
    SearchParametersInvalid,
    /// The last segment of a `cmap` format 4 subtable does not end at 0xFFFF.
    LastSegmentNotFfff {
        /// The value found.
        found: u32,
    },
    /// A range starts after its end.
    StartAfterEnd {
        /// Index of the offending entry.
        index: u32,
        /// The start of the range.
        start: u32,
        /// The end of the range.
        end: u32,
    },
    /// A `cmap` format 4 segment is out of order.
    SegmentsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A `cmap` format 4 segment overlaps the previous one.
    SegmentsOverlap {
        /// Index of the offending entry.
        index: u32,
    },
    /// `idRangeOffset` of a `cmap` format 4 segment points outside the glyph id array.
    RangeOffsetOutOfBounds {
        /// Index of the offending entry.
        index: u32,
    },
    /// `idRangeOffset` of a `cmap` format 4 segment is 0xFFFF.
    RangeOffsetFfff {
        /// Index of the offending entry.
        index: u32,
    },
    /// A glyph id is not below `numGlyphs`.
    GlyphIdOutOfRange {
        /// The glyph id.
        glyph: u32,
        /// `numGlyphs`.
        num_glyphs: u32,
    },
    /// A group or range is out of order or overlaps the previous one.
    GroupsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A code point is beyond U+10FFFF.
    CodepointOutOfRange {
        /// The code point.
        codepoint: u32,
    },
    /// A `cmap` format 14 variation selector record is out of order.
    SelectorsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A `subHeaderKeys` entry of a `cmap` format 2 subtable is not a multiple of 8.
    SubHeaderKeyNotAligned {
        /// Index of the offending entry.
        index: u32,
        /// The value found.
        found: u32,
    },
    /// A `cmap` format 2 sub-header covers bytes beyond 0xFF.
    SubHeaderRangeInvalid {
        /// Index of the offending entry.
        index: u32,
    },
    /// The `is32` bit of a `cmap` format 8 subtable does not match the width of a code.
    Is32Mismatch {
        /// The code point.
        codepoint: u32,
    },
    /// A declared length does not match the available bytes.
    LengthMismatch {
        /// The declared length.
        declared: u32,
        /// Bytes available.
        available: u32,
    },
    /// A `loca` entry is below the one before it.
    LocaNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A simple glyph with 0xFFF or more contours (FreeType rejects it).
    TooManyContours {
        /// The glyph id.
        glyph: u32,
        /// The number of contours.
        found: u32,
    },
    /// A contour end point not above the previous one.
    EndPointsNotSorted {
        /// The glyph id.
        glyph: u32,
        /// Index of the contour.
        contour: u32,
    },
    /// A flag repeat count that runs past the last point.
    FlagRepeatOverrun {
        /// The glyph id.
        glyph: u32,
    },
    /// A component glyph id not below numGlyphs.
    ComponentGlyphOutOfRange {
        /// The glyph id.
        glyph: u32,
        /// Index of the component.
        component: u32,
        /// The glyph id the component refers to.
        found: u32,
    },
    /// `usWeightClass` in `OS/2` is outside 1..=1000.
    WeightClassOutOfRange {
        /// The value found.
        found: u32,
    },
    /// `fsSelection` in `OS/2` sets REGULAR together with ITALIC or BOLD.
    FsSelectionConflict {
        /// The value found.
        found: u32,
    },
    /// `usFirstCharIndex` in `OS/2` is above `usLastCharIndex`.
    FirstCharAfterLast {
        /// `usFirstCharIndex`.
        first: u32,
        /// `usLastCharIndex`.
        last: u32,
    },
    /// A UTF-16BE `name` string has an odd length.
    OddLength {
        /// Index of the record.
        index: u32,
        /// The length.
        length: u32,
    },
    /// A `post` version 2.0 name index points past the stored names.
    NameIndexOutOfRange {
        /// The glyph id.
        glyph: u32,
        /// The name index.
        index: u32,
    },
    /// A deprecated table version (`post` 2.5).
    DeprecatedVersion {
        /// The value found.
        found: u32,
    },
    /// Glyph ids that should ascend do not (`VORG`).
    GlyphsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A glyph id appears in two records (`VORG`).
    DuplicateGlyph {
        /// Index of the second record.
        index: u32,
        /// The glyph id.
        glyph: u32,
    },
    /// Records searched by tag are not in ascending tag order, or repeat
    /// a tag (`BASE`).
    TagsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A BaseValues table has a different number of coordinates than
    /// the axis has baseline tags.
    BaseCoordCountMismatch {
        /// The number of baseline tags.
        tags: u32,
        /// The number of coordinates.
        coords: u32,
    },
    /// `defaultBaselineIndex` of a BaseValues table is not below the
    /// number of baseline tags.
    DefaultBaselineOutOfRange {
        /// The index.
        index: u32,
        /// The number of baseline tags.
        count: u32,
    },
}

impl Kind {
    /// The severity this kind always has.
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
            | SelectorsNotSorted { .. }
            | LocaNotSorted { .. }
            | TooManyContours { .. }
            | EndPointsNotSorted { .. }
            | FlagRepeatOverrun { .. }
            | ComponentGlyphOutOfRange { .. }
            | NameIndexOutOfRange { .. }
            | GlyphsNotSorted { .. }
            | DuplicateGlyph { .. }
            | TagsNotSorted { .. }
            | BaseCoordCountMismatch { .. } => Severity::Error,
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
            | LengthMismatch { .. }
            | WeightClassOutOfRange { .. }
            | FsSelectionConflict { .. }
            | FirstCharAfterLast { .. }
            | OddLength { .. }
            | DeprecatedVersion { .. }
            | DefaultBaselineOutOfRange { .. } => Severity::Warning,
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
            LocaNotSorted { .. } => "loca_not_sorted",
            TooManyContours { .. } => "too_many_contours",
            EndPointsNotSorted { .. } => "end_points_not_sorted",
            FlagRepeatOverrun { .. } => "flag_repeat_overrun",
            ComponentGlyphOutOfRange { .. } => "component_glyph_out_of_range",
            WeightClassOutOfRange { .. } => "weight_class_out_of_range",
            FsSelectionConflict { .. } => "fs_selection_conflict",
            FirstCharAfterLast { .. } => "first_char_after_last",
            OddLength { .. } => "odd_length",
            NameIndexOutOfRange { .. } => "name_index_out_of_range",
            DeprecatedVersion { .. } => "deprecated_version",
            GlyphsNotSorted { .. } => "glyphs_not_sorted",
            DuplicateGlyph { .. } => "duplicate_glyph",
            TagsNotSorted { .. } => "tags_not_sorted",
            BaseCoordCountMismatch { .. } => "base_coord_count_mismatch",
            DefaultBaselineOutOfRange { .. } => "default_baseline_out_of_range",
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
            | WeightClassOutOfRange { found }
            | FsSelectionConflict { found }
            | DeprecatedVersion { found }
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
            | SubHeaderRangeInvalid { index }
            | GlyphsNotSorted { index }
            | TagsNotSorted { index } => f("index", Uint(index)),
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
            LocaNotSorted { index } => f("index", Uint(index)),
            TooManyContours { glyph, found } => {
                f("glyph", Uint(glyph));
                f("found", Uint(found));
            }
            EndPointsNotSorted { glyph, contour } => {
                f("glyph", Uint(glyph));
                f("contour", Uint(contour));
            }
            FlagRepeatOverrun { glyph } => f("glyph", Uint(glyph)),
            ComponentGlyphOutOfRange {
                glyph,
                component,
                found,
            } => {
                f("glyph", Uint(glyph));
                f("component", Uint(component));
                f("found", Uint(found));
            }
            FirstCharAfterLast { first, last } => {
                f("first", Uint(first));
                f("last", Uint(last));
            }
            OddLength { index, length } => {
                f("index", Uint(index));
                f("length", Uint(length));
            }
            NameIndexOutOfRange { glyph, index } => {
                f("glyph", Uint(glyph));
                f("index", Uint(index));
            }
            DuplicateGlyph { index, glyph } => {
                f("index", Uint(index));
                f("glyph", Uint(glyph));
            }
            BaseCoordCountMismatch { tags, coords } => {
                f("tags", Uint(tags));
                f("coords", Uint(coords));
            }
            DefaultBaselineOutOfRange { index, count } => {
                f("index", Uint(index));
                f("count", Uint(count));
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
            LocaNotSorted { index } => {
                write!(f, "loca entry {index} is below the one before it")
            }
            TooManyContours { glyph, found } => {
                write!(f, "glyph {glyph} has {found} contours (0xFFF or more)")
            }
            EndPointsNotSorted { glyph, contour } => write!(
                f,
                "end point of contour {contour} of glyph {glyph} is not above the previous one"
            ),
            FlagRepeatOverrun { glyph } => {
                write!(f, "a flag repeat of glyph {glyph} runs past its last point")
            }
            ComponentGlyphOutOfRange {
                glyph,
                component,
                found,
            } => write!(
                f,
                "component {component} of glyph {glyph} refers to glyph {found}, not below numGlyphs"
            ),
            WeightClassOutOfRange { found } => {
                write!(f, "usWeightClass {found} is outside 1..=1000")
            }
            FsSelectionConflict { found } => write!(
                f,
                "fsSelection {found:#06x} sets REGULAR together with ITALIC or BOLD"
            ),
            FirstCharAfterLast { first, last } => write!(
                f,
                "usFirstCharIndex {first:#06x} is above usLastCharIndex {last:#06x}"
            ),
            OddLength { index, length } => {
                write!(
                    f,
                    "UTF-16BE string of record {index} has odd length {length}"
                )
            }
            NameIndexOutOfRange { glyph, index } => write!(
                f,
                "name index {index} of glyph {glyph} points past the stored names"
            ),
            DeprecatedVersion { found } => write!(f, "version {found:#010x} is deprecated"),
            GlyphsNotSorted { index } => {
                write!(
                    f,
                    "glyph id of record {index} is not above the previous one"
                )
            }
            DuplicateGlyph { index, glyph } => {
                write!(f, "record {index} repeats glyph id {glyph}")
            }
            TagsNotSorted { index } => {
                write!(f, "tag of record {index} is not above the previous one")
            }
            BaseCoordCountMismatch { tags, coords } => {
                write!(f, "{coords} base coordinates for {tags} baseline tags")
            }
            DefaultBaselineOutOfRange { index, count } => write!(
                f,
                "defaultBaselineIndex {index} is not below the {count} baseline tags"
            ),
        }
    }
}
