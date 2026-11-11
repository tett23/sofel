//! Location-resolved values from FreeType's COLRv1 paint queries.

use sofel_outline::font::read::colr::{ColorLine, Paint};
use sofel_outline::font::read::delta_set_index_map::DeltaSetIndexMap;
use sofel_outline::font::var::{Ft, Instance, StoreKind};

use crate::{PaintRef, RawPaintGraph};

/// A checked COLR table paired with FreeType's variation coordinates.
#[derive(Clone, Copy, Debug)]
pub struct FtPaintGraph<'font, 'data, 'loc, 'source> {
    raw: RawPaintGraph<'data>,
    instance: Instance<'font, 'data, 'loc, 'source>,
    map: Option<DeltaSetIndexMap<'data>>,
}

/// A paint resolved at one location. Coordinates, angles and transforms
/// use FreeType's 16.16 values; solid alpha is raw signed 2.14.
/// Child offsets are relative to the same COLR table.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug)]
pub enum FtPaint<'a> {
    Layers {
        first: u32,
        count: u8,
    },
    Solid {
        palette: u16,
        alpha: i16,
    },
    Linear {
        points: [i64; 6],
        line: Option<ColorLine<'a>>,
    },
    Radial {
        circles: [i64; 6],
        line: Option<ColorLine<'a>>,
    },
    Sweep {
        values: [i64; 4],
        line: Option<ColorLine<'a>>,
    },
    Glyph {
        paint: Option<usize>,
        glyph: u32,
    },
    ColrGlyph(u16),
    Transform {
        paint: Option<usize>,
        affine: [i64; 6],
    },
    Translate {
        paint: Option<usize>,
        shift: [i64; 2],
    },
    Scale {
        paint: Option<usize>,
        values: [i64; 4],
    },
    Rotate {
        paint: Option<usize>,
        values: [i64; 3],
    },
    Skew {
        paint: Option<usize>,
        values: [i64; 4],
    },
    Composite {
        source: Option<usize>,
        mode: u8,
        backdrop: Option<usize>,
    },
}

// Operands are at most i32; adding them and shifting by up to 16 fits i64.
#[allow(clippy::arithmetic_side_effects)]
impl<'font, 'data, 'loc, 'source> FtPaintGraph<'font, 'data, 'loc, 'source> {
    /// Opens the table at this location. An invalid FreeType location has no deltas.
    pub fn new(instance: Instance<'font, 'data, 'loc, 'source>) -> Option<Self> {
        let raw = RawPaintGraph::new(instance.font())?;
        let map = raw.0.var_index_map().ok().flatten();
        Some(Self { raw, map, instance })
    }

    fn delta(&self, base: Option<u32>, offset: u16) -> i32 {
        let Some(base) = base.filter(|&v| v != u32::MAX) else {
            return 0;
        };
        let index = u64::from(base).saturating_add(u64::from(offset));
        let (outer, inner) = if let Some(map) = self.map.filter(|m| !m.is_empty()) {
            let [a, b, c, d] = map
                .map(u32::try_from(index).unwrap_or(u32::MAX))
                .to_be_bytes();
            (u16::from_be_bytes([a, b]), u16::from_be_bytes([c, d]))
        } else {
            let Ok(inner) = u16::try_from(index) else {
                return 0;
            };
            (0, inner)
        };
        self.instance
            .item_delta::<Ft>(StoreKind::Colr, outer, inner)
            .unwrap_or(0)
    }

    /// Returns the graph root for a glyph.
    pub fn base_paint(&self, glyph: u32) -> Option<PaintRef<'data>> {
        self.raw.base_paint(glyph)
    }

    /// Opens a paint in the LayerList.
    pub fn layer_paint(&self, index: usize) -> Option<PaintRef<'data>> {
        self.raw.layer_paint(index)
    }

    /// Returns the four clip coordinates in font units, before the size transform.
    pub fn clip_box(&self, glyph: u32) -> Option<[i64; 4]> {
        let b = self.raw.clip_box(glyph)?;
        let e = |v: i16, i| i64::from(v) + i64::from(self.delta(b.var_index_base, i));
        Some([e(b.x_min, 0), e(b.y_min, 1), e(b.x_max, 2), e(b.y_max, 3)])
    }

    /// Returns one stop as (16.16 offset, palette index, 2.14 alpha).
    pub fn color_stop(&self, line: ColorLine<'data>, i: usize) -> Option<(i64, u16, i16)> {
        let s = line.stop(i)?;
        #[allow(clippy::cast_possible_truncation)]
        let alpha = s
            .alpha
            .0
            .wrapping_add(self.delta(s.var_index_base, 1) as i16);
        Some((
            (i64::from(s.offset.0) + i64::from(self.delta(s.var_index_base, 0))) * 4,
            s.palette_index,
            alpha,
        ))
    }

    /// Resolves a paint from this graph, including variable geometry.
    pub fn paint(&self, reference: PaintRef<'data>) -> Option<FtPaint<'data>> {
        let unit = |v: i16, base, i| (i64::from(v) + i64::from(self.delta(base, i))) * 65536;
        let angle = |v: sofel_outline::font::read::F2Dot14, base, i| {
            (i64::from(v.0) + i64::from(self.delta(base, i))) * 4
        };
        let center = |v: Option<(i16, i16)>, base, i: u16| {
            v.map_or([0, 0], |(x, y)| {
                [unit(x, base, i), unit(y, base, i.saturating_add(1))]
            })
        };
        Some(match reference.paint()? {
            Paint::ColrLayers {
                num_layers,
                first_layer,
            } => FtPaint::Layers {
                first: first_layer,
                count: num_layers,
            },
            Paint::Solid {
                palette_index,
                alpha,
                var_index_base: b,
            } => {
                #[allow(clippy::cast_possible_truncation)]
                let alpha = alpha.0.wrapping_add(self.delta(b, 0) as i16);
                FtPaint::Solid {
                    palette: palette_index,
                    alpha,
                }
            }
            Paint::LinearGradient {
                points,
                color_line,
                var_index_base: b,
            } => FtPaint::Linear {
                points: {
                    let [x0, y0, x1, y1, x2, y2] = points;
                    [
                        unit(x0, b, 0),
                        unit(y0, b, 1),
                        unit(x1, b, 2),
                        unit(y1, b, 3),
                        unit(x2, b, 4),
                        unit(y2, b, 5),
                    ]
                },
                line: color_line,
            },
            Paint::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_line,
                var_index_base: b,
            } => {
                let radius = |r: u16, i| {
                    (i64::from(r) * 65536).min(i64::from(i32::MAX))
                        + i64::from(self.delta(b, i)) * 65536
                };
                FtPaint::Radial {
                    circles: [
                        unit(c0.0, b, 0),
                        unit(c0.1, b, 1),
                        radius(r0, 2),
                        unit(c1.0, b, 3),
                        unit(c1.1, b, 4),
                        radius(r1, 5),
                    ],
                    line: color_line,
                }
            }
            Paint::SweepGradient {
                center: (x, y),
                start_angle,
                end_angle,
                color_line,
                var_index_base: b,
            } => FtPaint::Sweep {
                values: [
                    unit(x, b, 0),
                    unit(y, b, 1),
                    angle(start_angle, b, 2),
                    angle(end_angle, b, 3),
                ],
                line: color_line,
            },
            Paint::Glyph { paint, glyph } => FtPaint::Glyph { paint, glyph },
            Paint::ColrGlyph { glyph } => FtPaint::ColrGlyph(glyph),
            Paint::Transform { paint, transform } => {
                let a = transform?;
                let field = |v: sofel_outline::font::read::Fixed, index| {
                    i64::from(v.0) + i64::from(self.delta(a.var_index_base, index))
                };
                FtPaint::Transform {
                    paint,
                    affine: [
                        field(a.xx, 0),
                        field(a.yx, 1),
                        field(a.xy, 2),
                        field(a.yy, 3),
                        field(a.dx, 4),
                        field(a.dy, 5),
                    ],
                }
            }
            Paint::Translate {
                paint,
                delta: (x, y),
                var_index_base: b,
            } => FtPaint::Translate {
                paint,
                shift: [unit(x, b, 0), unit(y, b, 1)],
            },
            Paint::Scale {
                paint,
                scale,
                center: c,
                var_index_base: b,
            } => {
                let y = u16::from(!matches!(reference.format(), Some(20..=23)));
                let [x0, y0] = center(c, b, y.saturating_add(1));
                FtPaint::Scale {
                    paint,
                    values: [angle(scale.0, b, 0), angle(scale.1, b, y), x0, y0],
                }
            }
            Paint::Rotate {
                paint,
                angle: a,
                center: c,
                var_index_base: b,
            } => {
                let [x, y] = center(c, b, 1);
                FtPaint::Rotate {
                    paint,
                    values: [angle(a, b, 0), x, y],
                }
            }
            Paint::Skew {
                paint,
                angles,
                center: c,
                var_index_base: b,
            } => {
                let [x, y] = center(c, b, 2);
                FtPaint::Skew {
                    paint,
                    values: [angle(angles.0, b, 0), angle(angles.1, b, 1), x, y],
                }
            }
            Paint::Composite {
                source,
                mode,
                backdrop,
            } => FtPaint::Composite {
                source,
                mode,
                backdrop,
            },
        })
    }
}
