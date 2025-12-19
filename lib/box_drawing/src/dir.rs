//! Definitions for line directions.

use core::ops::{BitOr, BitOrAssign};

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
            bits |= dirs[i].as_bitmap();
            i += 1;
        }
        Self(bits)
    }

    /// Create a new `LineDirections` from a single direction
    #[inline]
    pub const fn from_dir(dir: LineDir) -> Self {
        Self(dir.as_bitmap())
    }

    /// Returns `true` if the specified direction is contained.
    #[inline]
    pub const fn contains(&self, dir: LineDir) -> bool {
        (self.0 & dir.as_bitmap()) != 0
    }

    /// Returns `true` if no directions are set.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Returns a new `LineDirections` with all directions reversed.
    pub const fn rev(&self) -> Self {
        let mut acc = 0;
        if self.contains(LineDir::SingleUp) {
            acc |= LineDir::SingleDown.as_bitmap();
        }
        if self.contains(LineDir::SingleDown) {
            acc |= LineDir::SingleUp.as_bitmap();
        }
        if self.contains(LineDir::SingleLeft) {
            acc |= LineDir::SingleRight.as_bitmap();
        }
        if self.contains(LineDir::SingleRight) {
            acc |= LineDir::SingleLeft.as_bitmap();
        }
        Self(acc)
    }

    /// Returns the directions rotated clockwise.
    pub const fn rotated_cw(&self) -> Self {
        Self(((self.0 & 0b0111) << 1) | ((self.0 & 0b1000) >> 3))
    }

    /// Returns the directions rotated counter-clockwise.
    pub const fn rotated_ccw(&self) -> Self {
        Self(((self.0 & 0b1110) >> 1) | ((self.0 & 0b0001) << 3))
    }
}

impl From<LineDir> for LineDirections {
    #[inline]
    fn from(dir: LineDir) -> Self {
        Self::from_dir(dir)
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
        self.bitor(Self::from_dir(rhs))
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
        self.bitor_assign(Self::from_dir(rhs));
    }
}

/// Represents a line direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineDir {
    SingleUp = 0,
    SingleRight = 1,
    SingleDown = 2,
    SingleLeft = 3,
    // DoubleUp = 4,
    // DoubleRight = 5,
    // DoubleDown = 6,
    // DoubleLeft = 7,
}

impl LineDir {
    #[inline]
    const fn as_bitmap(&self) -> u8 {
        1 << (*self as u8)
    }

    /// Returns the reversed direction.
    #[inline]
    pub const fn rev(&self) -> Self {
        match self {
            Self::SingleUp => Self::SingleDown,
            Self::SingleDown => Self::SingleUp,
            Self::SingleLeft => Self::SingleRight,
            Self::SingleRight => Self::SingleLeft,
            // Self::DoubleUp => Self::DoubleDown,
            // Self::DoubleDown => Self::DoubleUp,
            // Self::DoubleLeft => Self::DoubleRight,
            // Self::DoubleRight => Self::DoubleLeft,
        }
    }

    /// Returns the direction rotated clockwise.
    #[inline]
    pub const fn rotate_cw(&self) -> Self {
        match self {
            Self::SingleUp => Self::SingleRight,
            Self::SingleRight => Self::SingleDown,
            Self::SingleDown => Self::SingleLeft,
            Self::SingleLeft => Self::SingleUp,
            // Self::DoubleUp => Self::DoubleRight,
            // Self::DoubleRight => Self::DoubleDown,
            // Self::DoubleDown => Self::DoubleLeft,
            // Self::DoubleLeft => Self::DoubleUp,
        }
    }

    /// Returns the direction rotated counter-clockwise.
    #[inline]
    pub const fn rotate_ccw(&self) -> Self {
        match self {
            Self::SingleUp => Self::SingleLeft,
            Self::SingleLeft => Self::SingleDown,
            Self::SingleDown => Self::SingleRight,
            Self::SingleRight => Self::SingleUp,
            // Self::DoubleUp => Self::DoubleLeft,
            // Self::DoubleLeft => Self::DoubleDown,
            // Self::DoubleDown => Self::DoubleRight,
            // Self::DoubleRight => Self::DoubleUp,
        }
    }

    /// Returns all directions.
    #[inline]
    pub const fn all_dirs() -> &'static [Self; 4] {
        &[
            Self::SingleUp,
            Self::SingleDown,
            Self::SingleLeft,
            Self::SingleRight,
            // Self::DoubleUp,
            // Self::DoubleDown,
            // Self::DoubleLeft,
            // Self::DoubleRight,
        ]
    }
}
