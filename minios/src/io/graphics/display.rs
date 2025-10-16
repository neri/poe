//! Display implementations.

use super::PixelFormat;
use super::color::IndexedColor;
use core::convert::Infallible;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

pub struct BitmapDisplay8 {
    base: *mut u8,
    dims: Size,
    stride: usize,
}

impl BitmapDisplay8 {
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
    #[inline]
    pub unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Self> {
        if current.info.pixel_format != PixelFormat::Indexed8 {
            return None;
        }
        Some(Self {
            base: current.fb.as_usize() as *mut u8,
            dims: Size::new(current.info.width as u32, current.info.height as u32),
            stride: current.info.bytes_per_scanline as usize,
        })
    }

    #[inline]
    fn pos(&self, point: Point) -> Option<usize> {
        let x = point.x as usize;
        let y = point.y as usize;
        if x >= self.dims.width as usize || y >= self.dims.height as usize {
            return None;
        }
        Some(y * self.stride + x)
    }

    #[inline]
    fn draw_pixel(&mut self, point: Point, color: IndexedColor) {
        if let Some(pos) = self.pos(point) {
            unsafe {
                self.base.add(pos).write_volatile(color.0);
            }
        }
    }

    /// # Safety
    ///
    /// This function does not check bounds.
    #[inline]
    unsafe fn draw_hline(&mut self, origin: Point, length: u32, color: IndexedColor) {
        unsafe {
            self.base
                .add(origin.y as usize * self.stride + origin.x as usize)
                .write_bytes(color.0, length as usize);
        }
    }
}

impl Dimensions for BitmapDisplay8 {
    #[inline]
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::zero(), self.dims)
    }
}

impl DrawTarget for BitmapDisplay8 {
    type Color = IndexedColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels.into_iter() {
            self.draw_pixel(pixel.0, pixel.1);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let origin = area
            .top_left
            .clamp(Point::zero(), self.bounding_box().bottom_right().unwrap());
        let Some(bottom_right) = area.bottom_right() else {
            return Ok(());
        };
        let bottom_right =
            bottom_right.clamp(Point::zero(), self.bounding_box().bottom_right().unwrap());
        for y in origin.y..bottom_right.y {
            unsafe {
                self.draw_hline(Point::new(origin.x, y), area.size.width, color);
            }
        }
        Ok(())
    }
}
