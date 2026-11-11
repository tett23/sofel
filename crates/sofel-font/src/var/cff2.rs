//! CFF2 blend columns in the order of the ItemVariationData (ADR 0030).

use crate::CffError;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use sofel_read::cff::dict::BlendWeights;
use sofel_read::item_variation::ItemVariationStore;

use super::{
    Arith, Cff, Ft, FtLocationError, HbF64, Instance, SourceFont, StoreKind, StoreScalars,
};

/// Why a CFF2 blend vector cannot be evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendError {
    /// The font has no CFF2 Variation Store.
    NoStore,
    /// The CFF2 table or its Variation Store cannot be read.
    InvalidStore,
    /// The fvar and CFF2 Variation Store axis counts disagree.
    AxisCountMismatch,
    /// The ItemVariationData index is outside the store.
    VsindexOutOfRange,
    /// A column refers to a region outside the store.
    RegionOutOfRange,
    /// FreeType rejected the location.
    FtLocation(FtLocationError),
}

/// The coefficient of each CFF2 blend delta column, including zero columns.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendScalars<S> {
    values: Vec<S>,
}

impl<S: Copy> BlendScalars<S> {
    /// Number of delta columns, excluding the default-value column.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether there are no delta columns.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Coefficients in the original column order.
    pub fn iter(&self) -> impl Iterator<Item = S> + '_ {
        self.values.iter().copied()
    }
}

impl<'font, 'data, 'loc, 'source> Instance<'font, 'data, 'loc, 'source> {
    /// CFF2 blend scalars in ItemVariationData column order.
    pub fn blend_scalars<A: Arith>(
        &self,
        vsindex: u16,
    ) -> Result<BlendScalars<A::Scalar>, BlendError> {
        if A::checks_ft_error()
            && let Some(error) = self.location.ft_error
        {
            return Err(BlendError::FtLocation(error));
        }
        let cff = Cff::new(self.font).map_err(|error| match error {
            CffError::NotCff => BlendError::NoStore,
            _ => BlendError::InvalidStore,
        })?;
        let store = cff.variation_store().ok_or(BlendError::NoStore)?;
        if cff.fvar_axes().is_some_and(|n| n != store.axis_count()) {
            return Err(BlendError::AxisCountMismatch);
        }
        let data = store
            .item_data(vsindex)
            .ok_or(BlendError::VsindexOutOfRange)?;
        let same = self.location.source.is_some_and(|source: SourceFont<'_>| {
            source.matches(self.font.sfnt.file(), self.font.face_index)
        });
        let cached = if same {
            A::cached(self.location, StoreKind::Cff2)
        } else {
            None
        };
        let live;
        let regions = if let Some(cached) = cached {
            cached.regions()
        } else {
            live = StoreScalars::from_store::<A>(
                &store,
                A::coords(self.location),
                A::supplied(self.location),
            );
            live.regions()
        };
        select_columns(&store, &data, regions)
    }

    /// CFF2 region scalars for HarfBuzz boxes, borrowed from this location when possible.
    pub fn cff2_hb_region_scalars<'a>(&'a self, store: &ItemVariationStore<'_>) -> Cow<'a, [f32]> {
        let same = self.location.source.is_some_and(|source: SourceFont<'_>| {
            source.matches(self.font.sfnt.file(), self.font.face_index)
        });
        if same
            && let Some(cached) = HbF64::cached(self.location, StoreKind::Cff2)
            && cached.regions().len() == usize::from(store.region_count())
        {
            return Cow::Borrowed(cached.regions());
        }
        Cow::Owned(
            StoreScalars::from_store::<HbF64>(store, self.coords_normalized(), true)
                .regions()
                .to_vec(),
        )
    }
}

fn select_columns<S: Copy>(
    store: &ItemVariationStore<'_>,
    data: &sofel_read::item_variation::ItemVariationData<'_>,
    regions: &[S],
) -> Result<BlendScalars<S>, BlendError> {
    let mut values = Vec::with_capacity(usize::from(data.region_index_count()));
    for column in 0..usize::from(data.region_index_count()) {
        let index = data
            .region_index(column)
            .ok_or(BlendError::RegionOutOfRange)?;
        if index >= store.region_count() {
            return Err(BlendError::RegionOutOfRange);
        }
        values.push(
            regions
                .get(usize::from(index))
                .copied()
                .ok_or(BlendError::RegionOutOfRange)?,
        );
    }
    Ok(BlendScalars { values })
}

enum FtScalars<'a> {
    Cached(&'a [i32]),
    Computed(StoreScalars<i32>),
}

impl FtScalars<'_> {
    fn get(&self, index: u16) -> Option<i32> {
        match self {
            Self::Cached(values) => values.get(usize::from(index)).copied(),
            Self::Computed(values) => values.regions().get(usize::from(index)).copied(),
        }
    }
}

/// A FreeType CFF2 blend adapter for a specific font and location.
pub struct FtBlendAt<'store, 'cache> {
    store: Option<ItemVariationStore<'store>>,
    scalars: Option<FtScalars<'cache>>,
    valid: bool,
}

impl BlendWeights for FtBlendAt<'_, '_> {
    fn regions(&self, vsindex: u32) -> Option<u32> {
        if !self.valid {
            return None;
        }
        let store = self.store?;
        let data = store.item_data(u16::try_from(vsindex).ok()?)?;
        let count = data.region_index_count();
        for column in 0..usize::from(count) {
            if data.region_index(column)? >= store.region_count() {
                return None;
            }
        }
        Some(u32::from(count))
    }

    fn weight(&self, vsindex: u32, column: u32) -> i32 {
        let Some(store) = self.store else { return 0 };
        let index = u16::try_from(vsindex)
            .ok()
            .and_then(|value| store.item_data(value))
            .and_then(|data| data.region_index(usize::try_from(column).ok()?));
        index
            .and_then(|index| self.scalars.as_ref()?.get(index))
            .unwrap_or(0)
    }
}

impl<'font, 'data, 'loc, 'source> Instance<'font, 'data, 'loc, 'source> {
    /// FreeType's CFF2 blend weights for this location, reusing cached region scalars.
    pub fn ft_blend_weights<'cache>(&'cache self, cff: &Cff<'data>) -> FtBlendAt<'data, 'cache> {
        let store = cff.variation_store();
        let valid = self.location.ft_error.is_none()
            && store.is_some_and(|store| {
                cff.fvar_axes()
                    .is_none_or(|count| count == store.axis_count())
            });
        let scalars = store.map(|store| {
            let same = self
                .location
                .source
                .is_some_and(|source| source.matches(self.font.sfnt.file(), self.font.face_index));
            if same && let Some(cached) = Ft::cached(self.location, StoreKind::Cff2) {
                return FtScalars::Cached(cached.regions());
            }
            FtScalars::Computed(StoreScalars::from_store::<Ft>(
                &store,
                Ft::coords(self.location),
                Ft::supplied(self.location),
            ))
        });
        FtBlendAt {
            store,
            scalars,
            valid,
        }
    }
}
