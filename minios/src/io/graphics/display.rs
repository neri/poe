//! Framebuffer Display implementations.
use super::PixelFormat;
use super::color::IndexedColor;
use crate::*;
use core::convert::Infallible;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

pub struct FbDisplay8 {
    fb: Box<dyn FrameBuffer>,
}

impl FbDisplay8 {
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
    pub unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Self> {
        match current.info.pixel_format {
            PixelFormat::Indexed8 => unsafe {
                Fb8::from_graphics(current).map(|fb| Self { fb: Box::new(fb) })
            },
            PixelFormat::BGRX8888 => unsafe {
                Fb32::from_graphics(current).map(|fb| Self { fb: Box::new(fb) })
            },
            _ => None,
        }
    }

    #[inline]
    pub fn is_supported_pixel_format(format: PixelFormat) -> bool {
        matches!(format, PixelFormat::Indexed8 | PixelFormat::BGRX8888)
    }
}

impl Dimensions for FbDisplay8 {
    #[inline]
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::zero(), self.fb.dims())
    }
}

impl DrawTarget for FbDisplay8 {
    type Color = IndexedColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels.into_iter() {
            self.fb.draw_pixel(pixel.0, pixel.1);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let limit = self.bounding_box().bottom_right().unwrap();
        let origin = area.top_left.clamp(Point::zero(), limit);
        let Some(bottom_right) = area.bottom_right() else {
            return Ok(());
        };
        let bottom_right = bottom_right.clamp(Point::zero(), limit);
        let length = (bottom_right.x - origin.x + 1) as u32;
        for y in origin.y..=bottom_right.y {
            unsafe {
                self.fb.fill_block(Point::new(origin.x, y), length, color);
            }
        }
        Ok(())
    }
}

pub trait FrameBuffer {
    fn dims(&self) -> Size;

    fn draw_pixel(&mut self, point: Point, color: IndexedColor);

    /// # Safety
    ///
    /// This function does not check bounds.
    unsafe fn fill_block(&mut self, origin: Point, length: u32, color: IndexedColor);
}

struct Fb8 {
    fb: *mut u8,
    stride: usize,
    dims: Size,
}

impl Fb8 {
    #[inline]
    unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Self> {
        if current.info.pixel_format == PixelFormat::Indexed8 {
            Some(Self {
                fb: current.fb.as_usize() as *mut u8,
                dims: Size::new(current.info.width as u32, current.info.height as u32),
                stride: current.info.bytes_per_scanline as usize,
            })
        } else {
            None
        }
    }
}

impl FrameBuffer for Fb8 {
    fn dims(&self) -> Size {
        self.dims
    }

    fn draw_pixel(&mut self, point: Point, color: IndexedColor) {
        let x = point.x as usize;
        let y = point.y as usize;
        if x >= self.dims.width as usize || y >= self.dims.height as usize {
            return;
        }
        let pos = y * self.stride + x;
        unsafe {
            self.fb.add(pos).write_volatile(color.0);
        }
    }

    unsafe fn fill_block(&mut self, origin: Point, length: u32, color: IndexedColor) {
        unsafe {
            self.fb
                .add(origin.y as usize * self.stride + origin.x as usize)
                .write_bytes(color.0, length as usize);
        }
    }
}

struct Fb32 {
    fb: *mut u32,
    stride: usize,
    dims: Size,
}

impl Fb32 {
    #[inline]
    unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Self> {
        if current.info.pixel_format == PixelFormat::BGRX8888
            && (current.info.bytes_per_scanline & 3) == 0
        {
            Some(Self {
                fb: current.fb.as_usize() as *mut u32,
                dims: Size::new(current.info.width as u32, current.info.height as u32),
                stride: (current.info.bytes_per_scanline / 4) as usize,
            })
        } else {
            None
        }
    }
}

impl FrameBuffer for Fb32 {
    fn dims(&self) -> Size {
        self.dims
    }

    fn draw_pixel(&mut self, point: Point, color: IndexedColor) {
        let x = point.x as usize;
        let y = point.y as usize;
        if x >= self.dims.width as usize || y >= self.dims.height as usize {
            return;
        }
        let pos = y * self.stride + x;
        let color = super::color::COLOR_PALETTE[color.0 as usize];
        unsafe {
            self.fb.add(pos).write_volatile(color);
        }
    }

    unsafe fn fill_block(&mut self, origin: Point, length: u32, color: IndexedColor) {
        let color = super::color::COLOR_PALETTE[color.0 as usize];
        unsafe {
            let slice = core::slice::from_raw_parts_mut(
                self.fb
                    .add(origin.y as usize * self.stride + origin.x as usize),
                length as usize,
            );
            slice.fill(color);
        }
    }
}
