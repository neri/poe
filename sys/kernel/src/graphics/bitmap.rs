// Bitmap

use super::color::*;
use super::coords::*;

pub trait BitmapTrait
where
    Self::PixelType: Sized + Copy + Clone,
{
    type PixelType;

    fn bits_per_pixel(&self) -> usize;
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn slice(&self) -> &[Self::PixelType];

    fn stride(&self) -> usize {
        self.width()
    }

    fn size(&self) -> Size {
        Size::new(self.width() as isize, self.height() as isize)
    }

    fn get_pixel(&self, point: Point) -> Option<Self::PixelType> {
        if point.is_within(Rect::from(self.size())) {
            Some(unsafe { self.get_pixel_unchecked(point) })
        } else {
            None
        }
    }

    /// SAFETY: The point must be within the size range.
    unsafe fn get_pixel_unchecked(&self, point: Point) -> Self::PixelType {
        *self
            .slice()
            .get_unchecked(point.x as usize + point.y as usize * self.stride())
    }
}

pub trait MutableBitmapTrait: BitmapTrait {
    fn slice_mut(&mut self) -> &mut [Self::PixelType];

    fn set_pixel(&mut self, point: Point, pixel: Self::PixelType) {
        if point.is_within(Rect::from(self.size())) {
            unsafe {
                self.set_pixel_unchecked(point, pixel);
            }
        }
    }

    /// SAFETY: The point must be within the size range.
    unsafe fn set_pixel_unchecked(&mut self, point: Point, pixel: Self::PixelType) {
        let stride = self.stride();
        *self
            .slice_mut()
            .get_unchecked_mut(point.x as usize + point.y as usize * stride) = pixel;
    }
}

pub trait RasterFontWriter: MutableBitmapTrait {
    fn draw_font(&mut self, src: &[u8], size: Size, origin: Point, color: Self::PixelType);
}

#[repr(C)]
pub struct OsBitmap8<'a> {
    width: usize,
    height: usize,
    stride: usize,
    slice: &'a [IndexedColor],
}

impl<'a> OsBitmap8<'a> {
    #[inline]
    pub const fn from_slice(slice: &'a [IndexedColor], size: Size, stride: usize) -> Self {
        Self {
            width: size.width() as usize,
            height: size.height() as usize,
            stride,
            slice,
        }
    }
}

impl OsBitmap8<'_> {
    //
}

impl BitmapTrait for OsBitmap8<'_> {
    type PixelType = IndexedColor;

    fn bits_per_pixel(&self) -> usize {
        8
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn stride(&self) -> usize {
        self.stride
    }

    fn slice(&self) -> &[Self::PixelType] {
        self.slice
    }
}

#[repr(C)]
pub struct OsMutBitmap8<'a> {
    width: usize,
    height: usize,
    stride: usize,
    slice: &'a mut [IndexedColor],
}

impl<'a> OsMutBitmap8<'a> {
    #[inline]
    pub fn from_slice(slice: &'a mut [IndexedColor], size: Size, stride: usize) -> Self {
        Self {
            width: size.width() as usize,
            height: size.height() as usize,
            stride,
            slice,
        }
    }
}

impl OsMutBitmap8<'static> {
    /// SAFETY: Must guarantee the existence of the `ptr`.
    #[inline]
    pub unsafe fn from_static(ptr: *mut IndexedColor, size: Size, stride: usize) -> Self {
        let slice = core::slice::from_raw_parts_mut(ptr, size.height() as usize * stride);
        Self {
            width: size.width() as usize,
            height: size.height() as usize,
            stride,
            slice,
        }
    }
}

impl OsMutBitmap8<'_> {
    #[inline]
    fn memset_colors(slice: &mut [IndexedColor], cursor: usize, size: usize, color: IndexedColor) {
        // let slice = &mut slice[cursor..cursor + size];
        unsafe {
            let slice = slice.get_unchecked_mut(cursor);
            let color = color.0;
            let mut ptr: *mut u8 = core::mem::transmute(slice);
            let mut remain = size;

            while (ptr as usize & 0x3) != 0 && remain > 0 {
                ptr.write_volatile(color);
                ptr = ptr.add(1);
                remain -= 1;
            }

            if remain > 4 {
                let color32 = color as u32
                    | (color as u32) << 8
                    | (color as u32) << 16
                    | (color as u32) << 24;
                let count = remain / 4;
                let mut ptr2 = ptr as *mut u32;

                for _ in 0..count {
                    ptr2.write_volatile(color32);
                    ptr2 = ptr2.add(1);
                }

                ptr = ptr2 as *mut u8;
                remain -= count * 4;
            }

            for _ in 0..remain {
                ptr.write_volatile(color);
                ptr = ptr.add(1);
            }
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, color: IndexedColor) {
        let mut width = rect.width();
        let mut height = rect.height();
        let mut dx = rect.x();
        let mut dy = rect.y();

        if dx < 0 {
            width += dx;
            dx = 0;
        }
        if dy < 0 {
            height += dy;
            dy = 0;
        }
        let r = dx + width;
        let b = dy + height;
        if r >= self.width as isize {
            width = self.width as isize - dx;
        }
        if b >= self.height as isize {
            height = self.height as isize - dy;
        }
        if width <= 0 || height <= 0 {
            return;
        }

        let width = width as usize;
        let height = height as usize;
        let stride = self.stride;
        let mut cursor = dx as usize + dy as usize * stride;
        if stride == width {
            Self::memset_colors(self.slice, cursor, width * height, color);
        } else {
            for _ in 0..height {
                Self::memset_colors(self.slice, cursor, width, color);
                cursor += stride;
            }
        }
    }

    pub fn draw_hline(&mut self, point: Point, width: isize, color: IndexedColor) {
        let mut dx = point.x;
        let dy = point.y;
        let mut w = width;

        if dy < 0 || dy >= (self.height as isize) {
            return;
        }
        if dx < 0 {
            w += dx;
            dx = 0;
        }
        let r = dx + w;
        if r >= (self.width as isize) {
            w = (self.width as isize) - dx;
        }
        if w <= 0 {
            return;
        }

        let cursor = dx as usize + dy as usize * self.stride;
        Self::memset_colors(self.slice, cursor, w as usize, color);
    }

    pub fn draw_vline(&mut self, point: Point, height: isize, color: IndexedColor) {
        let dx = point.x;
        let mut dy = point.y;
        let mut h = height;

        if dx < 0 || dx >= (self.width as isize) {
            return;
        }
        if dy < 0 {
            h += dy;
            dy = 0;
        }
        let b = dy + h;
        if b >= (self.width as isize) {
            h = (self.width as isize) - dy;
        }
        if h <= 0 {
            return;
        }

        let stride = self.stride;
        let mut cursor = dx as usize + dy as usize * stride;
        for _ in 0..h {
            self.slice[cursor] = color;
            cursor += stride;
        }
    }

    pub fn draw_rect(&mut self, rect: Rect, color: IndexedColor) {
        let coords = Coordinates::from_rect(rect).unwrap();
        let width = rect.width();
        let height = rect.height();
        self.draw_hline(coords.left_top(), width, color);
        self.draw_hline(coords.left_bottom() - Point::new(0, 1), width, color);
        if height > 2 {
            self.draw_vline(coords.left_top() + Point::new(0, 1), height - 2, color);
            self.draw_vline(coords.right_top() + Point::new(-1, 1), height - 2, color);
        }
    }

    pub fn draw_circle(&mut self, origin: Point, radius: isize, color: IndexedColor) {
        let rect = Rect {
            origin: origin - radius,
            size: Size::new(radius * 2, radius * 2),
        };
        self.draw_round_rect(rect, radius, color);
    }

    pub fn fill_circle(&mut self, origin: Point, radius: isize, color: IndexedColor) {
        let rect = Rect {
            origin: origin - radius,
            size: Size::new(radius * 2, radius * 2),
        };
        self.fill_round_rect(rect, radius, color);
    }

    pub fn fill_round_rect(&mut self, rect: Rect, radius: isize, color: IndexedColor) {
        let width = rect.size.width;
        let height = rect.size.height;
        let dx = rect.origin.x;
        let dy = rect.origin.y;

        let mut radius = radius;
        if radius * 2 > width {
            radius = width / 2;
        }
        if radius * 2 > height {
            radius = height / 2;
        }

        let lh = height - radius * 2;
        if lh > 0 {
            let rect_line = Rect::new(dx, dy + radius, width, lh);
            self.fill_rect(rect_line, color);
        }

        let mut cx = radius;
        let mut cy = 0;
        let mut f = -2 * radius + 3;
        let qh = height - 1;

        while cx >= cy {
            {
                let bx = radius - cy;
                let by = radius - cx;
                let dw = width - bx * 2;
                self.draw_hline(Point::new(dx + bx, dy + by), dw, color);
                self.draw_hline(Point::new(dx + bx, dy + qh - by), dw, color);
            }

            {
                let bx = radius - cx;
                let by = radius - cy;
                let dw = width - bx * 2;
                self.draw_hline(Point::new(dx + bx, dy + by), dw, color);
                self.draw_hline(Point::new(dx + bx, dy + qh - by), dw, color);
            }

            if f >= 0 {
                cx -= 1;
                f -= 4 * cx;
            }
            cy += 1;
            f += 4 * cy + 2;
        }
    }

    pub fn draw_round_rect(&mut self, rect: Rect, radius: isize, color: IndexedColor) {
        let width = rect.size.width;
        let height = rect.size.height;
        let dx = rect.origin.x;
        let dy = rect.origin.y;

        let mut radius = radius;
        if radius * 2 > width {
            radius = width / 2;
        }
        if radius * 2 > height {
            radius = height / 2;
        }

        let lh = height - radius * 2;
        if lh > 0 {
            self.draw_vline(Point::new(dx, dy + radius), lh, color);
            self.draw_vline(Point::new(dx + width - 1, dy + radius), lh, color);
        }
        let lw = width - radius * 2;
        if lw > 0 {
            self.draw_hline(Point::new(dx + radius, dy), lw, color);
            self.draw_hline(Point::new(dx + radius, dy + height - 1), lw, color);
        }

        let mut cx = radius;
        let mut cy = 0;
        let mut f = -2 * radius + 3;
        let qh = height - 1;

        while cx >= cy {
            {
                let bx = radius - cy;
                let by = radius - cx;
                let dw = width - bx * 2 - 1;
                self.set_pixel(Point::new(dx + bx, dy + by), color);
                self.set_pixel(Point::new(dx + bx, dy + qh - by), color);
                self.set_pixel(Point::new(dx + bx + dw, dy + by), color);
                self.set_pixel(Point::new(dx + bx + dw, dy + qh - by), color);
            }

            {
                let bx = radius - cx;
                let by = radius - cy;
                let dw = width - bx * 2 - 1;
                self.set_pixel(Point::new(dx + bx, dy + by), color);
                self.set_pixel(Point::new(dx + bx, dy + qh - by), color);
                self.set_pixel(Point::new(dx + bx + dw, dy + by), color);
                self.set_pixel(Point::new(dx + bx + dw, dy + qh - by), color);
            }

            if f >= 0 {
                cx -= 1;
                f -= 4 * cx;
            }
            cy += 1;
            f += 4 * cy + 2;
        }
    }
}

impl BitmapTrait for OsMutBitmap8<'_> {
    type PixelType = IndexedColor;

    fn bits_per_pixel(&self) -> usize {
        8
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn stride(&self) -> usize {
        self.stride
    }

    fn slice(&self) -> &[Self::PixelType] {
        self.slice
    }
}

impl MutableBitmapTrait for OsMutBitmap8<'_> {
    fn slice_mut(&mut self) -> &mut [Self::PixelType] {
        &mut self.slice
    }
}

impl RasterFontWriter for OsMutBitmap8<'_> {
    fn draw_font(&mut self, src: &[u8], size: Size, origin: Point, color: Self::PixelType) {
        let width = size.width as usize;
        let stride = (width + 7) / 8;
        let w8 = width / 8;
        let w7 = width & 7;
        let mut cursor = 0;
        for y in 0..size.height {
            for i in 0..w8 {
                let data = unsafe { src.get_unchecked(cursor + i) };
                for j in 0..8 {
                    let position = 0x80u8 >> j;
                    if (data & position) != 0 {
                        let x = (i * 8 + j) as isize;
                        let y = y;
                        let point = Point::new(origin.x + x, origin.y + y);
                        self.set_pixel(point, color);
                    }
                }
            }
            if w7 > 0 {
                let data = unsafe { src.get_unchecked(cursor + w8) };
                let base_x = w8 * 8;
                for i in 0..w7 {
                    let position = 0x80u8 >> i;
                    if (data & position) != 0 {
                        let x = (i + base_x) as isize;
                        let y = y;
                        let point = Point::new(origin.x + x, origin.y + y);
                        self.set_pixel(point, color);
                    }
                }
            }
            cursor += stride;
        }
    }
}

impl<'a> From<&'a OsMutBitmap8<'a>> for OsBitmap8<'a> {
    fn from(src: &'a OsMutBitmap8) -> Self {
        Self::from_slice(src.slice(), src.size(), src.stride())
    }
}
