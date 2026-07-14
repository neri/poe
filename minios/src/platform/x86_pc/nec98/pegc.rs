//! PC-9821 640x480 Graphics Mode Driver

use x86::isolated_io::LoIoPortWB;

use super::bios::INT18;
use super::pc98_text::Pc98Text;
use crate::arch::vm86::Vm86Context;
use crate::io::graphics::color::IndexedColor;
use crate::io::graphics::*;
use crate::*;

pub struct PegcBios {
    modes: Vec<ModeInfo>,
    current_mode: CurrentMode,
    preferred_graphics_mode: PreferredGraphicsMode,
}

impl PegcBios {
    pub(super) unsafe fn init() {
        unsafe {
            if (0x45c as *const u8).read_volatile() & 0x40 == 0 {
                // not supported
                return;
            }

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
                fb: PhysicalAddress::from_usize(0x00f0_0000),
                fb_size: 640 * 480,
            };

            let driver = Box::new(Self {
                modes,
                current_mode,
                preferred_graphics_mode: inner_mode.into(),
            });

            System::conctl().set_graphics(driver as Box<dyn GraphicsOutputDevice>);
        }
    }
}

impl GraphicsOutputDevice for PegcBios {
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
            let _inner_mode = *self.modes.get(mode.0 as usize).ok_or(())?;

            let mut regs = Vm86Context::default();
            regs.eax = 0x300c.into();
            regs.ebx = 0x3200.into();
            INT18.call(&mut regs);
            regs.eax = 0x4d00.into();
            regs.ecx = 0x0100.into();
            INT18.call(&mut regs);

            regs.eax = 0x0d00.into();
            INT18.call(&mut regs);
            regs.eax = 0x4000.into();
            INT18.call(&mut regs);

            (0x000e_0100 as *mut u8).write_volatile(0x00);
            // (0x000e_0102 as *mut u8).write_volatile(0x01);
            (0x000e_0102 as *mut u16).write_volatile(0x0001);

            for (i, &color) in IndexedColor::COLOR_PALETTE.iter().enumerate() {
                LoIoPortWB::<0xa8>::new().write(i as u8);
                LoIoPortWB::<0xae>::new().write(color as u8);
                LoIoPortWB::<0xaa>::new().write((color >> 8) as u8);
                LoIoPortWB::<0xac>::new().write((color >> 16) as u8);
            }

            Ok(())
        }
    }

    fn detach(&mut self) {
        unsafe {
            let mut regs = Vm86Context::default();

            regs.eax = 0x3008.into();
            regs.ebx = 0x2100.into();
            INT18.call(&mut regs);
        }
        Pc98Text::handover();
    }
}
