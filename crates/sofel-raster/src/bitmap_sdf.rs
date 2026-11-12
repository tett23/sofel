#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0064_i2_bitmap_sdf_preserves_subpixel_edge_and_propagates_distance() {
        let mut map = [Distance::FAR; 49];
        map[24] = Distance::new(ONE / 4, 0);
        transform(&mut map, 7, 7);
        assert_eq!(map[24].length, ONE / 4);
        assert_eq!(map[23].length, ONE + ONE / 4);
        assert_eq!(map[25].length, ONE - ONE / 4);
        assert_eq!(
            map[8].length,
            sofel_geom::vector_length(2 * ONE + ONE / 4, 2 * ONE)
        );
        assert_eq!(
            map[40].length,
            sofel_geom::vector_length(-2 * ONE + ONE / 4, -2 * ONE)
        );
    }

    #[test]
    fn adr0064_i2_bitmap_sdf_encodes_the_sign_and_spread_limits() {
        assert_eq!(encode(400 * ONE, 0, 8), 0);
        assert_eq!(encode(400 * ONE, 255, 8), 255);
        assert_eq!(encode(0, 0, 8), 128);
        assert_eq!(encode(ONE / 2, 126, 8), 120);
        assert_eq!(encode(ONE / 2, 127, 8), 136);
    }
}

const ONE: i32 = 1 << 16;

#[derive(Clone, Copy)]
struct Distance {
    x: i32,
    y: i32,
    length: i32,
}

impl Distance {
    const FAR: Self = Self {
        x: 200 * ONE,
        y: 200 * ONE,
        length: 400 * ONE,
    };

    fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            length: sofel_geom::vector_length(x, y),
        }
    }

    fn compare(&mut self, neighbor: Self, dx: i32, dy: i32) {
        if self.length <= ONE / 2 || neighbor.length - ONE >= self.length {
            return;
        }
        let candidate = Self::new(neighbor.x + dx * ONE, neighbor.y + dy * ONE);
        if candidate.length < self.length {
            *self = candidate;
        }
    }
}

// FreeType first_pass and second_pass visit the same row neighborhood
// in the same order, looking above on the first pass and below on the second.
fn scan_row(map: &mut [Distance], width: usize, row: usize, neighbor_row: usize, dy: i32) {
    for x in 1..width - 1 {
        let index = row * width + x;
        let above = neighbor_row * width + x;
        for (other, dx, dy) in [
            (above - 1, -1, dy),
            (above, 0, dy),
            (above + 1, 1, dy),
            (index - 1, -1, 0),
        ] {
            let neighbor = map[other];
            map[index].compare(neighbor, dx, dy);
        }
    }
    for x in (0..width - 1).rev() {
        let index = row * width + x;
        let right = map[index + 1];
        map[index].compare(right, 1, 0);
    }
}

fn transform(map: &mut [Distance], width: usize, rows: usize) {
    if width == 0 || rows == 0 {
        return;
    }
    for row in 1..rows {
        scan_row(map, width, row, row - 1, -1);
    }
    for row in (0..rows - 1).rev() {
        scan_row(map, width, row, row + 1, 1);
    }
}

fn encode(distance: i32, alpha: u8, spread: u8) -> u8 {
    let limit = i32::from(spread) * ONE;
    let magnitude = sofel_geom::div_fix(distance.clamp(0, limit), limit) >> 9;
    if alpha < 127 {
        (128 - magnitude) as u8
    } else {
        (128 + magnitude.min(127)) as u8
    }
}
