//! Framebuffer Display implementations.
use super::PixelFormat;
use super::color::IndexedColor;
use crate::io::fonts::SimpleGlyph;
use crate::*;
use core::convert::Infallible;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

#[repr(transparent)]
pub struct FbDisplay8(Box<dyn FrameBuffer>);

impl FbDisplay8 {
    #[inline]
    const fn new(fb: Box<dyn FrameBuffer>) -> Self {
        Self(fb)
    }

    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
    pub unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Self> {
        match current.info.pixel_format {
            PixelFormat::Indexed8 => unsafe { Fb8::from_graphics(current).map(Self::new) },
            PixelFormat::BGRX8888 => unsafe { Fb32::from_graphics(current).map(Self::new) },
            _ => None,
        }
    }

    #[inline]
    pub fn is_supported_pixel_format(format: PixelFormat) -> bool {
        matches!(format, PixelFormat::Indexed8 | PixelFormat::BGRX8888)
    }

    #[inline]
    pub fn draw_glyph(
        &mut self,
        origin: Point,
        glyph: SimpleGlyph,
        fg: IndexedColor,
        bg: IndexedColor,
    ) {
        let mut iter = glyph.data.iter().copied();
        let w8 = glyph.dims.0 / 8;
        let w7 = glyph.dims.0 & 7;
        for y in 0..glyph.dims.1 {
            let mut sx = 0;
            for _ in 0..w8 {
                let Some(byte) = iter.next() else {
                    return;
                };
                let mut acc = 0x80;
                for _ in 0..8 {
                    let color = if (byte & acc) != 0 { fg } else { bg };
                    self.0
                        .draw_pixel(Point::new(origin.x + sx, origin.y + y as i32), color);
                    acc >>= 1;
                    sx += 1;
                }
            }
            if w7 > 0 {
                let Some(byte) = iter.next() else {
                    return;
                };
                let mut acc = 0x80;
                for _ in 0..w7 {
                    let color = if (byte & acc) != 0 { fg } else { bg };
                    self.0
                        .draw_pixel(Point::new(origin.x + sx, origin.y + y as i32), color);
                    acc >>= 1;
                    sx += 1;
                }
            }
        }
    }
}

impl Dimensions for FbDisplay8 {
    #[inline]
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::zero(), self.0.dims())
    }
}

impl DrawTarget for FbDisplay8 {
    type Color = IndexedColor;
    type Error = Infallible;

    #[inline]
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.0.draw_iter(&mut pixels.into_iter());
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        if let Some(bottom_right) = area.bottom_right() {
            let top_left = area.top_left;
            let limit = self.0.dims();
            if (top_left.x as usize) < limit.width as usize
                && (top_left.y as usize) < limit.height as usize
                && (bottom_right.x as usize) <= limit.width as usize
                && (bottom_right.y as usize) <= limit.height as usize
            {
                unsafe {
                    self.0.blt_fast(area, &mut colors.into_iter());
                }
                return Ok(());
            }
        }
        self.draw_iter(
            area.points()
                .zip(colors)
                .map(|(pos, color)| Pixel(pos, color)),
        )
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let limit = self.bounding_box().bottom_right().unwrap();
        let origin = area.top_left.max(Point::zero());
        if origin.x > limit.x || origin.y > limit.y {
            return Ok(());
        }
        let Some(bottom_right) = area.bottom_right() else {
            return Ok(());
        };
        let bottom_right = bottom_right.clamp(Point::zero(), limit);
        let length = (bottom_right.x - origin.x + 1) as u32;
        for y in origin.y..=bottom_right.y {
            unsafe {
                self.0.fill_fast(Point::new(origin.x, y), length, color);
            }
        }
        Ok(())
    }
}

pub trait FrameBuffer {
    fn dims(&self) -> Size;

    fn draw_pixel(&mut self, point: Point, color: IndexedColor);

    fn draw_iter(&mut self, points: &mut dyn Iterator<Item = Pixel<IndexedColor>>) {
        for pixel in points.into_iter() {
            self.draw_pixel(pixel.0, pixel.1);
        }
    }

    /// # Safety
    ///
    /// This function does not check bounds.
    unsafe fn blt_fast(
        &mut self,
        area: &Rectangle,
        colors: &mut dyn Iterator<Item = IndexedColor>,
    ) {
        for y in area.rows() {
            for x in area.columns() {
                let Some(color) = colors.next() else { return };
                self.draw_pixel(Point::new(x as i32, y as i32), color);
            }
        }
    }

    /// # Safety
    ///
    /// This function does not check bounds.
    unsafe fn fill_fast(&mut self, origin: Point, length: u32, color: IndexedColor);
}

struct Fb8 {
    fb: *mut u8,
    stride: usize,
    dims: Size,
}

impl Fb8 {
    #[inline]
    unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Box<dyn FrameBuffer>> {
        if current.info.pixel_format == PixelFormat::Indexed8 {
            match current.info.width as usize {
                // 640 => Some(Box::new(Fb8Fixed::<640> {
                //     fb: current.fb.as_usize() as *mut u8,
                //     dims: Size::new(current.info.width as u32, current.info.height as u32),
                // })),
                // 800 => Some(Box::new(Fb8Fixed::<800> {
                //     fb: current.fb.as_usize() as *mut u8,
                //     dims: Size::new(current.info.width as u32, current.info.height as u32),
                // })),
                // 1024 => Some(Box::new(Fb8Fixed::<1024> {
                //     fb: current.fb.as_usize() as *mut u8,
                //     dims: Size::new(current.info.width as u32, current.info.height as u32),
                // })),
                _ => Some(Box::new(Self {
                    fb: current.fb.as_usize() as *mut u8,
                    dims: Size::new(current.info.width as u32, current.info.height as u32),
                    stride: current.info.bytes_per_scanline as usize,
                })),
            }
        } else {
            None
        }
    }
}

impl FrameBuffer for Fb8 {
    fn dims(&self) -> Size {
        self.dims
    }

    #[inline]
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

    unsafe fn blt_fast(
        &mut self,
        area: &Rectangle,
        colors: &mut dyn Iterator<Item = IndexedColor>,
    ) {
        unsafe {
            let mut p = self
                .fb
                .add(area.top_left.y as usize * self.stride + area.top_left.x as usize);
            let stride = self.stride - area.size.width as usize;
            for _y in area.rows() {
                for _x in area.columns() {
                    let Some(color) = colors.next() else { return };
                    p.write_volatile(color.0);
                    p = p.add(1);
                }
                p = p.add(stride);
            }
        }
    }

    unsafe fn fill_fast(&mut self, origin: Point, length: u32, color: IndexedColor) {
        unsafe {
            self.fb
                .add(origin.y as usize * self.stride + origin.x as usize)
                .write_bytes(color.0, length as usize);
        }
    }
}

/// Special implementation for fixed width framebuffer
#[allow(unused)]
struct Fb8Fixed<const WIDTH: usize> {
    fb: *mut u8,
    dims: Size,
}

#[allow(unused)]
impl<const WIDTH: usize> FrameBuffer for Fb8Fixed<WIDTH> {
    fn dims(&self) -> Size {
        self.dims
    }

    #[inline]
    fn draw_pixel(&mut self, point: Point, color: IndexedColor) {
        let x = point.x as usize;
        let y = point.y as usize;
        if x >= self.dims.width as usize || y >= self.dims.height as usize {
            return;
        }
        let pos = y * WIDTH + x;
        unsafe {
            self.fb.add(pos).write_volatile(color.0);
        }
    }

    unsafe fn fill_fast(&mut self, origin: Point, length: u32, color: IndexedColor) {
        unsafe {
            self.fb
                .add(origin.y as usize * WIDTH + origin.x as usize)
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
    unsafe fn from_graphics(current: &super::CurrentMode) -> Option<Box<dyn FrameBuffer>> {
        if current.info.pixel_format == PixelFormat::BGRX8888 {
            Some(Box::new(Self {
                fb: current.fb.as_usize() as *mut u32,
                dims: Size::new(current.info.width as u32, current.info.height as u32),
                stride: current.info.pixels_per_scanline()? as usize,
            }))
        } else {
            None
        }
    }
}

impl FrameBuffer for Fb32 {
    fn dims(&self) -> Size {
        self.dims
    }

    #[inline]
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

    unsafe fn fill_fast(&mut self, origin: Point, length: u32, color: IndexedColor) {
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
