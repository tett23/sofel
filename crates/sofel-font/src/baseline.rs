//! The baseline tags of HarfBuzz's `hb_ot_layout_baseline_tag_t`
//! (ADR 0017), for [`Instance::baseline`](crate::var::Instance::baseline) and
//! [`Instance::baseline_with_fallback`](crate::var::Instance::baseline_with_fallback).

use sofel_read::Tag;

/// `romn`: the alphabetic baseline.
pub const ROMAN: Tag = Tag::new(b"romn");
/// `hang`: the hanging baseline (the top of Devanagari and similar
/// scripts).
pub const HANGING: Tag = Tag::new(b"hang");
/// `icfb`: the bottom (horizontal) or left (vertical) of the ideographic
/// character face.
pub const IDEO_FACE_BOTTOM_OR_LEFT: Tag = Tag::new(b"icfb");
/// `icft`: the top (horizontal) or right (vertical) of the ideographic
/// character face.
pub const IDEO_FACE_TOP_OR_RIGHT: Tag = Tag::new(b"icft");
/// `Icfc`: the center of the ideographic character face.
pub const IDEO_FACE_CENTRAL: Tag = Tag::new(b"Icfc");
/// `ideo`: the bottom or left of the ideographic em box.
pub const IDEO_EMBOX_BOTTOM_OR_LEFT: Tag = Tag::new(b"ideo");
/// `idtp`: the top or right of the ideographic em box.
pub const IDEO_EMBOX_TOP_OR_RIGHT: Tag = Tag::new(b"idtp");
/// `Idce`: the center of the ideographic em box.
pub const IDEO_EMBOX_CENTRAL: Tag = Tag::new(b"Idce");
/// `math`: the mathematical baseline.
pub const MATH: Tag = Tag::new(b"math");

/// Every tag, in the order of `hb_ot_layout_baseline_tag_t`'s
/// documentation.
pub const ALL: [Tag; 9] = [
    ROMAN,
    HANGING,
    IDEO_FACE_BOTTOM_OR_LEFT,
    IDEO_FACE_TOP_OR_RIGHT,
    IDEO_FACE_CENTRAL,
    IDEO_EMBOX_BOTTOM_OR_LEFT,
    IDEO_EMBOX_TOP_OR_RIGHT,
    IDEO_EMBOX_CENTRAL,
    MATH,
];
