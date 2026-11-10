//! Features and their string syntax (`hb_feature_t`).

use sofel_font::read::Tag;

/// A feature setting for a range of clusters (`hb_feature_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Feature {
    /// The feature tag.
    pub tag: Tag,
    /// The value: 0 disables, 1 enables, higher values pick alternates.
    pub value: u32,
    /// The first cluster the setting applies to.
    pub start: u32,
    /// The cluster after the last one it applies to; `u32::MAX` for the
    /// end of the buffer.
    pub end: u32,
}

impl Feature {
    /// The start of a feature that applies everywhere.
    pub const GLOBAL_START: u32 = 0;
    /// The end of a feature that applies everywhere.
    pub const GLOBAL_END: u32 = u32::MAX;

    /// A feature for the whole buffer.
    pub const fn new(tag: Tag, value: u32) -> Feature {
        Feature {
            tag,
            value,
            start: Feature::GLOBAL_START,
            end: Feature::GLOBAL_END,
        }
    }
    /// Whether the feature applies to the whole buffer.
    pub const fn is_global(&self) -> bool {
        self.start == Feature::GLOBAL_START && self.end == Feature::GLOBAL_END
    }
}
