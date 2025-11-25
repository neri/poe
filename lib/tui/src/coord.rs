//! TUI coordinate types.
use core::ops::{Add, AddAssign, Sub, SubAssign};

type ScalarType = i32;

/// A Point type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: ScalarType,
    pub y: ScalarType,
}

/// A Size type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub width: ScalarType,
    pub height: ScalarType,
}

/// A Rectangle type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub top_left: Point,
    pub size: Size,
}

/// Insets for a rectangle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Inset {
    pub left: ScalarType,
    pub top: ScalarType,
    pub right: ScalarType,
    pub bottom: ScalarType,
}

/// A Diagonal defined by two points
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Diagonal {
    pub top_left: Point,
    pub bottom_right: Point,
}

impl Point {
    #[inline]
    pub const fn new(x: ScalarType, y: ScalarType) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self { x: 0, y: 0 }
    }

    /// Draw a line from this point to another point using Bresenham's line algorithm.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the line was drawn successfully, or `Err(())` if an overflow occurred.
    #[inline]
    pub fn line_to<F>(&self, other: Self, mut plot: F) -> Result<(), ()>
    where
        F: FnMut(Point),
    {
        let dx: i32 = other.x.abs_diff(self.x).try_into().map_err(|_| ())?;
        let dy: i32 = -(other.y.abs_diff(self.y).try_into().map_err(|_| ())?);
        let sx = if self.x < other.x { 1 } else { -1 };
        let sy = if self.y < other.y { 1 } else { -1 };
        let mut err = dx + dy;
        let mut current = *self;

        loop {
            plot(current);

            if current == other {
                break;
            }

            let err2 = 2 * err;
            if err2 >= dy {
                err += dy;
                current.x += sx;
            }
            if err2 <= dx {
                err += dx;
                current.y += sy;
            }
        }
        Ok(())
    }
}

impl Size {
    #[inline]
    pub const fn new(width: ScalarType, height: ScalarType) -> Self {
        Self { width, height }
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            width: 0,
            height: 0,
        }
    }
}

impl Rect {
    #[inline]
    pub const fn new(top_left: Point, size: Size) -> Self {
        Self { top_left, size }
    }

    /// Creates a rectangle from the given top-left and bottom-right points.
    #[inline]
    pub const fn with_corners(top_left: Point, bottom_right: Point) -> Self {
        let width = bottom_right.x - top_left.x;
        let height = bottom_right.y - top_left.y;
        Self::new(top_left, Size::new(width, height))
    }

    #[inline]
    pub const fn zero() -> Self {
        Self {
            top_left: Point::zero(),
            size: Size::zero(),
        }
    }

    /// Returns a rectangle positioned at the origin with the same size as this rectangle.
    #[inline]
    pub const fn rect_at_origin(&self) -> Self {
        Self::new(Point::new(0, 0), self.size)
    }

    /// Returns the center point of the rectangle.
    #[inline]
    pub const fn center(&self) -> Point {
        Point::new(
            self.top_left.x + self.size.width / 2,
            self.top_left.y + self.size.height / 2,
        )
    }

    /// Checks if the rectangle contains the given point.
    #[inline]
    pub const fn contains(&self, point: &Point) -> bool {
        point.x >= self.top_left.x
            && point.x < self.top_left.x + self.size.width
            && point.y >= self.top_left.y
            && point.y < self.top_left.y + self.size.height
    }

    /// Returns the intersection of this rectangle with another rectangle.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x1 = self.top_left.x.max(other.top_left.x);
        let y1 = self.top_left.y.max(other.top_left.y);
        let x2 = (self.top_left.x + self.size.width).min(other.top_left.x + other.size.width);
        let y2 = (self.top_left.y + self.size.height).min(other.top_left.y + other.size.height);

        if x1 < x2 && y1 < y2 {
            Some(Self::with_corners(Point::new(x1, y1), Point::new(x2, y2)))
        } else {
            None
        }
    }

    /// Returns the top-left point of the rectangle.
    #[inline]
    pub const fn top_left(&self) -> Point {
        self.top_left
    }

    /// Returns the size of the rectangle.
    #[inline]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Returns the bottom-right point of the rectangle.
    #[inline]
    pub const fn bottom_right(&self) -> Option<Point> {
        if self.size.width > 0 && self.size.height > 0 {
            Some(Point {
                x: self.top_left.x + self.size.width - 1,
                y: self.top_left.y + self.size.height - 1,
            })
        } else {
            None
        }
    }

    /// Converts this rectangle to a diagonal.
    #[inline]
    pub fn to_diagonal(&self) -> Option<Diagonal> {
        let bottom_right = self.bottom_right()?;
        Some(Diagonal {
            top_left: self.top_left,
            bottom_right,
        })
    }

    /// Clips this rectangle to fit within the given rectangle.
    ///
    /// # Returns
    ///
    /// Returns `Some(())` if the clipping was successful,
    /// or `None` if the rectangle could not be clipped within the given rectangle.
    pub fn clip(&mut self, other: &Rect) -> Option<()> {
        let mut self_diag = self.to_diagonal()?;
        self_diag.clip(&other)?;
        *self = self_diag.to_rect()?;
        Some(())
    }

    /// Translates the rectangle by the given displacement.
    #[inline]
    pub fn translate(&mut self, displacement: Point) {
        self.top_left += displacement;
    }

    /// Returns a new rectangle inset by the given insets.
    pub fn insets(&self, inset: &Inset) -> Option<Rect> {
        let x = self.top_left.x.checked_add(inset.left)?;
        let y = self.top_left.y.checked_add(inset.top)?;
        let width = self
            .size
            .width
            .checked_sub(inset.left.checked_add(inset.right)?)?;
        let height = self
            .size
            .height
            .checked_sub(inset.top.checked_add(inset.bottom)?)?;

        // Ensure that the new origin plus size does not overflow
        let _ = x.checked_add(width)?;
        let _ = y.checked_add(height)?;

        Rect {
            top_left: Point { x, y },
            size: Size { width, height },
        }
        .into()
    }
}

impl Inset {
    #[inline]
    pub const fn new(
        left: ScalarType,
        top: ScalarType,
        right: ScalarType,
        bottom: ScalarType,
    ) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

impl Add<Self> for Point {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x.add(other.x),
            y: self.y.add(other.y),
        }
    }
}

impl AddAssign<Self> for Point {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.x += other.x;
        self.y += other.y;
    }
}

impl Add<Self> for Size {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Self {
            width: self.width.add(other.width),
            height: self.height.add(other.height),
        }
    }
}

impl AddAssign<Self> for Size {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.width += other.width;
        self.height += other.height;
    }
}

impl Sub<Self> for Size {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self {
            width: self.width.sub(other.width),
            height: self.height.sub(other.height),
        }
    }
}

impl SubAssign<Self> for Size {
    #[inline]
    fn sub_assign(&mut self, other: Self) {
        self.width -= other.width;
        self.height -= other.height;
    }
}

impl Add<Size> for Point {
    type Output = Self;

    #[inline]
    fn add(self, size: Size) -> Self {
        Self {
            x: self.x.add(size.width),
            y: self.y.add(size.height),
        }
    }
}

impl Add<Point> for Rect {
    type Output = Self;

    #[inline]
    fn add(self, point: Point) -> Self {
        Self {
            top_left: self.top_left + point,
            size: self.size,
        }
    }
}

impl AddAssign<Point> for Rect {
    #[inline]
    fn add_assign(&mut self, point: Point) {
        self.top_left += point;
    }
}

impl Add<Size> for Rect {
    type Output = Self;

    #[inline]
    fn add(self, size: Size) -> Self {
        Self {
            top_left: self.top_left,
            size: self.size + size,
        }
    }
}

impl AddAssign<Size> for Rect {
    #[inline]
    fn add_assign(&mut self, size: Size) {
        self.size += size;
    }
}

impl Add<Inset> for Rect {
    type Output = Self;

    #[inline]
    fn add(self, inset: Inset) -> Self {
        Self {
            top_left: Point {
                x: self.top_left.x.sub(inset.left),
                y: self.top_left.y.sub(inset.top),
            },
            size: Size {
                width: self.size.width.add(inset.left).add(inset.right),
                height: self.size.height.add(inset.top).add(inset.bottom),
            },
        }
    }
}

impl AddAssign<Inset> for Rect {
    #[inline]
    fn add_assign(&mut self, inset: Inset) {
        self.top_left.x -= inset.left;
        self.top_left.y -= inset.top;
        self.size.width += inset.left;
        self.size.width += inset.right;
        self.size.height += inset.top;
        self.size.height += inset.bottom;
    }
}

impl Diagonal {
    /// A diagonal that is always invalid.
    pub const INVALID: Self = Self {
        top_left: Point {
            x: ScalarType::MAX,
            y: ScalarType::MAX,
        },
        bottom_right: Point {
            x: ScalarType::MIN,
            y: ScalarType::MIN,
        },
    };

    /// A diagonal that covers all possible points.
    pub const ALL: Self = Self {
        top_left: Point {
            x: ScalarType::MIN,
            y: ScalarType::MIN,
        },
        bottom_right: Point {
            x: ScalarType::MAX,
            y: ScalarType::MAX,
        },
    };

    #[inline]
    pub const fn new(top_left: Point, bottom_right: Point) -> Self {
        Self {
            top_left,
            bottom_right,
        }
    }

    /// Checks if the diagonal is valid.
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.top_left.x <= self.bottom_right.x && self.top_left.y <= self.bottom_right.y
    }

    /// Converts this diagonal to a rectangle.
    ///
    /// # Returns
    ///
    /// Returns `Some(Rect)` if the diagonal is valid, or `None` otherwise.
    pub fn to_rect(&self) -> Option<Rect> {
        if self.is_valid() {
            let width = self
                .bottom_right
                .x
                .checked_sub(self.top_left.x)?
                .checked_add(1)?;
            let height = self
                .bottom_right
                .y
                .checked_sub(self.top_left.y)?
                .checked_add(1)?;
            Some(Rect {
                top_left: self.top_left,
                size: Size { width, height },
            })
        } else {
            None
        }
    }

    /// Expands this diagonal to include the given point.
    pub fn expand_point(&mut self, point: Point) {
        if point.x < self.top_left.x {
            self.top_left.x = point.x;
        }
        if point.y < self.top_left.y {
            self.top_left.y = point.y;
        }
        if point.x > self.bottom_right.x {
            self.bottom_right.x = point.x;
        }
        if point.y > self.bottom_right.y {
            self.bottom_right.y = point.y;
        }
    }

    /// Expands this diagonal to include the given diagonal.
    #[inline]
    pub fn expand_diagonal(&mut self, diag: &Diagonal) {
        self.expand_point(diag.top_left);
        self.expand_point(diag.bottom_right);
    }

    /// Expands this diagonal to include the given rectangle.
    #[inline]
    pub fn expand_rect(&mut self, rect: &Rect) {
        if let Some(diag) = rect.to_diagonal() {
            self.expand_diagonal(&diag);
        }
    }

    /// Clips this diagonal to fit within the given rectangle.
    ///
    /// # Returns
    ///
    /// Returns `Some(())` if the clipping was successful, or `None` if
    pub fn clip(&mut self, region: &Rect) -> Option<()> {
        if let Some(diag) = region.to_diagonal() {
            if self.top_left.x < diag.top_left.x {
                self.top_left.x = diag.top_left.x;
            }
            if self.top_left.y < diag.top_left.y {
                self.top_left.y = diag.top_left.y;
            }
            if self.bottom_right.x > diag.bottom_right.x {
                self.bottom_right.x = diag.bottom_right.x;
            }
            if self.bottom_right.y > diag.bottom_right.y {
                self.bottom_right.y = diag.bottom_right.y;
            }
            Some(())
        } else {
            None
        }
    }
}
