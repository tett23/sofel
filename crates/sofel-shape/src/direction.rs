//! Text direction (`hb_direction_t`).

/// The direction of a run of text, as HarfBuzz's `hb_direction_t`
/// without its invalid value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    /// Left to right.
    Ltr,
    /// Right to left.
    Rtl,
    /// Top to bottom.
    Ttb,
    /// Bottom to top.
    Btt,
}

impl Direction {
    /// Left to right or right to left.
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Direction::Ltr | Direction::Rtl)
    }
    /// Top to bottom or bottom to top.
    pub const fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }
    /// Left to right or top to bottom.
    pub const fn is_forward(self) -> bool {
        matches!(self, Direction::Ltr | Direction::Ttb)
    }
    /// Right to left or bottom to top.
    pub const fn is_backward(self) -> bool {
        !self.is_forward()
    }
    /// The opposite direction (`HB_DIRECTION_REVERSE`).
    pub const fn reverse(self) -> Direction {
        match self {
            Direction::Ltr => Direction::Rtl,
            Direction::Rtl => Direction::Ltr,
            Direction::Ttb => Direction::Btt,
            Direction::Btt => Direction::Ttb,
        }
    }
    /// The direction of a string as `hb_direction_from_string` reads it:
    /// by its first letter, `l`, `r`, `t` or `b` in either case. `None`
    /// for anything else.
    pub fn from_bytes(s: &[u8]) -> Option<Direction> {
        match s.first().map(u8::to_ascii_lowercase) {
            Some(b'l') => Some(Direction::Ltr),
            Some(b'r') => Some(Direction::Rtl),
            Some(b't') => Some(Direction::Ttb),
            Some(b'b') => Some(Direction::Btt),
            _ => None,
        }
    }
    /// The name `hb_direction_to_string` gives: `ltr`, `rtl`, `ttb` or `btt`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::Ltr => "ltr",
            Direction::Rtl => "rtl",
            Direction::Ttb => "ttb",
            Direction::Btt => "btt",
        }
    }
}
