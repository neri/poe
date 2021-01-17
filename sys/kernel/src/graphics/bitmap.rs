// Bitmap

use super::color::*;
use super::coords::*;

pub trait BitmapTrait {
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

    fn get_pixel(&self, point: Point) -> Option<Self::PixelType>
    where
        Self::PixelType: Copy,
    {
        if point.is_within(Rect::from(self.size())) {
            Some(unsafe { self.get_pixel_unchecked(point) })
        } else {
            None
        }
    }

    /// SAFETY: The point must be within the size range.
    unsafe fn get_pixel_unchecked(&self, point: Point) -> Self::PixelType
    where
        Self::PixelType: Copy,
    {
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

impl<'a> From<&'a OsMutBitmap8<'a>> for OsBitmap8<'a> {
    fn from(src: &'a OsMutBitmap8) -> Self {
        Self::from_slice(src.slice(), src.size(), src.stride())
    }
}
