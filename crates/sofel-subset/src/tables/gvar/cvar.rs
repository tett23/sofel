//! Rebase control value tuples. Unlike glyph variations these have one
//! delta per selected point and do not infer values for omitted points.
use super::{
    BTreeMap, Font, Plan, Serializer, SubsetError, Tables, Tag, Vec, iup, pack_deltas,
    variation_region,
};
use sofel_font::read::gvar::GlyphVariationData;
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32};

struct ControlTuple {
    region: Vec<[f32; 3]>,
    deltas: BTreeMap<usize, f32>,
}

pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    out: &mut Serializer,
    _: &mut Tables,
) -> Result<bool, SubsetError> {
    let bad = SubsetError::Malformed { tag };
    let limit = SubsetError::Limit { tag };
    let n = &plan.var.n;
    let count = font.table(Tag::new(b"cvt ")).map_or(0, |v| v.len() / 2);
    let store =
        GlyphVariationData::from_cvar(src, u16::try_from(n.axis_tags.len()).map_err(|_| limit)?)
            .ok_or(bad)?;
    let changes = variation_region::changes(font, plan);
    let mut groups = Vec::new();
    let mut indices = BTreeMap::new();
    for tuple in store.tuples() {
        let tuple = tuple.ok_or(bad)?;
        let peak = tuple.embedded_peak().ok_or(bad)?;
        let mut region = Vec::new();
        for axis in 0..n.axis_tags.len() {
            let middle = peak.axis(axis).ok_or(bad)?;
            let (start, end) = if let Some((a, b)) = tuple.intermediate() {
                (a.axis(axis).ok_or(bad)?, b.axis(axis).ok_or(bad)?)
            } else {
                (middle.min(0), middle.max(0))
            };
            region.push([start, middle, end].map(|v| f32::from(v) / 16384.0));
        }
        let deltas = tuple.cvt_deltas(count, store.shared_points()).ok_or(bad)?;
        let points: Vec<_> = match tuple
            .points()
            .or(store.shared_points())
            .filter(|p| !p.is_all())
        {
            Some(p) => p.iter().map(|i| i as usize).collect(),
            None => (0..count).collect(),
        };
        for (region, scales) in variation_region::split(region, &changes, 4095, tag)? {
            let region: Vec<_> = n
                .retained_axis_indices
                .iter()
                .map(|&axis| region.get(axis).copied().ok_or(bad))
                .collect::<Result<_, _>>()?;
            if region.iter().all(|r| r[1] == 0.0) {
                continue;
            }
            let key: Vec<_> = region
                .iter()
                .map(|r| r.map(|v| if v == 0.0 { 0 } else { v.to_bits() }))
                .collect();
            let pos = *indices.entry(key).or_insert_with(|| {
                let i = groups.len();
                groups.push(ControlTuple {
                    region,
                    deltas: BTreeMap::new(),
                });
                i
            });
            let values = &mut groups.get_mut(pos).ok_or(bad)?.deltas;
            for (&point, delta) in points.iter().zip(deltas.iter()) {
                if point >= count {
                    return Err(bad);
                }
                *values.entry(point).or_default() +=
                    scales.iter().fold(f32::from(delta), |v, s| v * s);
            }
        }
    }
    let mut headers = Vec::new();
    let mut payload = Vec::new();
    let mut tuples = 0u16;
    for ControlTuple {
        region,
        deltas: values,
    } in groups
    {
        let mut selected = alloc::vec![false;count];
        let mut deltas = Vec::new();
        for (i, value) in values {
            *selected.get_mut(i).ok_or(bad)? = true;
            deltas.push(crate::instance::lo16(hb_clamp_to_i32(hb_roundf32(value))).cast_signed());
        }
        let start = payload.len();
        payload.extend(iup::pack_points(&selected).ok_or(limit)?);
        pack_deltas(&deltas, &mut payload);
        headers.extend(
            u16::try_from(payload.len().saturating_sub(start))
                .map_err(|_| limit)?
                .to_be_bytes(),
        );
        let intermediate = region
            .iter()
            .any(|&[a, b, c]| a != b.min(0.0) || c != b.max(0.0));
        let flags: u16 = if intermediate { 0xe000 } else { 0xa000 };
        headers.extend(flags.to_be_bytes());
        for &column in if intermediate {
            &[1, 0, 2][..]
        } else {
            &[1][..]
        } {
            for tent in &region {
                let v = tent.get(column).ok_or(bad)?;
                headers.extend(
                    crate::instance::lo16(hb_clamp_to_i32(hb_roundf32(*v * 16384.0))).to_be_bytes(),
                );
            }
        }
        tuples = tuples.checked_add(1).filter(|n| *n <= 4095).ok_or(limit)?;
    }
    if tuples == 0 {
        return Ok(false);
    }
    out.u32(0x0001_0000);
    out.u16(tuples);
    out.u16(u16::try_from(headers.len().saturating_add(8)).map_err(|_| limit)?);
    out.extend(&headers);
    out.extend(&payload);
    Ok(true)
}
