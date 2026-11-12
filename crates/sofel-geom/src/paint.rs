//! Paint commands for color glyphs: the [`Painter`] trait and the values
//! it carries (ADR 0023).
//!
//! The commands and their order are those of HarfBuzz 15.1.0's
//! `hb_paint_funcs_t` (`src/hb-paint.h`); the transforms are built with
//! the formulas of `src/hb-geometry.hh` (`hb_transform_t`). Numbers are
//! `f32`, in font units with y growing upwards.

use crate::trigf;

/// A point with `f32` coordinates.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct PointF {
    /// x.
    pub x: f32,
    /// y.
    pub y: f32,
}

impl PointF {
    /// The point `(x, y)`.
    pub const fn new(x: f32, y: f32) -> PointF {
        PointF { x, y }
    }
}

/// A rectangle with `f32` edges.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    /// The left edge.
    pub x_min: f32,
    /// The bottom edge.
    pub y_min: f32,
    /// The right edge.
    pub x_max: f32,
    /// The top edge.
    pub y_max: f32,
}

/// A glyph's box (`hb_glyph_extents_t`): the top-left corner relative to
/// the glyph origin, the width (positive) and the height (negative for
/// glyphs that extend downward from the top).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct GlyphExtents {
    /// Left edge, from the origin.
    pub x_bearing: i32,
    /// Top edge, from the origin.
    pub y_bearing: i32,
    /// Width.
    pub width: i32,
    /// Height; negative when the box extends downward from the top.
    pub height: i32,
}

/// An affine transform, in HarfBuzz's order: a point `(x, y)` goes to
/// `(xx·x + xy·y + dx, yx·x + yy·y + dy)`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Transform {
    /// x scale.
    pub xx: f32,
    /// y shear of x.
    pub yx: f32,
    /// x shear of y.
    pub xy: f32,
    /// y scale.
    pub yy: f32,
    /// x translation.
    pub dx: f32,
    /// y translation.
    pub dy: f32,
}

impl Default for Transform {
    fn default() -> Transform {
        Transform::IDENTITY
    }
}

impl Transform {
    /// The identity.
    pub const IDENTITY: Transform = Transform {
        xx: 1.0,
        yx: 0.0,
        xy: 0.0,
        yy: 1.0,
        dx: 0.0,
        dy: 0.0,
    };

    /// `hb_transform_t::translation`.
    pub const fn translate(dx: f32, dy: f32) -> Transform {
        Transform {
            dx,
            dy,
            ..Transform::IDENTITY
        }
    }

    /// `hb_transform_t::scaling`.
    pub const fn scale(sx: f32, sy: f32) -> Transform {
        Transform {
            xx: sx,
            yy: sy,
            ..Transform::IDENTITY
        }
    }

    /// `hb_transform_t::scaling_around_center`.
    pub fn scale_around(sx: f32, sy: f32, cx: f32, cy: f32) -> Transform {
        Transform {
            xx: sx,
            yy: sy,
            dx: if cx != 0.0 { (1.0 - sx) * cx } else { 0.0 },
            dy: if cy != 0.0 { (1.0 - sy) * cy } else { 0.0 },
            ..Transform::IDENTITY
        }
    }

    /// `hb_transform_t::rotation`: counter-clockwise by `radians`.
    pub fn rotate(radians: f32) -> Transform {
        let (s, c) = trigf::sin_cos(radians);
        Transform {
            xx: c,
            yx: s,
            xy: -s,
            yy: c,
            dx: 0.0,
            dy: 0.0,
        }
    }

    /// `hb_transform_t::rotation_around_center`.
    pub fn rotate_around(radians: f32, cx: f32, cy: f32) -> Transform {
        let (s, c) = trigf::sin_cos(radians);
        Transform {
            xx: c,
            yx: s,
            xy: -s,
            yy: c,
            dx: (1.0 - c) * cx + s * cy,
            dy: -s * cx + (1.0 - c) * cy,
        }
    }

    /// `hb_transform_t::skewing`: `xy = tan(skew_x)`, `yx = tan(skew_y)`.
    pub fn skew(skew_x: f32, skew_y: f32) -> Transform {
        Transform {
            yx: if skew_y != 0.0 {
                trigf::tan(skew_y)
            } else {
                0.0
            },
            xy: if skew_x != 0.0 {
                trigf::tan(skew_x)
            } else {
                0.0
            },
            ..Transform::IDENTITY
        }
    }

    /// `hb_transform_t::skewing_around_center`.
    pub fn skew_around(skew_x: f32, skew_y: f32, cx: f32, cy: f32) -> Transform {
        let tx = if skew_x != 0.0 {
            trigf::tan(skew_x)
        } else {
            0.0
        };
        let ty = if skew_y != 0.0 {
            trigf::tan(skew_y)
        } else {
            0.0
        };
        Transform {
            xx: 1.0,
            yx: ty,
            xy: tx,
            yy: 1.0,
            dx: if cy != 0.0 { -tx * cy } else { 0.0 },
            dy: if cx != 0.0 { -ty * cx } else { 0.0 },
        }
    }

    /// `hb_transform_t::multiply`: the transform that applies `other`
    /// first and then `self`.
    pub fn multiply(&self, other: &Transform) -> Transform {
        let (a, b) = (self, other);
        Transform {
            xx: a.xx * b.xx + a.xy * b.yx,
            yx: a.yx * b.xx + a.yy * b.yx,
            xy: a.xx * b.xy + a.xy * b.yy,
            yy: a.yx * b.xy + a.yy * b.yy,
            dx: a.xx * b.dx + a.xy * b.dy + a.dx,
            dy: a.yx * b.dx + a.yy * b.dy + a.dy,
        }
    }

    /// The image of `p` (`hb_transform_t::transform_point`).
    pub fn apply(&self, p: PointF) -> PointF {
        PointF {
            x: self.dx + self.xx * p.x + self.xy * p.y,
            y: self.dy + self.yx * p.x + self.yy * p.y,
        }
    }
}

/// A color: sRGB, not premultiplied, 8 bits per component.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (opacity).
    pub a: u8,
}

impl Color {
    /// The color `(r, g, b, a)`.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }
}

/// Where a color comes from in the font.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ColorRef {
    /// The palette entry; `None` for the foreground color (index 0xFFFF
    /// in `COLR`).
    pub index: Option<u16>,
    /// The font's alpha (an `F2Dot14` plus variation deltas), clamped to
    /// 0..=1.
    pub alpha: f32,
}

/// A solid color: the color resolved from the chosen palette and the
/// foreground with HarfBuzz's rounding, and where it comes from, so that
/// a receiver can resolve it again for another palette.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Solid {
    /// The resolved color.
    pub color: Color,
    /// The palette entry and the alpha it was resolved from.
    pub source: ColorRef,
}

/// A color stop.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ColorStop {
    /// The position on the color line.
    pub offset: f32,
    /// The color.
    pub color: Solid,
}

/// How a gradient continues past its first and last stops.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Extend {
    /// The end colors continue.
    #[default]
    Pad,
    /// The stops repeat.
    Repeat,
    /// The stops repeat, every other time reversed.
    Reflect,
}

/// The stops of a gradient, in the order the font gives them, and how it
/// extends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ColorLine<'a> {
    /// The stops.
    pub stops: &'a [ColorStop],
    /// The extend mode.
    pub extend: Extend,
}

/// What fills the current clip.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Brush<'a> {
    /// A solid color.
    Solid(Solid),
    /// A linear gradient with COLR's three points: the line goes from
    /// `p0` to `p1`, rotated by `p2` (see [`reduce_linear_anchors`]).
    ///
    /// [`reduce_linear_anchors`]: crate::reduce_linear_anchors
    Linear {
        /// The stops.
        line: ColorLine<'a>,
        /// The start point.
        p0: PointF,
        /// The end point.
        p1: PointF,
        /// The rotation point.
        p2: PointF,
    },
    /// A two-point conical gradient.
    Radial {
        /// The stops.
        line: ColorLine<'a>,
        /// The start circle's center.
        c0: PointF,
        /// The start circle's radius.
        r0: f32,
        /// The end circle's center.
        c1: PointF,
        /// The end circle's radius.
        r1: f32,
    },
    /// A sweep gradient; angles in radians, counter-clockwise in font
    /// space (HarfBuzz's values).
    Sweep {
        /// The stops.
        line: ColorLine<'a>,
        /// The center.
        center: PointF,
        /// The start angle.
        start: f32,
        /// The end angle.
        end: f32,
    },
}

/// How a group is composited onto what is below it: the 28 modes of
/// `COLR`, with HarfBuzz's numbers (`hb_paint_composite_mode_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(u8)]
#[allow(missing_docs)]
pub enum CompositeMode {
    Clear = 0,
    Src = 1,
    Dest = 2,
    #[default]
    SrcOver = 3,
    DestOver = 4,
    SrcIn = 5,
    DestIn = 6,
    SrcOut = 7,
    DestOut = 8,
    SrcAtop = 9,
    DestAtop = 10,
    Xor = 11,
    Plus = 12,
    Screen = 13,
    Overlay = 14,
    Darken = 15,
    Lighten = 16,
    ColorDodge = 17,
    ColorBurn = 18,
    HardLight = 19,
    SoftLight = 20,
    Difference = 21,
    Exclusion = 22,
    Multiply = 23,
    HslHue = 24,
    HslSaturation = 25,
    HslColor = 26,
    HslLuminosity = 27,
}

impl CompositeMode {
    /// The mode numbered `v`; `None` past 27.
    pub fn from_u8(v: u8) -> Option<CompositeMode> {
        use CompositeMode::*;
        const ALL: [CompositeMode; 28] = [
            Clear,
            Src,
            Dest,
            SrcOver,
            DestOver,
            SrcIn,
            DestIn,
            SrcOut,
            DestOut,
            SrcAtop,
            DestAtop,
            Xor,
            Plus,
            Screen,
            Overlay,
            Darken,
            Lighten,
            ColorDodge,
            ColorBurn,
            HardLight,
            SoftLight,
            Difference,
            Exclusion,
            Multiply,
            HslHue,
            HslSaturation,
            HslColor,
            HslLuminosity,
        ];
        ALL.get(usize::from(v)).copied()
    }
}

/// The format of an [`Image`]'s data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ImageFormat {
    /// A PNG file.
    Png,
    /// An SVG document; `gzip` when it is gzip-compressed.
    Svg {
        /// Whether the document is gzip-compressed.
        gzip: bool,
    },
}

/// An image to draw: undecoded data from the font.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Image<'a> {
    /// The format.
    pub format: ImageFormat,
    /// The data.
    pub data: &'a [u8],
    /// The width in pixels (0 for SVG).
    pub width: u32,
    /// The height in pixels (0 for SVG).
    pub height: u32,
    /// HarfBuzz's deprecated image callback slant argument; always 0.0.
    /// Synthetic slant is a surrounding transform instead.
    pub slant: f32,
    /// Where the image goes, in font units; `None` for SVG documents,
    /// as HarfBuzz passes none.
    pub extents: Option<GlyphExtents>,
}

/// Receives the paint commands of a color glyph (ADR 0023): HarfBuzz's
/// paint functions (`hb_paint_funcs_t`), in the same order with the same
/// values. Units are font units with y growing upwards; no scale is
/// applied at the root.
pub trait Painter {
    /// Applies `t` to everything until the matching
    /// [`Painter::pop_transform`].
    fn push_transform(&mut self, t: Transform);
    /// Ends the innermost transform.
    fn pop_transform(&mut self);
    /// Clips to the outline of `glyph` until the matching
    /// [`Painter::pop_clip`].
    fn push_clip_glyph(&mut self, glyph: u32);
    /// Clips to `r` until the matching [`Painter::pop_clip`].
    fn push_clip_rectangle(&mut self, r: Rect);
    /// Ends the innermost clip.
    fn pop_clip(&mut self);
    /// Fills the current clip with `brush`.
    fn fill(&mut self, brush: &Brush<'_>);
    /// Fills the outline of `glyph` with `color`. By default
    /// [`Painter::push_clip_glyph`], [`Painter::fill`] and
    /// [`Painter::pop_clip`].
    fn fill_glyph(&mut self, glyph: u32, color: &Solid) {
        self.push_clip_glyph(glyph);
        self.fill(&Brush::Solid(*color));
        self.pop_clip();
    }
    /// Draws `image`; `false` when it cannot.
    fn image(&mut self, image: &Image<'_>) -> bool;
    /// Starts a group that is composited with `mode` when it ends.
    fn push_group(&mut self, mode: CompositeMode);
    /// Ends the innermost group, compositing it with `mode`.
    fn pop_group(&mut self, mode: CompositeMode);
    /// Called for a `PaintColrGlyph` of `glyph`: `true` when the painter
    /// drew that color glyph itself (for example from its own cache), so
    /// that its paint is not walked. By default `false`.
    fn color_glyph(&mut self, glyph: u32) -> bool {
        let _ = glyph;
        false
    }
}
