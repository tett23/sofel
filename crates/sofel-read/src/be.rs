//! Big-endian reads from byte slices. Every read is bounds checked and
//! returns `None` when the bytes are not there.

/// `len` bytes starting at `offset`.
#[inline]
pub fn slice_at(data: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    data.get(offset..offset.checked_add(len)?)
}

/// The bytes from `offset` to the end.
#[inline]
pub fn tail_at(data: &[u8], offset: usize) -> Option<&[u8]> {
    data.get(offset..)
}

/// `N` bytes starting at `offset` as a fixed-size array.
#[inline]
pub fn array_at<const N: usize>(data: &[u8], offset: usize) -> Option<&[u8; N]> {
    data.get(offset..)?.first_chunk::<N>()
}

/// A `u8` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn u8_at(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

/// An `i8` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn i8_at(data: &[u8], offset: usize) -> Option<i8> {
    array_at::<1>(data, offset).map(|a| i8::from_be_bytes(*a))
}

/// A big-endian `u16` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    array_at::<2>(data, offset).map(|a| u16::from_be_bytes(*a))
}

/// A big-endian `i16` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    array_at::<2>(data, offset).map(|a| i16::from_be_bytes(*a))
}

/// A big-endian 24-bit unsigned integer at `offset`, widened to `u32`. `None` if the bytes are not there.
#[inline]
pub fn u24_at(data: &[u8], offset: usize) -> Option<u32> {
    array_at::<3>(data, offset).map(|a| u24(*a))
}

/// A big-endian `u32` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    array_at::<4>(data, offset).map(|a| u32::from_be_bytes(*a))
}

/// A big-endian `i32` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
    array_at::<4>(data, offset).map(|a| i32::from_be_bytes(*a))
}

/// A big-endian `i64` at `offset`. `None` if the bytes are not there.
#[inline]
pub fn i64_at(data: &[u8], offset: usize) -> Option<i64> {
    array_at::<8>(data, offset).map(|a| i64::from_be_bytes(*a))
}

/// A 24-bit big-endian unsigned integer.
#[inline]
pub fn u24(a: [u8; 3]) -> u32 {
    let [x, y, z] = a;
    u32::from_be_bytes([0, x, y, z])
}

/// Converts a `u32` count or offset to `usize`. On targets where it does
/// not fit, saturates so that any slice built from it fails.
#[inline]
pub fn to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// Converts a `usize` offset or length to `u32` for diagnostics,
/// saturating at `u32::MAX`.
#[inline]
pub fn to_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// `count` elements of `N` bytes starting at `offset`, as fixed-size
/// chunks. `None` if they do not all fit.
#[inline]
pub fn chunks_at<const N: usize>(data: &[u8], offset: usize, count: usize) -> Option<&[[u8; N]]> {
    let bytes = slice_at(data, offset, count.checked_mul(N)?)?;
    let (chunks, rest) = bytes.as_chunks::<N>();
    rest.is_empty().then_some(chunks)
}
