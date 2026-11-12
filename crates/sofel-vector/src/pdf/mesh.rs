//! Type 6 Coons patches for sweep gradients, from HarfBuzz hb-vector-paint-pdf.cc.
use super::{PdfPage, number};
use alloc::{format, vec::Vec};
use core::fmt::Write as _;
use sofel_geom::{Color, ColorLine, PointF, float::ceilf, sin_cos, sweep_gradient_tiles, tan};

const RADIUS: f32 = 32767.;

fn offset(p: PointF, x: f32, y: f32, scale: f32) -> PointF {
    PointF::new(p.x + scale * x, p.y + scale * y)
}
fn between(p: PointF, q: PointF, n: f32) -> PointF {
    PointF::new(p.x + n * (q.x - p.x) / 3., p.y + n * (q.y - p.y) / 3.)
}

#[derive(Default)]
struct Mesh {
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

impl Mesh {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn sector(&mut self, center: PointF, a: f32, c: Color, b: f32, d: Color) {
        let n = ceilf((b - a).abs() / core::f32::consts::FRAC_PI_2).max(1.) as u8;
        for i in 0..n {
            let k = f32::from(i) / f32::from(n);
            let l = f32::from(i.saturating_add(1)) / f32::from(n);
            let (start, end) = (a + k * (b - a), a + l * (b - a));
            let (s0, c0) = sin_cos(start);
            let (s1, c1) = sin_cos(end);
            let curvature = (4. / 3.) * tan((end - start) / 4.);
            let p0 = offset(center, c0, s0, 0.5);
            let p3 = offset(center, c0, s0, RADIUS);
            let p6 = offset(center, c1, s1, RADIUS);
            let p9 = offset(center, c1, s1, 0.5);
            let points = [
                p0,
                between(p0, p3, 1.),
                between(p0, p3, 2.),
                p3,
                offset(p3, -s0, c0, curvature * RADIUS),
                offset(p6, -s1, c1, -curvature * RADIUS),
                p6,
                between(p6, p9, 1.),
                between(p6, p9, 2.),
                p9,
                offset(p9, -s1, c1, curvature * 0.5),
                offset(p0, -s0, c0, -curvature * 0.5),
            ];
            let mut geometry = [[0u8; 4]; 12];
            for (dst, p) in geometry.iter_mut().zip(points) {
                for (bytes, (v, mid)) in dst
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip([(p.x, center.x), (p.y, center.y)])
                {
                    let lo = mid - RADIUS - 1.;
                    let hi = mid + RADIUS + 1.;
                    let t = ((v - lo) / (hi - lo)).clamp(0., 1.);
                    bytes.copy_from_slice(&((t * 65535. + 0.5) as u16).to_be_bytes());
                }
            }
            self.rgb.push(0);
            self.rgb.extend_from_slice(geometry.as_flattened());
            if let Some(alpha) = &mut self.alpha {
                alpha.push(0);
                alpha.extend_from_slice(geometry.as_flattened());
            }
            let (first, last) = (
                sofel_geom::interpolate_sweep_color(c, d, k),
                sofel_geom::interpolate_sweep_color(c, d, l),
            );
            for color in [first, first, last, last] {
                self.rgb.extend_from_slice(&[color.r, color.g, color.b]);
                if let Some(alpha) = &mut self.alpha {
                    alpha.push(color.a);
                }
            }
        }
    }
}

impl PdfPage {
    fn mesh_object(&mut self, data: &[u8], decode: &str, alpha: bool) -> usize {
        let (space, components) = if alpha {
            ("DeviceGray", "0 1")
        } else {
            ("DeviceRGB", "0 1 0 1 0 1")
        };
        let dict = format!(
            "<< /ShadingType 6 /ColorSpace /{space} /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [{decode} {components}] /Length {} >>\nstream\n",
            data.len()
        );
        let mut obj = dict.into_bytes();
        obj.extend_from_slice(data);
        obj.extend_from_slice(b"\nendstream");
        let id = self.meshes.len().saturating_add(5);
        self.meshes.push(obj);
        id
    }

    pub(super) fn sweep(
        &mut self,
        line: ColorLine<'_>,
        center: PointF,
        start: f32,
        end: f32,
    ) -> bool {
        let mut stops = line.stops.to_vec();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let mut mesh = Mesh {
            rgb: Vec::new(),
            alpha: stops.iter().any(|s| s.color.color.a != 255).then(Vec::new),
        };
        sweep_gradient_tiles(&mut stops, line.extend, start, end, &mut |a, c, b, d| {
            if mesh.rgb.len() <= 16 << 20 {
                mesh.sector(center, a, c, b, d);
            }
        });
        if mesh.rgb.len() > 16 << 20 {
            return false;
        }
        if mesh.rgb.is_empty() {
            return true;
        }
        let [x0, x1, y0, y1] = [
            center.x - RADIUS - 1.,
            center.x + RADIUS + 1.,
            center.y - RADIUS - 1.,
            center.y + RADIUS + 1.,
        ]
        .map(|v| number(v, 2));
        let decode = format!("{x0} {x1} {y0} {y1}");
        let rgb = self.mesh_object(&mesh.rgb, &decode, false);
        let shade = self.shadings.len();
        self.shadings.push(format!("{rgb} 0 R"));
        if let Some(alpha) = mesh.alpha {
            let gray = self.mesh_object(&alpha, &decode, true);
            let mask = self.soft_masks.len();
            self.soft_masks.push(format!("<< /Type /XObject /Subtype /Form /BBox [{x0} {y0} {x1} {y1}] /Group << /S /Transparency /CS /DeviceGray >> /Resources << /Shading << /SHa {gray} 0 R >> >> /Length 8 >>\nstream\n/SHa sh\nendstream"));
            let _ = writeln!(self.stream, "/Mask{mask} gs");
        }
        let _ = writeln!(self.stream, "/SH{shade} sh");
        true
    }
}
