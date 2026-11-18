//! `sofel_geom::float::atan2f` as the C function `atan2f` (ADR 0095).
//!
//! `tools/hb-raster-reference/build.sh` links this static library before
//! the C library, so HarfBuzz's raster calls sofel's `atan2f` instead of
//! the platform's, and its sweep gradients give the angles sofel uses on
//! every platform.

#![no_std]

/// C's `float atan2f(float y, float x)`.
// Exporting a C symbol is what this library is for; the function itself
// is safe Rust.
#[unsafe(no_mangle)]
pub extern "C" fn atan2f(y: f32, x: f32) -> f32 {
    sofel_geom::float::atan2f(y, x)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
