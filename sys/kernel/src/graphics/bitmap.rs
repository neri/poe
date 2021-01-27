// Bitmap Drawing

use super::color::*;
use super::coords::*;
use core::cell::UnsafeCell;
use core::convert::TryFrom;
use core::mem::transmute;

pub trait ImageTrait
where
    Self::PixelType: Sized + Copy + Clone,
{
    type PixelType;

    fn width(&self) -> usize;
    fn height(&self) -> usize;

    fn size(&self) -> Size {
        Size::new(self.width() as isize, self.height() as isize)
    }

    fn bounds(&self) -> Rect {
        Rect::from(self.size())
    }
}

pub trait RasterImage: ImageTrait {
    fn stride(&self) -> usize {
        self.width()
    }
    fn slice(&self) -> &[Self::PixelType];

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

pub trait MutableRasterImage: RasterImage {
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

pub trait BasicDrawing: MutableRasterImage {
    fn fill_rect(&mut self, rect: Rect, color: Self::PixelType);
    fn draw_hline(&mut self, point: Point, width: isize, color: Self::PixelType);
    fn draw_vline(&mut self, point: Point, height: isize, color: Self::PixelType);

    fn draw_rect(&mut self, rect: Rect, color: Self::PixelType) {
        let coords = match Coordinates::from_rect(rect) {
            Ok(v) => v,
            Err(_) => return,
        };
        let width = rect.width();
        let height = rect.height();
        self.draw_hline(coords.left_top(), width, color);
        self.draw_hline(coords.left_bottom() - Point::new(0, 1), width, color);
        if height > 2 {
            self.draw_vline(coords.left_top() + Point::new(0, 1), height - 2, color);
            self.draw_vline(coords.right_top() + Point::new(-1, 1), height - 2, color);
        }
    }

    fn draw_circle(&mut self, origin: Point, radius: isize, color: Self::PixelType) {
        let rect = Rect {
            origin: origin - radius,
            size: Size::new(radius * 2, radius * 2),
        };
        self.draw_round_rect(rect, radius, color);
    }

    fn fill_circle(&mut self, origin: Point, radius: isize, color: Self::PixelType) {
        let rect = Rect {
            origin: origin - radius,
            size: Size::new(radius * 2, radius * 2),
        };
        self.fill_round_rect(rect, radius, color);
    }

    fn fill_round_rect(&mut self, rect: Rect, radius: isize, color: Self::PixelType) {
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

    fn draw_round_rect(&mut self, rect: Rect, radius: isize, color: Self::PixelType) {
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

    fn draw_line(&mut self, c1: Point, c2: Point, color: Self::PixelType) {
        if c1.x() == c2.x() {
            if c1.y() < c2.y() {
                let height = c2.y() - c1.y();
                self.draw_vline(c1, height, color);
            } else {
                let height = c1.y() - c2.y();
                self.draw_vline(c2, height, color);
            }
        } else if c1.y() == c2.y() {
            if c1.x() < c2.x() {
                let width = c2.x() - c1.x();
                self.draw_hline(c1, width, color);
            } else {
                let width = c1.x() - c2.x();
                self.draw_hline(c2, width, color);
            }
        } else {
            c1.line_to(c2, |point| {
                self.set_pixel(point, color);
            });
        }
    }

    fn blt<T>(&mut self, src: &T, origin: Point, rect: Rect)
    where
        T: RasterImage<PixelType = <Self as ImageTrait>::PixelType>;
}

pub trait RasterFontWriter: MutableRasterImage {
    fn draw_font(&mut self, src: &[u8], size: Size, origin: Point, color: Self::PixelType) {
        let stride = (size.width as usize + 7) / 8;

        let mut coords = match Coordinates::from_rect(Rect { origin, size }) {
            Ok(v) => v,
            Err(_) => return,
        };

        let width = self.width() as isize;
        let height = self.height() as isize;
        if coords.right > width {
            coords.right = width;
        }
        if coords.bottom > height {
            coords.bottom = height;
        }
        if coords.left < 0 || coords.left >= width || coords.top < 0 || coords.top >= height {
            return;
        }

        let new_rect = Rect::from(coords);
        let width = new_rect.width() as usize;
        let height = new_rect.height();
        let w8 = width / 8;
        let w7 = width & 7;
        let mut cursor = 0;
        for y in 0..height {
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

    #[inline]
    pub const fn from_bytes(bytes: &'a [u8], size: Size) -> Self {
        Self {
            width: size.width() as usize,
            height: size.height() as usize,
            stride: size.width() as usize,
            slice: unsafe { transmute(bytes) },
        }
    }
}

impl ImageTrait for OsBitmap8<'_> {
    type PixelType = IndexedColor;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }
}

impl RasterImage for OsBitmap8<'_> {
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
    slice: UnsafeCell<&'a mut [IndexedColor]>,
}

impl<'a> OsMutBitmap8<'a> {
    #[inline]
    pub fn from_slice(slice: &'a mut [IndexedColor], size: Size, stride: usize) -> Self {
        Self {
            width: size.width() as usize,
            height: size.height() as usize,
            stride,
            slice: UnsafeCell::new(slice),
        }
    }

    /// Clone a bitmap
    #[inline]
    pub fn clone(&self) -> OsMutBitmap8<'a> {
        let slice = unsafe { self.slice.get().as_mut().unwrap() };
        Self {
            width: self.width(),
            height: self.height(),
            stride: self.stride(),
            slice: UnsafeCell::new(slice),
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
            slice: UnsafeCell::new(slice),
        }
    }
}

impl OsMutBitmap8<'_> {
    /// Fast fill
    #[inline]
    fn memset_colors(slice: &mut [IndexedColor], cursor: usize, size: usize, color: IndexedColor) {
        // let slice = &mut slice[cursor..cursor + size];
        unsafe {
            let slice = slice.get_unchecked_mut(cursor);
            let color = color.0;
            let mut ptr: *mut u8 = transmute(slice);
            let mut remain = size;

            // prologue
            let prologue = usize::min(ptr as usize & 0x0F, remain);
            remain -= prologue;
            for _ in 0..prologue {
                ptr.write_volatile(color);
                ptr = ptr.add(1);
            }

            // kernel
            if remain > 16 {
                let color32 = color as u32
                    | (color as u32) << 8
                    | (color as u32) << 16
                    | (color as u32) << 24;
                let color64 = color32 as u64 | (color32 as u64) << 32;
                let color128 = color64 as u128 | (color64 as u128) << 64;
                let count = remain / 16;
                let mut ptr2 = ptr as *mut u128;

                for _ in 0..count {
                    ptr2.write_volatile(color128);
                    ptr2 = ptr2.add(1);
                }

                ptr = ptr2 as *mut u8;
                remain -= count * 16;
            }

            // epilogue
            for _ in 0..remain {
                ptr.write_volatile(color);
                ptr = ptr.add(1);
            }
        }
    }

    /// Fast copy
    #[allow(dead_code)]
    #[inline]
    fn memcpy_colors(
        dest: &mut [IndexedColor],
        dest_cursor: usize,
        src: &[IndexedColor],
        src_cursor: usize,
        size: usize,
    ) {
        // let dest = &mut dest[dest_cursor..dest_cursor + size];
        // let src = &src[src_cursor..src_cursor + size];
        unsafe {
            let dest = dest.get_unchecked_mut(dest_cursor);
            let src = src.get_unchecked(src_cursor);
            let mut ptr_d: *mut u8 = transmute(dest);
            let mut ptr_s: *const u8 = transmute(src);
            let mut remain = size;
            if ((ptr_d as usize) & 0x3) == ((ptr_s as usize) & 0x3) {
                while (ptr_d as usize & 0x3) != 0 && remain > 0 {
                    ptr_d.write_volatile(ptr_s.read_volatile());
                    ptr_d = ptr_d.add(1);
                    ptr_s = ptr_s.add(1);
                    remain -= 1;
                }

                if remain > 4 {
                    let count = remain / 4;
                    let mut ptr2d = ptr_d as *mut u32;
                    let mut ptr2s = ptr_s as *const u32;

                    for _ in 0..count {
                        ptr2d.write_volatile(ptr2s.read_volatile());
                        ptr2d = ptr2d.add(1);
                        ptr2s = ptr2s.add(1);
                    }

                    ptr_d = ptr2d as *mut u8;
                    ptr_s = ptr2s as *const u8;
                    remain -= count * 4;
                }

                for _ in 0..remain {
                    ptr_d.write_volatile(ptr_s.read_volatile());
                    ptr_d = ptr_d.add(1);
                    ptr_s = ptr_s.add(1);
                }
            } else {
                for _ in 0..size {
                    ptr_d.write_volatile(ptr_s.read_volatile());
                    ptr_d = ptr_d.add(1);
                    ptr_s = ptr_s.add(1);
                }
            }
        }
    }

    pub fn blt_with_key<T>(
        &mut self,
        src: &T,
        origin: Point,
        rect: Rect,
        color_key: <Self as ImageTrait>::PixelType,
    ) where
        T: RasterImage<PixelType = <Self as ImageTrait>::PixelType>,
    {
        self.blt_main(src, origin, rect, Some(color_key));
    }

    #[inline]
    fn blt_main<T>(
        &mut self,
        src: &T,
        origin: Point,
        rect: Rect,
        color_key: Option<<Self as ImageTrait>::PixelType>,
    ) where
        T: RasterImage<PixelType = <Self as ImageTrait>::PixelType>,
    {
        let mut dx = origin.x;
        let mut dy = origin.y;
        let mut sx = rect.origin.x;
        let mut sy = rect.origin.y;
        let mut width = rect.width();
        let mut height = rect.height();

        if dx < 0 {
            sx -= dx;
            width += dx;
            dx = 0;
        }
        if dy < 0 {
            sy -= dy;
            height += dy;
            dy = 0;
        }
        let sw = src.width() as isize;
        let sh = src.height() as isize;
        if width > sx + sw {
            width = sw - sx;
        }
        if height > sy + sh {
            height = sh - sy;
        }
        let r = dx + width;
        let b = dy + height;
        let dw = self.width() as isize;
        let dh = self.height() as isize;
        if r >= dw {
            width = dw - dx;
        }
        if b >= dh {
            height = dh - dy;
        }
        if width <= 0 || height <= 0 {
            return;
        }

        let width = width as usize;
        let height = height as usize;

        let ds = self.stride();
        let ss = src.stride();
        let mut dest_cursor = dx as usize + dy as usize * ds;
        let mut src_cursor = sx as usize + sy as usize * ss;
        let dest_fb = self.slice_mut();
        let src_fb = src.slice();

        if let Some(color_key) = color_key {
            for _ in 0..height {
                for i in 0..width {
                    let c = src_fb[src_cursor + i];
                    if c != color_key {
                        dest_fb[dest_cursor + i] = c;
                    }
                }
                dest_cursor += ds;
                src_cursor += ss;
            }
        } else {
            if ds == width && ss == width {
                Self::memcpy_colors(dest_fb, dest_cursor, src_fb, src_cursor, width * height);
            } else {
                for _ in 0..height {
                    Self::memcpy_colors(dest_fb, dest_cursor, src_fb, src_cursor, width);
                    dest_cursor += ds;
                    src_cursor += ss;
                }
            }
        }
    }

    /// Make a bitmap view
    pub fn view<'a, F, R>(&'a mut self, rect: Rect, f: F) -> Option<R>
    where
        F: FnOnce(&mut OsMutBitmap8) -> R,
    {
        let coords = match Coordinates::try_from(rect) {
            Ok(v) => v,
            Err(_) => return None,
        };
        let width = self.width as isize;
        let height = self.height as isize;
        let stride = self.stride;

        if coords.left < 0
            || coords.left >= width
            || coords.right > width
            || coords.top < 0
            || coords.top >= height
            || coords.bottom > height
        {
            return None;
        }

        let offset = rect.x() as usize + rect.y() as usize * stride;
        let new_len = rect.height() as usize * stride;
        let r = {
            let slice = self.slice.get_mut();
            let mut view = OsMutBitmap8 {
                width: rect.width() as usize,
                height: rect.height() as usize,
                stride,
                slice: UnsafeCell::new(&mut slice[offset..offset + new_len]),
            };
            f(&mut view)
        };
        Some(r)
    }
}

impl ImageTrait for OsMutBitmap8<'_> {
    type PixelType = IndexedColor;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }
}

impl RasterImage for OsMutBitmap8<'_> {
    fn stride(&self) -> usize {
        self.stride
    }

    fn slice(&self) -> &[Self::PixelType] {
        unsafe { self.slice.get().as_ref().unwrap() }
    }
}

impl MutableRasterImage for OsMutBitmap8<'_> {
    fn slice_mut(&mut self) -> &mut [Self::PixelType] {
        self.slice.get_mut()
    }
}

impl BasicDrawing for OsMutBitmap8<'_> {
    fn fill_rect(&mut self, rect: Rect, color: Self::PixelType) {
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
            Self::memset_colors(self.slice_mut(), cursor, width * height, color);
        } else {
            for _ in 0..height {
                Self::memset_colors(self.slice_mut(), cursor, width, color);
                cursor += stride;
            }
        }
    }

    fn draw_hline(&mut self, point: Point, width: isize, color: Self::PixelType) {
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
        Self::memset_colors(self.slice_mut(), cursor, w as usize, color);
    }

    fn draw_vline(&mut self, point: Point, height: isize, color: Self::PixelType) {
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
        if b >= (self.height as isize) {
            h = (self.height as isize) - dy;
        }
        if h <= 0 {
            return;
        }

        let stride = self.stride;
        let mut cursor = dx as usize + dy as usize * stride;
        for _ in 0..h {
            self.slice_mut()[cursor] = color;
            cursor += stride;
        }
    }

    fn blt<T>(&mut self, src: &T, origin: Point, rect: Rect)
    where
        T: RasterImage<PixelType = <Self as ImageTrait>::PixelType>,
    {
        self.blt_main(src, origin, rect, None);
    }
}

impl RasterFontWriter for OsMutBitmap8<'_> {}

impl<'a> From<&'a OsMutBitmap8<'a>> for OsBitmap8<'a> {
    fn from(src: &'a OsMutBitmap8) -> Self {
        Self::from_slice(src.slice(), src.size(), src.stride())
    }
}
