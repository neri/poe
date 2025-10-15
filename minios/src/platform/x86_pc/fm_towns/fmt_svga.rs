//! FM TOWNS Graphics Mode Driver

use super::{crtc::Crtc, fmt_text::FmtText};
use crate::io::graphics::*;
use crate::*;
use x86::isolated_io::IoPortWB;

pub struct FmtSvga {
    modes: Vec<ModeInfo>,
    current_mode: CurrentMode,
}

/// Video mode settings for 640x480x8 mode
#[rustfmt::skip]
const VIDEO_MODE_SETTINGS: [u16; 30] = [
    0x0060, 0x02c0, /* ---   --- */ 0x031f, 0x0000, 0x0004, 0x0000,
    0x0419, 0x008a, 0x030a, 0x008a, 0x030a, 0x0046, 0x0406, 0x0046,
    0x0406, 0x0000, 0x008a, 0x0000, 0x0050, 0x0000, 0x008a, 0x0000,
    0x0050, 0x0058, 0x0001, 0x0000, 0x000f, 0x0002, 0x0000, 0x0192,
];

impl FmtSvga {
    #[inline]
    const fn new() -> Self {
        Self {
            modes: Vec::new(),
            current_mode: CurrentMode::empty(),
        }
    }

    pub(super) unsafe fn init() {
        let mut driver = Box::new(Self::new());
        let inner_mode = ModeInfo {
            width: 640,
            height: 480,
            bytes_per_scanline: 640,
            pixel_format: PixelFormat::Indexed8,
        };
        driver.modes.push(inner_mode);
        driver.current_mode = CurrentMode {
            current: ModeIndex(0),
            info: inner_mode,
            fb: PhysicalAddress::from_usize(0x8010_0000),
            fb_size: 512 * 1024,
        };

        System::console_controller().set_graphics(driver as Box<dyn GraphicsOutput>);
    }
}

impl GraphicsOutput for FmtSvga {
    fn deactivate(&mut self) {
        unsafe {
            FmtText::hw_set_mode();
        }
    }

    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        unsafe {
            let _inner_mode = *self.modes.get(mode.0 as usize).ok_or(())?;

            Crtc::set_mode(&VIDEO_MODE_SETTINGS, 0b0000_1010, 0b0001_1000, 0b0000_1000);

            for (i, &color) in COLOR_PALETTE.iter().enumerate() {
                IoPortWB(0xfd90).write(i as u8);
                IoPortWB(0xfd92).write(color as u8);
                IoPortWB(0xfd96).write((color >> 8) as u8);
                IoPortWB(0xfd94).write((color >> 16) as u8);
            }

            Ok(())
        }
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }
}
