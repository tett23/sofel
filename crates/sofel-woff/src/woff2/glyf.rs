//! Coordinate triplets in the transformed WOFF2 glyph stream.

fn take<const N: usize>(input: &mut &[u8]) -> Option<[u8; N]> {
    let (value, tail) = input.split_first_chunk()?;
    *input = tail;
    Some(*value)
}

/// Decode signed x and y deltas. Bit 7 describes curve membership separately.
pub(super) fn triplet(flag: u8, input: &mut &[u8]) -> Option<(i32, i32)> {
    let code = flag & 127;
    let unsigned = match code {
        0..=9 => (
            0,
            (u16::from(code & 14) << 7) | u16::from(take::<1>(input)?[0]),
        ),
        10..=19 => (
            (u16::from(code.checked_sub(10)? & 14) << 7) | u16::from(take::<1>(input)?[0]),
            0,
        ),
        20..=83 => {
            let group = code.checked_sub(20)?;
            let [xy] = take(input)?;
            (
                1 + u16::from((group & 48) | (xy >> 4)),
                1 + u16::from(((group & 12) << 2) | (xy & 15)),
            )
        }
        84..=119 => {
            let group = code.checked_sub(84)?;
            let [x, y] = take(input)?;
            (
                ((u16::from(group / 12) << 8) | u16::from(x)).checked_add(1)?,
                ((u16::from((group % 12) >> 2) << 8) | u16::from(y)).checked_add(1)?,
            )
        }
        120..=123 => {
            let [a, b, c] = take(input)?;
            (
                (u16::from(a) << 4) | u16::from(b >> 4),
                (u16::from(b & 15) << 8) | u16::from(c),
            )
        }
        _ => {
            let [a, b, c, d] = take(input)?;
            (u16::from_be_bytes([a, b]), u16::from_be_bytes([c, d]))
        }
    };
    let signed = |n: u16, positive| {
        if positive {
            i32::from(n)
        } else {
            i32::from(n).saturating_neg()
        }
    };
    // The single-axis forms encode their sign in the low bit.
    Some((
        signed(unsigned.0, code & 1 != 0),
        signed(
            unsigned.1,
            if code < 10 {
                code & 1 != 0
            } else {
                code & 2 != 0
            },
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0065_i2_triplet_consumption_sign_and_truncation() {
        for (flag, bytes, point) in [
            (9, &[1][..], (0, 1025)),
            (18, &[2], (-1026, 0)),
            (23, &[0x34], (4, 5)),
            (84, &[0, 0], (-1, -1)),
            (123, &[0x12, 0x34, 0x56], (0x123, 0x456)),
            (125, &[0xab, 0xcd, 0xef, 1], (0xabcd, -0xef01)),
        ] {
            for curve in [0, 128] {
                let mut input = bytes;
                assert_eq!(triplet(flag | curve, &mut input), Some(point));
                assert!(input.is_empty());
                for n in 0..bytes.len() {
                    assert_eq!(
                        triplet(flag | curve, &mut bytes.get(..n).unwrap_or_default()),
                        None
                    );
                }
            }
        }
    }
}

fn take_bytes<'a>(input: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let (bytes, rest) = input.split_at_checked(count)?;
    *input = rest;
    Some(bytes)
}

fn uint255(input: &mut &[u8]) -> Option<u16> {
    Some(match take::<1>(input)?[0] {
        253 => u16::from_be_bytes(take(input)?),
        254 => 506 + u16::from(take::<1>(input)?[0]),
        255 => 253 + u16::from(take::<1>(input)?[0]),
        n => u16::from(n),
    })
}

fn axis(delta: i32, bytes: &mut alloc::vec::Vec<u8>, short: u8, same: u8) -> Option<u8> {
    if delta == 0 {
        return Some(same);
    }
    if let Ok(magnitude) = u8::try_from(delta.unsigned_abs()) {
        bytes.push(magnitude);
        return Some(short | if delta > 0 { same } else { 0 });
    }
    bytes.extend_from_slice(&i16::try_from(delta).ok()?.to_be_bytes());
    Some(0)
}

pub(super) fn restore(
    mut data: &[u8],
    loca_len: usize,
    limit: usize,
) -> Option<(alloc::vec::Vec<u8>, alloc::vec::Vec<u8>)> {
    use alloc::vec::Vec;
    let version = u16::from_be_bytes(take(&mut data)?);
    let options = u16::from_be_bytes(take(&mut data)?);
    let count = usize::from(u16::from_be_bytes(take(&mut data)?));
    let format = u16::from_be_bytes(take(&mut data)?);
    if version != 0
        || options > 1
        || format > 1
        || loca_len
            != count
                .checked_add(1)?
                .checked_mul(if format == 0 { 2 } else { 4 })?
    {
        return None;
    }
    let mut sizes = [0usize; 7];
    for size in &mut sizes {
        *size = usize::try_from(u32::from_be_bytes(take(&mut data)?)).ok()?;
    }
    let mut streams = [&[][..]; 7];
    for (stream, size) in streams.iter_mut().zip(sizes) {
        *stream = take_bytes(&mut data, size)?;
    }
    let [
        mut contours,
        mut points,
        mut flags,
        mut glyphs,
        mut composites,
        mut bounds,
        mut instructions,
    ] = streams;
    let bbox_bits = take_bytes(&mut bounds, count.div_ceil(32).checked_mul(4)?)?;
    let overlap = if options == 1 {
        take_bytes(&mut data, count.div_ceil(8))?
    } else {
        &[]
    };
    if !data.is_empty() {
        return None;
    }
    let mut glyf = Vec::new();
    let mut loca = Vec::with_capacity(loca_len);
    for index in 0..=count {
        if format == 0 {
            loca.extend_from_slice(&u16::try_from(glyf.len() / 2).ok()?.to_be_bytes());
        } else {
            loca.extend_from_slice(&u32::try_from(glyf.len()).ok()?.to_be_bytes());
        }
        if index == count {
            break;
        }
        let n = i16::from_be_bytes(take(&mut contours)?);
        let bit = 0x80 >> (index % 8);
        let bbox = if bbox_bits.get(index / 8)? & bit != 0 {
            Some(take::<8>(&mut bounds)?)
        } else {
            None
        };
        if n == 0 {
            if bbox.is_some() {
                return None;
            }
            continue;
        }
        let mut body = Vec::new();
        let mut extrema = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        if n == -1 {
            let mut has_instructions = false;
            loop {
                let component = u16::from_be_bytes(take(&mut composites)?);
                body.extend_from_slice(&component.to_be_bytes());
                let args: usize = if component & 1 != 0 { 4 } else { 2 };
                let matrix = match component & 0xc8 {
                    0 => 0,
                    8 => 2,
                    64 => 4,
                    128 => 8,
                    _ => return None,
                };
                body.extend_from_slice(take_bytes(
                    &mut composites,
                    args.checked_add(matrix)?.checked_add(2)?,
                )?);
                has_instructions |= component & 256 != 0;
                if has_instructions && component & 32 == 0 {
                    let size = uint255(&mut glyphs)?;
                    body.extend_from_slice(&size.to_be_bytes());
                    body.extend_from_slice(take_bytes(&mut instructions, usize::from(size))?);
                }
                if component & 32 == 0 {
                    break;
                }
            }
            bbox?;
        } else {
            let mut end = 0usize;
            for _ in 0..u16::try_from(n).ok()? {
                let points_in_contour = uint255(&mut points)?;
                if points_in_contour == 0 {
                    return None;
                }
                end = end.checked_add(usize::from(points_in_contour))?;
                body.extend_from_slice(&u16::try_from(end.checked_sub(1)?).ok()?.to_be_bytes());
            }
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            let mut encoded = Vec::new();
            let (mut x, mut y) = (0i32, 0i32);
            for (p, &flag) in take_bytes(&mut flags, end)?.iter().enumerate() {
                let (dx, dy) = triplet(flag, &mut glyphs)?;
                x = x.checked_add(dx)?;
                y = y.checked_add(dy)?;
                let px = i16::try_from(x).ok()?;
                let py = i16::try_from(y).ok()?;
                extrema = [
                    extrema[0].min(px),
                    extrema[1].min(py),
                    extrema[2].max(px),
                    extrema[3].max(py),
                ];
                let mut f = u8::from(flag & 128 == 0)
                    | axis(dx, &mut xs, 2, 16)?
                    | axis(dy, &mut ys, 4, 32)?;
                if p == 0 && overlap.get(index / 8).is_some_and(|v| v & bit != 0) {
                    f |= 64;
                }
                encoded.push(f);
            }
            let size = uint255(&mut glyphs)?;
            body.extend_from_slice(&size.to_be_bytes());
            body.extend_from_slice(take_bytes(&mut instructions, usize::from(size))?);
            for run in encoded.chunk_by(|left, right| left == right) {
                for chunk in run.chunks(256) {
                    let (&flag, repeats) = chunk.split_first()?;
                    if repeats.is_empty() {
                        body.push(flag);
                    } else {
                        body.extend([flag | 8, u8::try_from(repeats.len()).ok()?]);
                    }
                }
            }
            body.extend(xs);
            body.extend(ys);
        }
        let end = glyf
            .len()
            .checked_add(10)?
            .checked_add(body.len())?
            .checked_next_multiple_of(4)?;
        if end > limit {
            return None;
        }
        glyf.extend_from_slice(&n.to_be_bytes());
        if let Some(bytes) = bbox {
            glyf.extend(bytes);
        } else {
            for val in extrema {
                glyf.extend(val.to_be_bytes());
            }
        }
        glyf.extend(body);
        glyf.resize(end, 0);
    }
    [
        contours,
        points,
        flags,
        glyphs,
        composites,
        bounds,
        instructions,
    ]
    .iter()
    .all(|s| s.is_empty())
    .then_some((glyf, loca))
}

#[cfg(test)]
mod composite_tests {
    #[test]
    fn adr0065_i1_composite_instructions_on_first_component() {
        assert_eq!(verify(), Some(()));
    }
    fn verify() -> Option<()> {
        let components = [1, 34, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0];
        let streams: [&[u8]; 7] = [
            &[0, 0, 255, 255],
            &[],
            &[],
            &[1],
            &components,
            &[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            &[0x31],
        ];
        let mut stream = alloc::vec![0, 0, 0, 0, 0, 2, 0, 1];
        for part in streams {
            stream.extend(u32::try_from(part.len()).ok()?.to_be_bytes());
        }
        for part in streams {
            stream.extend(part);
        }
        let (glyf, loca) = super::restore(&stream, 12, 64)?;
        assert_eq!(glyf.get(10..22)?, components);
        assert_eq!(glyf.get(22..25)?, &[0, 1, 0x31]);
        assert_eq!(loca, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28]);
        Some(())
    }
}

#[cfg(test)]
mod simple_tests {
    #[test]
    fn adr0065_i1_repeated_flags_fit_original_table() -> Result<(), core::num::TryFromIntError> {
        let parts: [&[u8]; 7] = [&[0, 1], &[8], &[0; 8], &[0; 9], &[], &[0; 4], &[]];
        let mut input = alloc::vec![0, 0, 0, 0, 0, 1, 0, 0];
        for part in parts {
            input.extend(u32::try_from(part.len())?.to_be_bytes());
        }
        for part in parts {
            input.extend(part);
        }
        let restored = super::restore(&input, 4, 16);
        assert_eq!(
            restored.as_ref().and_then(|(glyf, _)| glyf.get(14..)),
            Some(&[0x39, 7][..])
        );
        Ok(())
    }
}
