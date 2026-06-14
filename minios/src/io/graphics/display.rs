//! Framebuffer Display implementations.
use core::convert::Infallible;

use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

use super::PixelFormat;
use super::color::IndexedColor;
use crate::io::fonts::SimpleGlyph;
use crate::io::graphics::color::IndexedColorX4;
use crate::*;

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
        self.0.draw_glyph(origin, glyph, fg, bg);
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
        if self.bounding_box().size.width == length {
            unsafe {
                self.0.fill_fast(
                    origin,
                    length * (bottom_right.y - origin.y + 1) as u32,
                    color,
                );
            }
        } else {
            for y in origin.y..=bottom_right.y {
                unsafe {
                    self.0.fill_fast(Point::new(origin.x, y), length, color);
                }
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

    fn draw_glyph(
        &mut self,
        origin: Point,
        glyph: SimpleGlyph,
        fg: IndexedColor,
        bg: IndexedColor,
    ) {
        self.draw_glyph_fallback(origin, glyph, fg, bg);
    }

    /// Draw 8 pixels from a pattern.
    fn put_8pixels(
        &mut self,
        origin: Point,
        pattern: u8,
        length: u32,
        bg_color: IndexedColor,
        fg_color: IndexedColor,
    ) {
        let mut acc = 0x80;
        for i in 0..length {
            let color = if (pattern & acc) != 0 {
                fg_color
            } else {
                bg_color
            };
            self.draw_pixel(Point::new(origin.x + i as i32, origin.y), color);
            acc >>= 1;
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

    #[inline]
    fn draw_glyph_fallback(
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
            let mut origin = Point::new(origin.x, origin.y + y as i32);
            for _ in 0..w8 {
                let Some(pattern) = iter.next() else {
                    return;
                };
                self.put_8pixels(origin, pattern, 8, bg, fg);
                origin.x += 8;
            }
            if w7 > 0 {
                let Some(pattern) = iter.next() else {
                    return;
                };
                self.put_8pixels(origin, pattern, w7, bg, fg);
            }
        }
    }
}

struct Fb8 {
    fb: *mut u8,
    stride: usize,
    dims: Size,
}

const PATTERN_LUT_8B: [u64; 256] = [
    0x0000000000000000,
    0xFF00000000000000,
    0x00FF000000000000,
    0xFFFF000000000000,
    0x0000FF0000000000,
    0xFF00FF0000000000,
    0x00FFFF0000000000,
    0xFFFFFF0000000000,
    0x000000FF00000000,
    0xFF0000FF00000000,
    0x00FF00FF00000000,
    0xFFFF00FF00000000,
    0x0000FFFF00000000,
    0xFF00FFFF00000000,
    0x00FFFFFF00000000,
    0xFFFFFFFF00000000,
    0x00000000FF000000,
    0xFF000000FF000000,
    0x00FF0000FF000000,
    0xFFFF0000FF000000,
    0x0000FF00FF000000,
    0xFF00FF00FF000000,
    0x00FFFF00FF000000,
    0xFFFFFF00FF000000,
    0x000000FFFF000000,
    0xFF0000FFFF000000,
    0x00FF00FFFF000000,
    0xFFFF00FFFF000000,
    0x0000FFFFFF000000,
    0xFF00FFFFFF000000,
    0x00FFFFFFFF000000,
    0xFFFFFFFFFF000000,
    0x0000000000FF0000,
    0xFF00000000FF0000,
    0x00FF000000FF0000,
    0xFFFF000000FF0000,
    0x0000FF0000FF0000,
    0xFF00FF0000FF0000,
    0x00FFFF0000FF0000,
    0xFFFFFF0000FF0000,
    0x000000FF00FF0000,
    0xFF0000FF00FF0000,
    0x00FF00FF00FF0000,
    0xFFFF00FF00FF0000,
    0x0000FFFF00FF0000,
    0xFF00FFFF00FF0000,
    0x00FFFFFF00FF0000,
    0xFFFFFFFF00FF0000,
    0x00000000FFFF0000,
    0xFF000000FFFF0000,
    0x00FF0000FFFF0000,
    0xFFFF0000FFFF0000,
    0x0000FF00FFFF0000,
    0xFF00FF00FFFF0000,
    0x00FFFF00FFFF0000,
    0xFFFFFF00FFFF0000,
    0x000000FFFFFF0000,
    0xFF0000FFFFFF0000,
    0x00FF00FFFFFF0000,
    0xFFFF00FFFFFF0000,
    0x0000FFFFFFFF0000,
    0xFF00FFFFFFFF0000,
    0x00FFFFFFFFFF0000,
    0xFFFFFFFFFFFF0000,
    0x000000000000FF00,
    0xFF0000000000FF00,
    0x00FF00000000FF00,
    0xFFFF00000000FF00,
    0x0000FF000000FF00,
    0xFF00FF000000FF00,
    0x00FFFF000000FF00,
    0xFFFFFF000000FF00,
    0x000000FF0000FF00,
    0xFF0000FF0000FF00,
    0x00FF00FF0000FF00,
    0xFFFF00FF0000FF00,
    0x0000FFFF0000FF00,
    0xFF00FFFF0000FF00,
    0x00FFFFFF0000FF00,
    0xFFFFFFFF0000FF00,
    0x00000000FF00FF00,
    0xFF000000FF00FF00,
    0x00FF0000FF00FF00,
    0xFFFF0000FF00FF00,
    0x0000FF00FF00FF00,
    0xFF00FF00FF00FF00,
    0x00FFFF00FF00FF00,
    0xFFFFFF00FF00FF00,
    0x000000FFFF00FF00,
    0xFF0000FFFF00FF00,
    0x00FF00FFFF00FF00,
    0xFFFF00FFFF00FF00,
    0x0000FFFFFF00FF00,
    0xFF00FFFFFF00FF00,
    0x00FFFFFFFF00FF00,
    0xFFFFFFFFFF00FF00,
    0x0000000000FFFF00,
    0xFF00000000FFFF00,
    0x00FF000000FFFF00,
    0xFFFF000000FFFF00,
    0x0000FF0000FFFF00,
    0xFF00FF0000FFFF00,
    0x00FFFF0000FFFF00,
    0xFFFFFF0000FFFF00,
    0x000000FF00FFFF00,
    0xFF0000FF00FFFF00,
    0x00FF00FF00FFFF00,
    0xFFFF00FF00FFFF00,
    0x0000FFFF00FFFF00,
    0xFF00FFFF00FFFF00,
    0x00FFFFFF00FFFF00,
    0xFFFFFFFF00FFFF00,
    0x00000000FFFFFF00,
    0xFF000000FFFFFF00,
    0x00FF0000FFFFFF00,
    0xFFFF0000FFFFFF00,
    0x0000FF00FFFFFF00,
    0xFF00FF00FFFFFF00,
    0x00FFFF00FFFFFF00,
    0xFFFFFF00FFFFFF00,
    0x000000FFFFFFFF00,
    0xFF0000FFFFFFFF00,
    0x00FF00FFFFFFFF00,
    0xFFFF00FFFFFFFF00,
    0x0000FFFFFFFFFF00,
    0xFF00FFFFFFFFFF00,
    0x00FFFFFFFFFFFF00,
    0xFFFFFFFFFFFFFF00,
    0x00000000000000FF,
    0xFF000000000000FF,
    0x00FF0000000000FF,
    0xFFFF0000000000FF,
    0x0000FF00000000FF,
    0xFF00FF00000000FF,
    0x00FFFF00000000FF,
    0xFFFFFF00000000FF,
    0x000000FF000000FF,
    0xFF0000FF000000FF,
    0x00FF00FF000000FF,
    0xFFFF00FF000000FF,
    0x0000FFFF000000FF,
    0xFF00FFFF000000FF,
    0x00FFFFFF000000FF,
    0xFFFFFFFF000000FF,
    0x00000000FF0000FF,
    0xFF000000FF0000FF,
    0x00FF0000FF0000FF,
    0xFFFF0000FF0000FF,
    0x0000FF00FF0000FF,
    0xFF00FF00FF0000FF,
    0x00FFFF00FF0000FF,
    0xFFFFFF00FF0000FF,
    0x000000FFFF0000FF,
    0xFF0000FFFF0000FF,
    0x00FF00FFFF0000FF,
    0xFFFF00FFFF0000FF,
    0x0000FFFFFF0000FF,
    0xFF00FFFFFF0000FF,
    0x00FFFFFFFF0000FF,
    0xFFFFFFFFFF0000FF,
    0x0000000000FF00FF,
    0xFF00000000FF00FF,
    0x00FF000000FF00FF,
    0xFFFF000000FF00FF,
    0x0000FF0000FF00FF,
    0xFF00FF0000FF00FF,
    0x00FFFF0000FF00FF,
    0xFFFFFF0000FF00FF,
    0x000000FF00FF00FF,
    0xFF0000FF00FF00FF,
    0x00FF00FF00FF00FF,
    0xFFFF00FF00FF00FF,
    0x0000FFFF00FF00FF,
    0xFF00FFFF00FF00FF,
    0x00FFFFFF00FF00FF,
    0xFFFFFFFF00FF00FF,
    0x00000000FFFF00FF,
    0xFF000000FFFF00FF,
    0x00FF0000FFFF00FF,
    0xFFFF0000FFFF00FF,
    0x0000FF00FFFF00FF,
    0xFF00FF00FFFF00FF,
    0x00FFFF00FFFF00FF,
    0xFFFFFF00FFFF00FF,
    0x000000FFFFFF00FF,
    0xFF0000FFFFFF00FF,
    0x00FF00FFFFFF00FF,
    0xFFFF00FFFFFF00FF,
    0x0000FFFFFFFF00FF,
    0xFF00FFFFFFFF00FF,
    0x00FFFFFFFFFF00FF,
    0xFFFFFFFFFFFF00FF,
    0x000000000000FFFF,
    0xFF0000000000FFFF,
    0x00FF00000000FFFF,
    0xFFFF00000000FFFF,
    0x0000FF000000FFFF,
    0xFF00FF000000FFFF,
    0x00FFFF000000FFFF,
    0xFFFFFF000000FFFF,
    0x000000FF0000FFFF,
    0xFF0000FF0000FFFF,
    0x00FF00FF0000FFFF,
    0xFFFF00FF0000FFFF,
    0x0000FFFF0000FFFF,
    0xFF00FFFF0000FFFF,
    0x00FFFFFF0000FFFF,
    0xFFFFFFFF0000FFFF,
    0x00000000FF00FFFF,
    0xFF000000FF00FFFF,
    0x00FF0000FF00FFFF,
    0xFFFF0000FF00FFFF,
    0x0000FF00FF00FFFF,
    0xFF00FF00FF00FFFF,
    0x00FFFF00FF00FFFF,
    0xFFFFFF00FF00FFFF,
    0x000000FFFF00FFFF,
    0xFF0000FFFF00FFFF,
    0x00FF00FFFF00FFFF,
    0xFFFF00FFFF00FFFF,
    0x0000FFFFFF00FFFF,
    0xFF00FFFFFF00FFFF,
    0x00FFFFFFFF00FFFF,
    0xFFFFFFFFFF00FFFF,
    0x0000000000FFFFFF,
    0xFF00000000FFFFFF,
    0x00FF000000FFFFFF,
    0xFFFF000000FFFFFF,
    0x0000FF0000FFFFFF,
    0xFF00FF0000FFFFFF,
    0x00FFFF0000FFFFFF,
    0xFFFFFF0000FFFFFF,
    0x000000FF00FFFFFF,
    0xFF0000FF00FFFFFF,
    0x00FF00FF00FFFFFF,
    0xFFFF00FF00FFFFFF,
    0x0000FFFF00FFFFFF,
    0xFF00FFFF00FFFFFF,
    0x00FFFFFF00FFFFFF,
    0xFFFFFFFF00FFFFFF,
    0x00000000FFFFFFFF,
    0xFF000000FFFFFFFF,
    0x00FF0000FFFFFFFF,
    0xFFFF0000FFFFFFFF,
    0x0000FF00FFFFFFFF,
    0xFF00FF00FFFFFFFF,
    0x00FFFF00FFFFFFFF,
    0xFFFFFF00FFFFFFFF,
    0x000000FFFFFFFFFF,
    0xFF0000FFFFFFFFFF,
    0x00FF00FFFFFFFFFF,
    0xFFFF00FFFFFFFFFF,
    0x0000FFFFFFFFFFFF,
    0xFF00FFFFFFFFFFFF,
    0x00FFFFFFFFFFFFFF,
    0xFFFFFFFFFFFFFFFF,
];

impl Fb8 {
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
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

    fn draw_glyph(
        &mut self,
        origin: Point,
        glyph: SimpleGlyph,
        fg: IndexedColor,
        bg: IndexedColor,
    ) {
        if glyph.dims.0 == 8 && (origin.x & 3) == 0 {
            // Fast path for 8-pixel wide glyphs with 4-byte aligned x coordinate
            let x = origin.x as usize;
            let y = origin.y as usize;
            if x > self.dims.width as usize - glyph.dims.0 as usize
                || y > self.dims.height as usize - glyph.dims.1 as usize
            {
                return;
            }

            let fg = (IndexedColorX4::from_indexed_color(fg).0 as u64) * 0x1_0000_0001;
            let bg = (IndexedColorX4::from_indexed_color(bg).0 as u64) * 0x1_0000_0001;
            let mut pos = unsafe { self.fb.add(y * self.stride + x) };

            for pettern in glyph.data.iter().copied() {
                let mask = PATTERN_LUT_8B[pettern as usize];
                let data = (fg & mask) | (bg & !mask);
                unsafe {
                    (pos as *mut u32).write_volatile(data as u32);
                    (pos.add(4) as *mut u32).write_volatile((data >> 32) as u32);
                    pos = pos.add(self.stride);
                }
            }
        } else {
            self.draw_glyph_fallback(origin, glyph, fg, bg);
        }
    }

    fn put_8pixels(
        &mut self,
        origin: Point,
        pattern: u8,
        length: u32,
        bg_color: IndexedColor,
        fg_color: IndexedColor,
    ) {
        let x = origin.x as usize;
        let y = origin.y as usize;
        if x >= self.dims.width as usize || y >= self.dims.height as usize {
            return;
        }
        let width = length.min((self.dims.width - x as u32) as u32);
        let mut pos = unsafe { self.fb.add(y * self.stride + x) };

        let mut acc = 0x80;
        for _ in 0..width {
            let color = if (pattern & acc) != 0 {
                fg_color
            } else {
                bg_color
            };
            unsafe {
                pos.write_volatile(color.0);
            }
            acc >>= 1;
            pos = unsafe { pos.add(1) };
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
    /// # Safety
    ///
    /// This function is unsafe because it dereferences a raw pointer.
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
        let color = IndexedColor::COLOR_PALETTE[color.0 as usize];
        unsafe {
            self.fb.add(pos).write_volatile(color);
        }
    }

    fn put_8pixels(
        &mut self,
        origin: Point,
        pattern: u8,
        length: u32,
        bg_color: IndexedColor,
        fg_color: IndexedColor,
    ) {
        let x = origin.x as usize;
        let y = origin.y as usize;
        if x >= self.dims.width as usize || y >= self.dims.height as usize {
            return;
        }
        let mut pos = y * self.stride + x;
        let length = length.min((self.dims.width - x as u32) as u32);
        let bg_color = IndexedColor::COLOR_PALETTE[bg_color.0 as usize];
        let fg_color = IndexedColor::COLOR_PALETTE[fg_color.0 as usize];

        let mut acc = 0x80;
        for _ in 0..length {
            let color = if (pattern & acc) != 0 {
                fg_color
            } else {
                bg_color
            };
            unsafe {
                self.fb.add(pos).write_volatile(color);
            }
            acc >>= 1;
            pos += 1;
        }
    }

    unsafe fn fill_fast(&mut self, origin: Point, length: u32, color: IndexedColor) {
        let color = IndexedColor::COLOR_PALETTE[color.0 as usize];
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
