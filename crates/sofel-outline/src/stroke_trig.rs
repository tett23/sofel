//! Fixed-point angle and polar operations of FreeType 2.15.0's fttrigon.c.
//! The stroker uses degree angles in signed 16.16 units.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
use sofel_geom::Point;

pub(super) const PI: i32 = 180 << 16;
pub(super) const PI2: i32 = PI / 2;
pub(super) const PI4: i32 = PI / 4;
const SCALE: i64 = 0xdbd9_5b16;
const ARCTAN: [i32; 22] = [
    1_740_967, 919_879, 466_945, 234_379, 117_304, 58_666, 29_335, 14_668, 7_334, 3_667, 1_833,
    917, 458, 229, 115, 57, 29, 14, 7, 4, 2, 1,
];

fn downscale(value: i64) -> i64 {
    let scaled = (value.unsigned_abs() * SCALE as u64 + 0x4000_0000) >> 32;
    if value < 0 {
        -(scaled as i64)
    } else {
        scaled as i64
    }
}

fn prenorm(x: i64, y: i64) -> (i64, i64, i32) {
    let magnitude = (x.unsigned_abs() | y.unsigned_abs()) as u32;
    let highest = 31 - magnitude.leading_zeros();
    if highest <= 29 {
        let shift = 29 - highest;
        (x << shift, y << shift, shift as i32)
    } else {
        let shift = highest - 29;
        (x >> shift, y >> shift, -(shift as i32))
    }
}

fn pseudo_rotate(mut x: i64, mut y: i64, mut angle: i32) -> (i64, i64) {
    while angle < -PI4 {
        (x, y) = (y, -x);
        angle += PI2;
    }
    while angle > PI4 {
        (x, y) = (-y, x);
        angle -= PI2;
    }
    let mut b = 1i64;
    for (index, step) in ARCTAN.iter().enumerate() {
        let shift = index + 1;
        if angle < 0 {
            (x, y) = (x + ((y + b) >> shift), y - ((x + b) >> shift));
            angle += step;
        } else {
            (x, y) = (x - ((y + b) >> shift), y + ((x + b) >> shift));
            angle -= step;
        }
        b <<= 1;
    }
    (x, y)
}

fn pseudo_polarize(mut x: i64, mut y: i64) -> i32 {
    let mut angle = if y > x {
        if y > -x {
            (x, y) = (y, -x);
            PI2
        } else {
            let angle = if y > 0 { PI } else { -PI };
            (x, y) = (-x, -y);
            angle
        }
    } else if y < -x {
        (x, y) = (-y, x);
        -PI2
    } else {
        0
    };
    let mut b = 1i64;
    for (index, step) in ARCTAN.iter().enumerate() {
        let shift = index + 1;
        if y > 0 {
            (x, y) = (x + ((y + b) >> shift), y - ((x + b) >> shift));
            angle += step;
        } else {
            (x, y) = (x - ((y + b) >> shift), y + ((x + b) >> shift));
            angle -= step;
        }
        b <<= 1;
    }
    if angle >= 0 {
        (angle + 8) & !15
    } else {
        -((-angle + 8) & !15)
    }
}

pub(super) fn atan2(x: i32, y: i32) -> i32 {
    if x == 0 && y == 0 {
        return 0;
    }
    let (x, y, _) = prenorm(i64::from(x), i64::from(y));
    pseudo_polarize(x, y)
}

pub(super) fn angle_diff(a: i32, b: i32) -> i32 {
    let mut d = b - a;
    while d <= -PI {
        d += PI * 2;
    }
    while d > PI {
        d -= PI * 2;
    }
    d
}

pub(super) fn from_polar(length: i32, angle: i32) -> Point {
    if length == 0 {
        return Point::default();
    }
    let (mut x, mut y, shift) = prenorm(i64::from(length), 0);
    (x, y) = pseudo_rotate(x, y, angle);
    x = downscale(x);
    y = downscale(y);
    let (x, y) = if shift > 0 {
        let half = 1i64 << (shift - 1);
        (
            (x + half - i64::from(x < 0)) >> shift,
            (y + half - i64::from(y < 0)) >> shift,
        )
    } else {
        (x << -shift, y << -shift)
    };
    Point {
        x: x as i32,
        y: y as i32,
    }
}

pub(super) fn unit(angle: i32) -> Point {
    let (x, y) = pseudo_rotate(SCALE >> 8, 0, angle);
    Point {
        x: ((x + 128) >> 8) as i32,
        y: ((y + 128) >> 8) as i32,
    }
}

pub(super) fn tan(angle: i32) -> i32 {
    let (x, y) = pseudo_rotate(1 << 24, 0, angle);
    sofel_geom::div_fix(y as i32, x as i32)
}
