//! Common layout records checked when HarfBuzz opens GSUB or GPOS.
use super::children::Budget;
use crate::be::{u16_at, u32_at};

struct Records<'a> {
    bytes: &'a [u8],
    base: &'a [u8],
}

impl<'a> Records<'a> {
    fn read(base: &'a [u8], at: usize) -> Option<Self> {
        let count = usize::from(u16_at(base, at)?);
        Some(Self {
            bytes: crate::be::slice_at(base, at.checked_add(2)?, count.checked_mul(6)?)?,
            base,
        })
    }

    fn visit(&self, mut check: impl FnMut(u32, &[u8]) -> bool) -> bool {
        self.bytes.as_chunks::<6>().0.iter().all(|r| {
            let Some(offset) = u16_at(r, 4) else {
                return false;
            };
            offset == 0
                || self
                    .base
                    .get(usize::from(offset)..)
                    .is_some_and(|target| check(u32_at(r, 0).unwrap_or(0), target))
        })
    }
}

fn array_fits(data: &[u8], at: usize, stride: usize) -> bool {
    u16_at(data, at).is_some_and(|n| {
        crate::be::slice_at(
            data,
            at.saturating_add(2),
            stride.saturating_mul(usize::from(n)),
        )
        .is_some()
    })
}

fn lang_sys(data: &[u8], budget: &mut Budget) -> bool {
    array_fits(data, 4, 2)
        && budget.take(usize::from(u16_at(data, 4).unwrap_or(0)).saturating_mul(2))
}

pub(super) fn scripts(data: &[u8], budget: &mut Budget) -> bool {
    Records::read(data, 0).is_some_and(|records| {
        budget.take(records.bytes.len())
            && records.visit(|_, script| {
                let Some(default) = u16_at(script, 0) else {
                    return false;
                };
                (default == 0
                    || script
                        .get(usize::from(default)..)
                        .is_some_and(|d| lang_sys(d, budget)))
                    && Records::read(script, 2).is_some_and(|langs| {
                        budget.take(langs.bytes.len())
                            && langs.visit(|_, lang| lang_sys(lang, budget))
                    })
            })
    })
}

pub(super) fn feature(tag: u32, data: &[u8], budget: &mut Budget) -> bool {
    let Some(offset) = u16_at(data, 0) else {
        return false;
    };
    if !array_fits(data, 2, 2)
        || !budget.take(usize::from(u16_at(data, 2).unwrap_or(0)).saturating_mul(2))
    {
        return false;
    }
    if offset == 0 {
        return true;
    }
    let tag = tag.to_be_bytes();
    let params = data.get(usize::from(offset)..).unwrap_or_default();
    match &tag {
        b"size" => {
            let (Some(design), Some(family), Some(name), Some(start), Some(end)) = (
                u16_at(params, 0),
                u16_at(params, 2),
                u16_at(params, 4),
                u16_at(params, 6),
                u16_at(params, 8),
            ) else {
                return false;
            };
            design != 0
                && ((family, name, start, end) == (0, 0, 0, 0)
                    || ((start..=end).contains(&design) && (256..=32767).contains(&name)))
        }
        [b's', b's', _, _] => params.len() >= 4,
        [b'c', b'v', _, _] => array_fits(params, 12, 3),
        _ => true,
    }
}

pub(super) fn features(data: &[u8], budget: &mut Budget) -> bool {
    Records::read(data, 0).is_some_and(|records| {
        budget.take(records.bytes.len()) && records.visit(|tag, data| feature(tag, data, budget))
    })
}
