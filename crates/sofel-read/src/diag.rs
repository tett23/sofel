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
    /// A compressed font container cannot be opened.
    InvalidFontContainer {
        /// The reason reported by the container decoder.
        reason: &'static str,
    },
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
    /// A VARC structure contains an invalid value or component record.
    InvalidVarc,
    /// An unknown subtable format.
    UnknownFormat {
        /// The value found.
        found: u32,
    },
    /// An unknown FeatureVariations condition format, treated as false.
    UnknownConditionFormat {
        /// The value found.
        found: u32,
    },
    /// A FeatureVariations condition refers to an axis outside fvar.
    ConditionAxisOutOfRange {
        /// The axis index.
        axis: u32,
        /// Number of fvar axes.
        count: u32,
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
    /// A CFF or CFF2 `hdrSize` below the header's own size.
    HeaderSizeTooSmall {
        /// The value found.
        found: u32,
        /// The smallest allowed value.
        minimum: u32,
    },
    /// A CFF offset size that is not 1 to 4.
    InvalidOffSize {
        /// The value found.
        found: u32,
    },
    /// The first offset of a CFF INDEX is not 1.
    FirstOffsetNotOne {
        /// The value found.
        found: u32,
    },
    /// A CFF INDEX offset is below the one before it.
    OffsetsNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A sfnt `CFF ` table with more than one font in its Name INDEX.
    MultipleNames {
        /// The count.
        count: u32,
    },
    /// The Top DICT INDEX has fewer entries than the Name INDEX.
    TooFewTopDicts {
        /// The value found.
        found: u32,
        /// The expected value.
        expected: u32,
    },
    /// Too many operands before a DICT operator.
    DictStackOverflow,
    /// A DICT operator without the operands it needs.
    DictMissingOperands {
        /// The operator (`0x0C00 | b` for `12 b`).
        operator: u32,
    },
    /// A two-byte DICT operator cut by the end of the data.
    DictTruncatedOperator,
    /// A DICT operand value FreeType rejects.
    DictInvalidValue {
        /// The operator (`0x0C00 | b` for `12 b`).
        operator: u32,
    },
    /// A Private DICT `blend` without a usable Variation Store region list.
    DictBlend,
    /// A DICT real number that is not terminated or has an invalid nibble.
    InvalidReal,
    /// A Top DICT without `CharStrings`.
    MissingCharStrings,
    /// A `CharstringType` other than 2.
    UnsupportedCharstringType {
        /// The value found.
        found: u32,
    },
    /// A CharStrings INDEX without glyphs.
    NoGlyphs,
    /// More glyphs than a predefined charset covers.
    PredefinedCharsetTooShort {
        /// The number of glyphs.
        glyphs: u32,
        /// The number of glyphs it covers.
        max: u32,
    },
    /// An FDArray with more than 256 Font DICTs.
    TooManyFontDicts {
        /// The value found.
        found: u32,
    },
    /// A Font DICT without a `Private` DICT.
    MissingPrivate {
        /// The Font DICT index.
        fd: u32,
    },
    /// A CID-keyed CFF or CFF2 table without an FDArray.
    MissingFdArray,
    /// An FDSelect of format 3 or 4 without ranges.
    EmptyRanges,
    /// The first FDSelect range does not start at glyph 0.
    FirstRangeNotZero {
        /// The value found.
        found: u32,
    },
    /// An FDSelect range does not start after the one before it.
    RangesNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// The FDSelect sentinel is not the glyph count.
    SentinelMismatch {
        /// The value found.
        found: u32,
        /// The expected value.
        expected: u32,
    },
    /// An FDSelect Font DICT index not below the FDArray count.
    FdOutOfRange {
        /// The Font DICT index.
        fd: u32,
        /// The count.
        count: u32,
    },
    /// Variation regions, or `avar`, whose axis count differs from `fvar`.
    AxisCountMismatch {
        /// The value found.
        found: u32,
        /// The expected value.
        expected: u32,
    },
    /// A region index not below the region count.
    RegionIndexOutOfRange {
        /// Index of the offending entry.
        index: u32,
        /// The count.
        count: u32,
    },
    /// `wordDeltaCount` exceeds `regionIndexCount` in ItemVariationData.
    WordDeltaCountOutOfRange {
        /// Count excluding the LONG_WORDS flag.
        found: u32,
        /// `regionIndexCount`.
        count: u32,
    },
    /// A variation index refers outside its ItemVariationStore.
    VariationIndexOutOfRange {
        /// ItemVariationData index.
        outer: u32,
        /// Item index in that subtable.
        inner: u32,
    },
    /// A gvar tuple has invalid point numbers or packed delta lengths.
    PointDeltaCountMismatch {
        /// Glyph index.
        glyph: u32,
        /// Tuple index within that glyph.
        tuple: u32,
    },
    /// A charstring pushes more operands than the stack holds.
    CharStringStackOverflow {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A charstring operator without the operands it needs.
    CharStringStackUnderflow {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A subroutine number outside its INDEX.
    SubroutineOutOfRange {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// Subroutines nested deeper than FreeType allows.
    SubroutineTooDeep {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A charstring number, operator or mask cut by the end of the data.
    CharStringTruncated {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A CFF charstring that ends without `endchar`.
    MissingEndchar {
        /// The glyph id.
        glyph: u32,
    },
    /// A CFF2 `vsindex` outside the Variation Store.
    VsindexOutOfRange {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A CFF2 `blend` in a font without a Variation Store.
    BlendWithoutStore {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A charstring FreeType cannot decode for another reason.
    InvalidCharString {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
    },
    /// A DICT operator neither the specification nor FreeType knows.
    UnknownOperator {
        /// The operator (`0x0C00 | b` for `12 b`).
        operator: u32,
    },
    /// A DICT operator of the other CFF version.
    OperatorNotAllowed {
        /// The operator (`0x0C00 | b` for `12 b`).
        operator: u32,
    },
    /// A DICT operator that appears more than once (the last one wins).
    DuplicateOperator {
        /// The operator (`0x0C00 | b` for `12 b`).
        operator: u32,
    },
    /// The CharStrings count differs from `maxp`.
    GlyphCountMismatch {
        /// The value found.
        found: u32,
        /// The expected value.
        expected: u32,
    },
    /// A charset SID beyond the String INDEX.
    SidOutOfRange {
        /// The glyph id.
        glyph: u32,
        /// The SID.
        sid: u32,
    },
    /// An FDSelect of format 4, which FreeType cannot read.
    FdSelectFormat4,
    /// A charstring operator that is reserved or of the other version.
    UnknownCharStringOperator {
        /// The glyph id.
        glyph: u32,
        /// The byte position in the charstring or subroutine.
        position: u32,
        /// The operator (`0x0C00 | b` for `12 b`).
        operator: u32,
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
    /// Coverage glyphs or ranges that do not ascend, or ranges that
    /// overlap; binary search may then miss covered glyphs.
    CoverageNotSorted {
        /// Index of the offending entry.
        index: u32,
    },
    /// A Coverage glyph array repeats a glyph; binary search still finds
    /// it, at one of its coverage indices.
    CoverageDuplicate {
        /// Index of the second entry.
        index: u32,
        /// The glyph id.
        glyph: u32,
    },
    /// `startCoverageIndex` of a Coverage range is not the number of
    /// glyphs in the ranges before it.
    CoverageIndexMismatch {
        /// Index of the range.
        index: u32,
        /// The expected value.
        expected: u32,
        /// The value found.
        found: u32,
    },
    /// A ClassDef range starts at or before the end of the previous one.
    ClassRangesOverlap {
        /// Index of the offending range.
        index: u32,
    },
    /// The number of entries of an array indexed by coverage index is
    /// not the number of glyphs of its Coverage table.
    CoverageCountMismatch {
        /// The number of glyphs covered.
        coverage: u32,
        /// The number of entries.
        count: u32,
    },
    /// A lookup index is not below the number of lookups.
    LookupIndexOutOfRange {
        /// The index.
        index: u32,
        /// The number of lookups.
        count: u32,
    },
    /// A feature index is not below the number of features.
    FeatureIndexOutOfRange {
        /// The index.
        index: u32,
        /// The number of features.
        count: u32,
    },
    /// A sequence lookup record's position is not below the length of
    /// the input sequence.
    SequenceIndexOutOfRange {
        /// The position.
        index: u32,
        /// The length of the input sequence.
        count: u32,
    },
    /// A lookup's `markFilteringSet` is not below the number of mark
    /// glyph sets in `GDEF`.
    MarkSetOutOfRange {
        /// The set index.
        index: u32,
        /// The number of sets.
        count: u32,
    },
    /// An unknown lookup type.
    UnknownLookupType {
        /// The value found.
        found: u32,
    },
    /// An extension subtable points to a type other than that of the
    /// lookup's first subtable.
    ExtensionTypeMismatch {
        /// The type of the first subtable.
        expected: u32,
        /// The type found.
        found: u32,
    },
    /// An extension subtable points to another extension.
    NestedExtension,
    /// A sequence context with no input glyphs, or a ligature of no
    /// components.
    EmptyInputSequence,
    /// Reserved bits of a flag field are set (lookup flags, ValueFormat).
    ReservedBitsSet {
        /// The value found.
        found: u32,
    },
    /// Shaping discarded a layout table during sanitization.
    LayoutTableDiscarded {
        /// The discarded table.
        table: Tag,
    },
    /// Validation followed more offsets and records than the size of
    /// the table allows (HarfBuzz rejects such a table), or found 1000
    /// problems, and stopped.
    WorkLimitExceeded,
    /// A color index is not below the number of colors: a `CPAL`
    /// palette that runs past the color records, or a `COLR` palette
    /// index past `numPaletteEntries` (ADR 0023).
    ColorIndexOutOfRange {
        /// The index.
        index: u32,
        /// The number of colors.
        count: u32,
    },
    /// A `COLR` layer index is not below the number of layers (ADR 0023).
    LayerIndexOutOfRange {
        /// The index.
        index: u32,
        /// The number of layers.
        count: u32,
    },
}

impl Kind {
    /// The severity this kind always has.
    pub fn severity(&self) -> Severity {
        use Kind::*;
        match self {
            InvalidFontContainer { .. }
            | Truncated { .. }
            | UnknownVersion { .. }
            | InvalidVarc
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
            | HeaderSizeTooSmall { .. }
            | InvalidOffSize { .. }
            | FirstOffsetNotOne { .. }
            | OffsetsNotSorted { .. }
            | MultipleNames { .. }
            | TooFewTopDicts { .. }
            | DictStackOverflow
            | DictMissingOperands { .. }
            | DictTruncatedOperator
            | DictInvalidValue { .. }
            | DictBlend
            | InvalidReal
            | MissingCharStrings
            | UnsupportedCharstringType { .. }
            | NoGlyphs
            | PredefinedCharsetTooShort { .. }
            | TooManyFontDicts { .. }
            | MissingPrivate { .. }
            | MissingFdArray
            | EmptyRanges
            | FirstRangeNotZero { .. }
            | RangesNotSorted { .. }
            | SentinelMismatch { .. }
            | FdOutOfRange { .. }
            | AxisCountMismatch { .. }
            | RegionIndexOutOfRange { .. }
            | WordDeltaCountOutOfRange { .. }
            | VariationIndexOutOfRange { .. }
            | PointDeltaCountMismatch { .. }
            | ConditionAxisOutOfRange { .. }
            | CharStringStackOverflow { .. }
            | CharStringStackUnderflow { .. }
            | SubroutineOutOfRange { .. }
            | SubroutineTooDeep { .. }
            | CharStringTruncated { .. }
            | MissingEndchar { .. }
            | VsindexOutOfRange { .. }
            | BlendWithoutStore { .. }
            | InvalidCharString { .. }
            | TagsNotSorted { .. }
            | BaseCoordCountMismatch { .. }
            | CoverageNotSorted { .. }
            | CoverageIndexMismatch { .. }
            | ClassRangesOverlap { .. }
            | CoverageCountMismatch { .. }
            | LookupIndexOutOfRange { .. }
            | FeatureIndexOutOfRange { .. }
            | SequenceIndexOutOfRange { .. }
            | MarkSetOutOfRange { .. }
            | UnknownLookupType { .. }
            | ExtensionTypeMismatch { .. }
            | NestedExtension
            | EmptyInputSequence
            | WorkLimitExceeded
            | LayerIndexOutOfRange { .. } => Severity::Error,
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
            | UnknownOperator { .. }
            | OperatorNotAllowed { .. }
            | DuplicateOperator { .. }
            | GlyphCountMismatch { .. }
            | SidOutOfRange { .. }
            | FdSelectFormat4
            | UnknownCharStringOperator { .. }
            | DefaultBaselineOutOfRange { .. }
            | ReservedBitsSet { .. }
            | CoverageDuplicate { .. }
            | ColorIndexOutOfRange { .. } => Severity::Warning,
            UnknownConditionFormat { .. } | LayoutTableDiscarded { .. } => Severity::Warning,
        }
    }

    /// A stable snake_case identifier.
    pub fn id(&self) -> &'static str {
        use Kind::*;
        match self {
            InvalidFontContainer { .. } => "invalid_font_container",
            Truncated { .. } => "truncated",
            UnknownVersion { .. } => "unknown_version",
            InvalidVarc => "invalid_varc",
            UnknownFormat { .. } => "unknown_format",
            UnknownConditionFormat { .. } => "unknown_condition_format",
            ConditionAxisOutOfRange { .. } => "condition_axis_out_of_range",
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
            HeaderSizeTooSmall { .. } => "header_size_too_small",
            InvalidOffSize { .. } => "invalid_off_size",
            FirstOffsetNotOne { .. } => "first_offset_not_one",
            OffsetsNotSorted { .. } => "offsets_not_sorted",
            MultipleNames { .. } => "multiple_names",
            TooFewTopDicts { .. } => "too_few_top_dicts",
            DictStackOverflow => "dict_stack_overflow",
            DictMissingOperands { .. } => "dict_missing_operands",
            DictTruncatedOperator => "dict_truncated_operator",
            DictInvalidValue { .. } => "dict_invalid_value",
            DictBlend => "dict_blend",
            InvalidReal => "invalid_real",
            MissingCharStrings => "missing_char_strings",
            UnsupportedCharstringType { .. } => "unsupported_charstring_type",
            NoGlyphs => "no_glyphs",
            PredefinedCharsetTooShort { .. } => "predefined_charset_too_short",
            TooManyFontDicts { .. } => "too_many_font_dicts",
            MissingPrivate { .. } => "missing_private",
            MissingFdArray => "missing_fd_array",
            EmptyRanges => "empty_ranges",
            FirstRangeNotZero { .. } => "first_range_not_zero",
            RangesNotSorted { .. } => "ranges_not_sorted",
            SentinelMismatch { .. } => "sentinel_mismatch",
            FdOutOfRange { .. } => "fd_out_of_range",
            AxisCountMismatch { .. } => "axis_count_mismatch",
            RegionIndexOutOfRange { .. } => "region_index_out_of_range",
            WordDeltaCountOutOfRange { .. } => "word_delta_count_out_of_range",
            VariationIndexOutOfRange { .. } => "variation_index_out_of_range",
            PointDeltaCountMismatch { .. } => "point_delta_count_mismatch",
            CharStringStackOverflow { .. } => "char_string_stack_overflow",
            CharStringStackUnderflow { .. } => "char_string_stack_underflow",
            SubroutineOutOfRange { .. } => "subroutine_out_of_range",
            SubroutineTooDeep { .. } => "subroutine_too_deep",
            CharStringTruncated { .. } => "char_string_truncated",
            MissingEndchar { .. } => "missing_endchar",
            VsindexOutOfRange { .. } => "vsindex_out_of_range",
            BlendWithoutStore { .. } => "blend_without_store",
            InvalidCharString { .. } => "invalid_char_string",
            UnknownOperator { .. } => "unknown_operator",
            OperatorNotAllowed { .. } => "operator_not_allowed",
            DuplicateOperator { .. } => "duplicate_operator",
            GlyphCountMismatch { .. } => "glyph_count_mismatch",
            SidOutOfRange { .. } => "sid_out_of_range",
            FdSelectFormat4 => "fd_select_format4",
            UnknownCharStringOperator { .. } => "unknown_char_string_operator",
            TagsNotSorted { .. } => "tags_not_sorted",
            BaseCoordCountMismatch { .. } => "base_coord_count_mismatch",
            DefaultBaselineOutOfRange { .. } => "default_baseline_out_of_range",
            CoverageNotSorted { .. } => "coverage_not_sorted",
            CoverageDuplicate { .. } => "coverage_duplicate",
            CoverageIndexMismatch { .. } => "coverage_index_mismatch",
            ClassRangesOverlap { .. } => "class_ranges_overlap",
            CoverageCountMismatch { .. } => "coverage_count_mismatch",
            LookupIndexOutOfRange { .. } => "lookup_index_out_of_range",
            FeatureIndexOutOfRange { .. } => "feature_index_out_of_range",
            SequenceIndexOutOfRange { .. } => "sequence_index_out_of_range",
            MarkSetOutOfRange { .. } => "mark_set_out_of_range",
            UnknownLookupType { .. } => "unknown_lookup_type",
            ExtensionTypeMismatch { .. } => "extension_type_mismatch",
            NestedExtension => "nested_extension",
            EmptyInputSequence => "empty_input_sequence",
            ReservedBitsSet { .. } => "reserved_bits_set",
            LayoutTableDiscarded { .. } => "layout_table_discarded",
            WorkLimitExceeded => "work_limit_exceeded",
            ColorIndexOutOfRange { .. } => "color_index_out_of_range",
            LayerIndexOutOfRange { .. } => "layer_index_out_of_range",
        }
    }

    /// Passes each attached value with its key, in a fixed order.
    pub fn details(&self, f: &mut dyn FnMut(&'static str, DetailValue)) {
        use DetailValue::{Int, Uint};
        use Kind::*;
        match *self {
            InvalidFontContainer { .. } => {}
            InvalidVarc => {}
            Truncated { needed, available } => {
                f("needed", Uint(needed));
                f("available", Uint(available));
            }
            UnknownVersion { found }
            | UnknownFormat { found }
            | UnknownConditionFormat { found }
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
            | TagsNotSorted { index }
            | CoverageNotSorted { index }
            | ClassRangesOverlap { index } => f("index", Uint(index)),
            UnknownLookupType { found } | ReservedBitsSet { found } => f("found", Uint(found)),
            CoverageIndexMismatch {
                index,
                expected,
                found,
            } => {
                f("index", Uint(index));
                f("expected", Uint(expected));
                f("found", Uint(found));
            }
            CoverageCountMismatch { coverage, count } => {
                f("coverage", Uint(coverage));
                f("count", Uint(count));
            }
            LookupIndexOutOfRange { index, count }
            | FeatureIndexOutOfRange { index, count }
            | SequenceIndexOutOfRange { index, count }
            | MarkSetOutOfRange { index, count }
            | ColorIndexOutOfRange { index, count }
            | LayerIndexOutOfRange { index, count } => {
                f("index", Uint(index));
                f("count", Uint(count));
            }
            ExtensionTypeMismatch { expected, found } => {
                f("expected", Uint(expected));
                f("found", Uint(found));
            }
            NestedExtension
            | EmptyInputSequence
            | WorkLimitExceeded
            | LayoutTableDiscarded { .. } => {}
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
            DuplicateGlyph { index, glyph } | CoverageDuplicate { index, glyph } => {
                f("index", Uint(index));
                f("glyph", Uint(glyph));
            }
            HeaderSizeTooSmall { found, minimum } => {
                f("found", Uint(found));
                f("minimum", Uint(minimum));
            }
            InvalidOffSize { found } => {
                f("found", Uint(found));
            }
            FirstOffsetNotOne { found } => {
                f("found", Uint(found));
            }
            OffsetsNotSorted { index } => {
                f("index", Uint(index));
            }
            MultipleNames { count } => {
                f("count", Uint(count));
            }
            TooFewTopDicts { found, expected } => {
                f("found", Uint(found));
                f("expected", Uint(expected));
            }
            DictStackOverflow => {}
            DictMissingOperands { operator } => {
                f("operator", Uint(operator));
            }
            DictTruncatedOperator => {}
            DictInvalidValue { operator } => {
                f("operator", Uint(operator));
            }
            DictBlend => {}
            InvalidReal => {}
            MissingCharStrings => {}
            UnsupportedCharstringType { found } => {
                f("found", Uint(found));
            }
            NoGlyphs => {}
            PredefinedCharsetTooShort { glyphs, max } => {
                f("glyphs", Uint(glyphs));
                f("max", Uint(max));
            }
            TooManyFontDicts { found } => {
                f("found", Uint(found));
            }
            MissingPrivate { fd } => {
                f("fd", Uint(fd));
            }
            MissingFdArray => {}
            EmptyRanges => {}
            FirstRangeNotZero { found } => {
                f("found", Uint(found));
            }
            RangesNotSorted { index } => {
                f("index", Uint(index));
            }
            SentinelMismatch { found, expected } => {
                f("found", Uint(found));
                f("expected", Uint(expected));
            }
            FdOutOfRange { fd, count } => {
                f("fd", Uint(fd));
                f("count", Uint(count));
            }
            AxisCountMismatch { found, expected } => {
                f("found", Uint(found));
                f("expected", Uint(expected));
            }
            RegionIndexOutOfRange { index, count } => {
                f("index", Uint(index));
                f("count", Uint(count));
            }
            WordDeltaCountOutOfRange { found, count } => {
                f("found", Uint(found));
                f("count", Uint(count));
            }
            VariationIndexOutOfRange { outer, inner } => {
                f("outer", Uint(outer));
                f("inner", Uint(inner));
            }
            PointDeltaCountMismatch { glyph, tuple } => {
                f("glyph", Uint(glyph));
                f("tuple", Uint(tuple));
            }
            ConditionAxisOutOfRange { axis, count } => {
                f("axis", Uint(axis));
                f("count", Uint(count));
            }
            CharStringStackOverflow { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            CharStringStackUnderflow { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            SubroutineOutOfRange { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            SubroutineTooDeep { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            CharStringTruncated { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            MissingEndchar { glyph } => {
                f("glyph", Uint(glyph));
            }
            VsindexOutOfRange { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            BlendWithoutStore { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            InvalidCharString { glyph, position } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
            }
            UnknownOperator { operator } => {
                f("operator", Uint(operator));
            }
            OperatorNotAllowed { operator } => {
                f("operator", Uint(operator));
            }
            DuplicateOperator { operator } => {
                f("operator", Uint(operator));
            }
            GlyphCountMismatch { found, expected } => {
                f("found", Uint(found));
                f("expected", Uint(expected));
            }
            SidOutOfRange { glyph, sid } => {
                f("glyph", Uint(glyph));
                f("sid", Uint(sid));
            }
            FdSelectFormat4 => {}
            UnknownCharStringOperator {
                glyph,
                position,
                operator,
            } => {
                f("glyph", Uint(glyph));
                f("position", Uint(position));
                f("operator", Uint(operator));
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
            InvalidFontContainer { reason } => write!(f,"invalid font container: {reason}"),
            Truncated { needed, available } => {
                write!(
                    f,
                    "data ends early: {needed} bytes needed, {available} available"
                )
            }
            UnknownVersion { found } => write!(f, "unknown version {found:#010x}"),
            InvalidVarc => f.write_str("invalid VARC structure"),
            UnknownFormat { found } => write!(f, "unknown subtable format {found}"),
            UnknownConditionFormat { found } => {
                write!(f, "unknown FeatureVariations condition format {found}")
            }
            ConditionAxisOutOfRange { axis, count } => {
                write!(f, "FeatureVariations condition axis {axis} is not below fvar axis count {count}")
            }
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
            HeaderSizeTooSmall { found, minimum } => {
                write!(f, "hdrSize {found} is below {minimum}")
            }
            InvalidOffSize { found } => write!(f, "offset size {found} is not 1 to 4"),
            FirstOffsetNotOne { found } => write!(f, "the first INDEX offset is {found}, not 1"),
            OffsetsNotSorted { index } => {
                write!(f, "INDEX offset {index} is below the one before it")
            }
            MultipleNames { count } => write!(
                f,
                "the Name INDEX has {count} fonts; a sfnt CFF table has one"
            ),
            TooFewTopDicts { found, expected } => {
                write!(f, "{found} Top DICTs for {expected} names")
            }
            DictStackOverflow => f.write_str("too many operands before a DICT operator"),
            DictMissingOperands { operator } => {
                write!(f, "DICT operator {operator:#06x} lacks operands")
            }
            DictTruncatedOperator => {
                f.write_str("a two-byte DICT operator is cut by the end of the data")
            }
            DictInvalidValue { operator } => {
                write!(f, "DICT operator {operator:#06x} has an invalid value")
            }
            DictBlend => f.write_str("a Private DICT blend cannot be computed"),
            InvalidReal => f.write_str("a DICT real number is broken"),
            MissingCharStrings => f.write_str("the Top DICT has no CharStrings"),
            UnsupportedCharstringType { found } => write!(f, "CharstringType {found} is not 2"),
            NoGlyphs => f.write_str("the CharStrings INDEX is empty"),
            PredefinedCharsetTooShort { glyphs, max } => {
                write!(f, "{glyphs} glyphs for a predefined charset of {max}")
            }
            TooManyFontDicts { found } => {
                write!(f, "the FDArray has {found} Font DICTs (more than 256)")
            }
            MissingPrivate { fd } => write!(f, "Font DICT {fd} has no Private DICT"),
            MissingFdArray => f.write_str("there is no FDArray"),
            EmptyRanges => f.write_str("the FDSelect has no ranges"),
            FirstRangeNotZero { found } => {
                write!(f, "the first FDSelect range starts at glyph {found}, not 0")
            }
            RangesNotSorted { index } => write!(
                f,
                "FDSelect range {index} does not start after the one before it"
            ),
            SentinelMismatch { found, expected } => write!(
                f,
                "the FDSelect sentinel is {found}, not the glyph count {expected}"
            ),
            FdOutOfRange { fd, count } => write!(
                f,
                "Font DICT index {fd} is not below the FDArray count {count}"
            ),
            AxisCountMismatch { found, expected } => {
                write!(f, "the regions have {found} axes, fvar has {expected}")
            }
            RegionIndexOutOfRange { index, count } => write!(
                f,
                "region index {index} is not below the region count {count}"
            ),
            WordDeltaCountOutOfRange { found, count } => write!(
                f,
                "word delta count {found} exceeds the region index count {count}"
            ),
            VariationIndexOutOfRange { outer, inner } => write!(
                f,
                "variation index ({outer}, {inner}) is outside the ItemVariationStore"
            ),
            PointDeltaCountMismatch { glyph, tuple } => write!(
                f,
                "gvar glyph {glyph} tuple {tuple} has invalid point numbers or delta count"
            ),
            CharStringStackOverflow { glyph, position } => write!(
                f,
                "glyph {glyph}: operand stack overflow at byte {position}"
            ),
            CharStringStackUnderflow { glyph, position } => write!(
                f,
                "glyph {glyph}: operand stack underflow at byte {position}"
            ),
            SubroutineOutOfRange { glyph, position } => write!(
                f,
                "glyph {glyph}: subroutine out of range at byte {position}"
            ),
            SubroutineTooDeep { glyph, position } => write!(
                f,
                "glyph {glyph}: subroutines nested too deeply at byte {position}"
            ),
            CharStringTruncated { glyph, position } => {
                write!(f, "glyph {glyph}: charstring cut at byte {position}")
            }
            MissingEndchar { glyph } => {
                write!(f, "glyph {glyph}: the charstring ends without endchar")
            }
            VsindexOutOfRange { glyph, position } => {
                write!(f, "glyph {glyph}: vsindex out of range at byte {position}")
            }
            BlendWithoutStore { glyph, position } => write!(
                f,
                "glyph {glyph}: blend without a Variation Store at byte {position}"
            ),
            InvalidCharString { glyph, position } => {
                write!(f, "glyph {glyph}: invalid charstring at byte {position}")
            }
            UnknownOperator { operator } => write!(f, "unknown DICT operator {operator:#06x}"),
            OperatorNotAllowed { operator } => write!(
                f,
                "DICT operator {operator:#06x} is not allowed in this version"
            ),
            DuplicateOperator { operator } => {
                write!(f, "DICT operator {operator:#06x} appears more than once")
            }
            GlyphCountMismatch { found, expected } => {
                write!(f, "{found} charstrings, but maxp has {expected} glyphs")
            }
            SidOutOfRange { glyph, sid } => {
                write!(f, "SID {sid} of glyph {glyph} is beyond the String INDEX")
            }
            FdSelectFormat4 => f.write_str("FDSelect format 4 (FreeType cannot read it)"),
            UnknownCharStringOperator {
                glyph,
                position,
                operator,
            } => write!(
                f,
                "glyph {glyph}: unknown operator {operator:#06x} at byte {position}"
            ),
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
            CoverageNotSorted { index } => write!(
                f,
                "coverage record {index} does not come after the previous one"
            ),
            CoverageDuplicate { index, glyph } => {
                write!(f, "coverage record {index} repeats glyph id {glyph}")
            }
            CoverageIndexMismatch {
                index,
                expected,
                found,
            } => write!(
                f,
                "startCoverageIndex of range {index} is {found}, expected {expected}"
            ),
            ClassRangesOverlap { index } => write!(
                f,
                "class range {index} starts at or before the end of the previous one"
            ),
            CoverageCountMismatch { coverage, count } => {
                write!(f, "{count} entries for {coverage} covered glyphs")
            }
            LookupIndexOutOfRange { index, count } => {
                write!(f, "lookup index {index} is not below the {count} lookups")
            }
            FeatureIndexOutOfRange { index, count } => {
                write!(f, "feature index {index} is not below the {count} features")
            }
            SequenceIndexOutOfRange { index, count } => write!(
                f,
                "sequence index {index} is not below the input length {count}"
            ),
            MarkSetOutOfRange { index, count } => write!(
                f,
                "mark filtering set {index} is not below the {count} mark glyph sets"
            ),
            UnknownLookupType { found } => write!(f, "unknown lookup type {found}"),
            ExtensionTypeMismatch { expected, found } => write!(
                f,
                "extension subtable of type {found} in a lookup of type {expected}"
            ),
            NestedExtension => f.write_str("an extension subtable points to an extension"),
            EmptyInputSequence => f.write_str("an input sequence or ligature has no glyphs"),
            ReservedBitsSet { found } => write!(f, "reserved bits are set in {found:#06x}"),
            ColorIndexOutOfRange { index, count } => {
                write!(f, "color index {index} is not below the {count} colors")
            }
            LayerIndexOutOfRange { index, count } => {
                write!(f, "layer index {index} is not below the {count} layers")
            }
            LayoutTableDiscarded { table } => write!(f, "shaping ignores the {table} table after sanitization failed"),
            WorkLimitExceeded => {
                f.write_str("validation stopped: too many offsets for the size of the table, or too many problems")
            }
        }
    }
}
