// FT_Vector_NormLen's reciprocal-length iteration. Intermediate products
// intentionally wrap in 32 bits, as in the pinned FreeType implementation.
pub(super) fn unit_vector(x: i32, y: i32) -> (i32, i32) {
    if x == 0 || y == 0 {
        return (x.signum() * 0x10000, y.signum() * 0x10000);
    }
    let (mut u, mut v) = (x.unsigned_abs(), y.unsigned_abs());
    let estimate = |u: u32, v: u32| u.max(v) + (u.min(v) >> 1);
    let mut length = estimate(u, v);
    let zeros = length.leading_zeros();
    let shift = zeros as i32 - 15 - i32::from(length >= (0xaaaa_aaaa >> zeros));
    if shift > 0 {
        u <<= shift;
        v <<= shift;
        length = estimate(u, v);
    } else {
        u >>= -shift;
        v >>= -shift;
        length >>= -shift;
    }
    let (u, v) = (u as i32, v as i32);
    let mut b = 0x10000 - length as i32;
    loop {
        let nx = u + (u.wrapping_mul(b) >> 16);
        let ny = v + (v.wrapping_mul(b) >> 16);
        let residual = nx
            .wrapping_mul(nx)
            .wrapping_add(ny.wrapping_mul(ny))
            .wrapping_neg()
            / 512;
        let update = residual.wrapping_mul((0x10000 + b) >> 8) / 0x10000;
        if update <= 0 {
            return (nx * x.signum(), ny * y.signum());
        }
        b += update;
    }
}
