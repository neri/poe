use core::ops::BitOr;
use core::ops::BitOrAssign;

/// Represents a set of line directions
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineDirections(pub(crate) u8);

impl LineDirections {
    /// Create a new `LineDirections` from individual directions
    pub const fn new(dirs: &[LineDir]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < dirs.len() {
            bits |= dirs[i] as u8;
            i += 1;
        }
        Self(bits)
    }

    #[inline]
    pub const fn contains(&self, dir: LineDir) -> bool {
        (self.0 & (dir as u8)) != 0
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub const fn rev(&self) -> Self {
        let mut acc = 0;
        if self.contains(LineDir::SingleUp) {
            acc |= LineDir::SingleDown as u8;
        }
        if self.contains(LineDir::SingleDown) {
            acc |= LineDir::SingleUp as u8;
        }
        if self.contains(LineDir::SingleLeft) {
            acc |= LineDir::SingleRight as u8;
        }
        if self.contains(LineDir::SingleRight) {
            acc |= LineDir::SingleLeft as u8;
        }
        Self(acc)
    }

    /// Rotate the directions clockwise
    pub const fn rotated_cw(&self) -> Self {
        let mut acc = (self.0 << 1) & 0b1110_1110;
        if self.contains(LineDir::SingleLeft) {
            acc |= LineDir::SingleUp as u8;
        }
        Self(acc)
    }

    /// Rotate the directions counter-clockwise
    pub const fn rotated_ccw(&self) -> Self {
        let mut acc = (self.0 >> 1) & 0b0111_0111;
        if self.contains(LineDir::SingleUp) {
            acc |= LineDir::SingleLeft as u8;
        }
        Self(acc)
    }
}

impl BitOr<Self> for LineDirections {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOr<LineDir> for LineDirections {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: LineDir) -> Self::Output {
        Self(self.0 | (rhs as u8))
    }
}

impl BitOrAssign<Self> for LineDirections {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitOrAssign<LineDir> for LineDirections {
    #[inline]
    fn bitor_assign(&mut self, rhs: LineDir) {
        self.0 |= rhs as u8;
    }
}

/// Represents a line direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineDir {
    SingleUp = 0b0000_0001,
    SingleRight = 0b0000_0010,
    SingleDown = 0b0000_0100,
    SingleLeft = 0b0000_1000,
}

impl LineDir {
    #[inline]
    pub const fn is_single(&self) -> bool {
        match self {
            LineDir::SingleUp
            | LineDir::SingleDown
            | LineDir::SingleLeft
            | LineDir::SingleRight => true,

            #[allow(unreachable_patterns)]
            _ => false,
        }
    }

    #[inline]
    pub const fn rev(&self) -> Self {
        match self {
            LineDir::SingleUp => LineDir::SingleDown,
            LineDir::SingleDown => LineDir::SingleUp,
            LineDir::SingleLeft => LineDir::SingleRight,
            LineDir::SingleRight => LineDir::SingleLeft,
            // LineDir::DoubleUp => LineDir::DoubleDown,
            // LineDir::DoubleDown => LineDir::DoubleUp,
            // LineDir::DoubleLeft => LineDir::DoubleRight,
            // LineDir::DoubleRight => LineDir::DoubleLeft,
        }
    }

    #[inline]
    pub const fn rotate_cw(&self) -> Self {
        match self {
            LineDir::SingleUp => LineDir::SingleRight,
            LineDir::SingleRight => LineDir::SingleDown,
            LineDir::SingleDown => LineDir::SingleLeft,
            LineDir::SingleLeft => LineDir::SingleUp,
            // LineDir::DoubleUp => LineDir::DoubleRight,
            // LineDir::DoubleRight => LineDir::DoubleDown,
            // LineDir::DoubleDown => LineDir::DoubleLeft,
            // LineDir::DoubleLeft => LineDir::DoubleUp,
        }
    }

    #[inline]
    pub const fn rotate_ccw(&self) -> Self {
        match self {
            LineDir::SingleUp => LineDir::SingleLeft,
            LineDir::SingleLeft => LineDir::SingleDown,
            LineDir::SingleDown => LineDir::SingleRight,
            LineDir::SingleRight => LineDir::SingleUp,
            // LineDir::DoubleUp => LineDir::DoubleLeft,
            // LineDir::DoubleLeft => LineDir::DoubleDown,
            // LineDir::DoubleDown => LineDir::DoubleRight,
            // LineDir::DoubleRight => LineDir::DoubleUp,
        }
    }

    #[inline]
    pub const fn all_dirs() -> &'static [Self; 4] {
        &[
            LineDir::SingleUp,
            LineDir::SingleDown,
            LineDir::SingleLeft,
            LineDir::SingleRight,
        ]
    }
}
