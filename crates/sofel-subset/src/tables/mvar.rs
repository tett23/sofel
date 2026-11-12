//! Bake MVAR records into the metric fields written by a subset.

use sofel_font::Font;
use sofel_font::read::{Tag, be, mvar::Mvar};
use sofel_font::var::{HbF32, Location, StoreKind};

use crate::{Plan, SubsetError};

type Field = (&'static [u8; 4], usize);
const OS2: &[Field] = &[
    (b"hasc", 68),
    (b"hdsc", 70),
    (b"hlgp", 72),
    (b"hcla", 74),
    (b"hcld", 76),
    (b"sbxs", 10),
    (b"sbys", 12),
    (b"sbxo", 14),
    (b"sbyo", 16),
    (b"spxs", 18),
    (b"spys", 20),
    (b"spxo", 22),
    (b"spyo", 24),
    (b"strs", 26),
    (b"stro", 28),
    (b"xhgt", 86),
    (b"cpht", 88),
];
const HHEA: &[Field] = &[(b"hcrs", 18), (b"hcrn", 20), (b"hcof", 22)];
const VHEA: &[Field] = &[(b"vcrs", 18), (b"vcrn", 20), (b"vcof", 22)];
const TYPO: &[Field] = &[(b"hasc", 4), (b"hdsc", 6), (b"hlgp", 8)];
const POST: &[Field] = &[(b"undo", 8), (b"unds", 10)];

pub(super) fn apply(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    bytes: &mut [u8],
) -> Result<(), SubsetError> {
    let Some(data) = font.table(Tag::new(b"MVAR")) else {
        return Ok(());
    };
    if !plan.var.instancing() {
        return Ok(());
    }
    let fields = match &tag.0 {
        b"OS/2" => OS2,
        b"post" => POST,
        b"hhea" | b"HHEA" => HHEA,
        b"vhea" | b"VHEA" => VHEA,
        _ => return Ok(()),
    };
    let Ok(table) = Mvar::new(data) else {
        return Ok(());
    };
    let sync_typo = matches!(&tag.0, b"hhea" | b"HHEA")
        && font
            .table(Tag::new(b"OS/2"))
            .and_then(|t| t.get(68..74))
            .is_some_and(|t| Some(t) == bytes.get(4..10));
    let location = Location::from_normalized_2_14(font, &plan.var.n.normalized_coords);
    let instance = font.instance(&location);
    for &(name, offset) in fields.iter().chain(if sync_typo { TYPO } else { &[] }) {
        let Some(record) = table.find(Tag::new(name)) else {
            continue;
        };
        let Some(base) = be::u16_at(bytes, offset) else {
            continue;
        };
        let delta = instance
            .item_delta::<HbF32>(StoreKind::Mvar, record.outer, record.inner)
            .map_err(|_| SubsetError::Malformed {
                tag: Tag::new(b"MVAR"),
            })?;
        let unsigned = matches!(name, b"hcla" | b"hcld");
        let base = if unsigned {
            f32::from(base)
        } else {
            f32::from(base.cast_signed())
        };
        let value = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(base + delta));
        let failed = SubsetError::Failed { tag };
        let word = if unsigned {
            u16::try_from(value).map_err(|_| failed)?
        } else if sync_typo && offset < 10 {
            crate::instance::lo16(value)
        } else {
            i16::try_from(value).map_err(|_| failed)?.cast_unsigned()
        };
        bytes
            .get_mut(offset..offset.saturating_add(2))
            .ok_or(failed)?
            .copy_from_slice(&word.to_be_bytes());
    }
    Ok(())
}
