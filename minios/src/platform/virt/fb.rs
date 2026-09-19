//! Linear framebuffer set up by the firmware

use crate::io::graphics::{
    CurrentMode, GraphicsOutputDevice, ModeIndex, ModeInfo, PixelFormat, PreferredGraphicsMode,
};
use crate::*;

/// Linear framebuffer described by the firmware
///
/// The pixel format is described by the position and the size of each color component.
#[derive(Debug, Clone, Copy)]
pub struct Framebuffer {
    pub base: usize,
    pub width: usize,
    pub height: usize,
    /// Bytes per line
    pub stride: usize,
    pub bits_per_pixel: u8,
    /// (position, size) in bits
    pub red: (u8, u8),
    pub green: (u8, u8),
    pub blue: (u8, u8),
}

impl Framebuffer {
    /// Returns the pixel format, if it is supported.
    ///
    /// Only 32bpp xRGB (in little endian) is supported, which is `BGRX8888` in memory.
    pub fn pixel_format(&self) -> Option<PixelFormat> {
        (self.bits_per_pixel == 32
            && self.red == (16, 8)
            && self.green == (8, 8)
            && self.blue == (0, 8))
            .then_some(PixelFormat::BGRX8888)
    }

    /// Returns the video mode, if the framebuffer can be used as a graphics output device.
    pub fn mode_info(&self) -> Option<ModeInfo> {
        let pixel_format = self.pixel_format()?;
        let bytes_per_pixel = pixel_format.bytes_per_pixel();
        if self.base == 0
            || (self.base | self.stride) % bytes_per_pixel != 0
            || self.stride < self.width * bytes_per_pixel
        {
            return None;
        }
        Some(ModeInfo {
            width: u16::try_from(self.width).ok().filter(|&v| v > 0)?,
            height: u16::try_from(self.height).ok().filter(|&v| v > 0)?,
            bytes_per_scanline: u16::try_from(self.stride).ok()?,
            pixel_format,
        })
    }

    #[inline]
    pub const fn size(&self) -> usize {
        self.stride * self.height
    }
}

/// Graphics output device for the framebuffer set up by the firmware
///
/// Only the mode set by the firmware is available.
pub struct FirmwareFb {
    modes: [ModeInfo; 1],
    current_mode: CurrentMode,
}

impl FirmwareFb {
    /// Registers the framebuffer as the graphics output device.
    ///
    /// Returns `false` if the framebuffer is not supported.
    pub fn install(fb: &Framebuffer) -> bool {
        let Some(info) = fb.mode_info() else {
            return false;
        };
        let driver = Box::new(Self {
            modes: [info],
            current_mode: CurrentMode {
                current: ModeIndex(0),
                info,
                fb: PhysicalAddress::from_usize(fb.base),
                fb_size: fb.size(),
            },
        });
        System::conctl().set_graphics(driver as Box<dyn GraphicsOutputDevice>);
        true
    }
}

impl GraphicsOutputDevice for FirmwareFb {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }

    fn preferred_graphics_mode(&self) -> Option<PreferredGraphicsMode> {
        Some(self.modes[0].into())
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        (mode == self.current_mode.current).then_some(()).ok_or(())
    }

    fn detach(&mut self) {
        // The MMU may be off (Device memory), so clear with aligned 32-bit writes
        let base = self.current_mode.fb.as_usize() as *mut u32;
        for i in 0..self.current_mode.fb_size / 4 {
            unsafe {
                base.add(i).write_volatile(0);
            }
        }
    }
}
