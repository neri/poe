//! FM TOWNS Graphics Mode Driver

use x86::isolated_io::IoPortWB;

use super::crtc::Crtc;
use super::fmt_text::FmtText;
use crate::arch::cpu::Cpu;
use crate::io::graphics::color::IndexedColor;
use crate::io::graphics::*;
use crate::*;

pub struct FmtSvga {
    modes: Vec<ModeInfo>,
    current_mode: CurrentMode,
    preferred_graphics_mode: PreferredGraphicsMode,
}

/// Video mode settings for 640x480x8 mode
const VIDEO_MODE_SETTINGS: [u16; 30] = [
    0x0060, 0x02c0, /* ---   --- */ 0x031f, 0x0000, 0x0004, 0x0000, //
    0x0419, 0x008a, 0x030a, 0x008a, 0x030a, 0x0046, 0x0406, 0x0046, //
    0x0406, 0x0000, 0x008a, 0x0000, 0x0050, 0x0000, 0x008a, 0x0000, //
    0x0050, 0x0058, 0x0001, 0x0000, 0x000f, 0x0002, 0x0000, 0x0192,
];

impl FmtSvga {
    pub(super) unsafe fn init() {
        let inner_mode = ModeInfo {
            width: 640,
            height: 480,
            bytes_per_scanline: 640,
            pixel_format: PixelFormat::Indexed8,
        };
        let modes = [inner_mode].into();
        let current_mode = CurrentMode {
            current: ModeIndex(0),
            info: inner_mode,
            fb: PhysicalAddress::from_usize(0x8010_0000),
            fb_size: 512 * 1024,
        };

        let driver = Box::new(Self {
            modes,
            current_mode,
            preferred_graphics_mode: inner_mode.into(),
        });

        System::conctl().set_graphics(driver as Box<dyn GraphicsOutputDevice>);
    }
}

impl GraphicsOutputDevice for FmtSvga {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }

    fn preferred_graphics_mode(&self) -> Option<PreferredGraphicsMode> {
        Some(self.preferred_graphics_mode)
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        unsafe {
            let _mode_info = *self.modes.get(mode.0 as usize).ok_or(())?;

            Crtc::set_mode(&VIDEO_MODE_SETTINGS, 0b0000_1010, 0b0001_1000, 0b0000_1000);

            for (i, &color) in IndexedColor::COLOR_PALETTE.iter().enumerate() {
                IoPortWB(0xfd90).write(i as u8);
                IoPortWB(0xfd92).write(color as u8);
                IoPortWB(0xfd96).write((color >> 8) as u8);
                IoPortWB(0xfd94).write((color >> 16) as u8);
            }

            Cpu::zero_memory32(
                self.current_mode.fb.as_usize() as *mut u32,
                self.current_mode.fb_size / 4,
            );

            Ok(())
        }
    }

    fn detach(&mut self) {
        unsafe {
            Cpu::zero_memory32(
                self.current_mode.fb.as_usize() as *mut u32,
                self.current_mode.fb_size / 4,
            );
        }

        FmtText::handover();
    }
}
