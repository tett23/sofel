//! The floating-point helpers moved to [`sofel_geom::float`] (ADR 0026).
//!
//! These re-exports, under the old names, keep code written against the
//! old module compiling while ADR 0020 and ADR 0023 are implemented in
//! parallel; remove the module once they have merged and use
//! `sofel_geom::float` directly.

#[allow(unused_imports)]
pub(crate) use sofel_geom::float::{
    ceilf, f32_to_i64, floor, floorf, hb_clamp_to_i32 as f64_to_i32, hb_roundf32 as roundf,
    i32_to_f32, i64_to_f32, truncf,
};
