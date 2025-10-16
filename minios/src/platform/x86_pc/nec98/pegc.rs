//! PC-9821 640x480 Graphics Mode Driver

use super::bios::INT18;
use crate::arch::vm86::{VM86, X86StackContext};
use crate::io::graphics::*;
use crate::*;
use x86::isolated_io::LoIoPortWB;

pub struct PegcBios {
    modes: Vec<ModeInfo>,
    current_mode: CurrentMode,
}

impl PegcBios {
    #[inline]
    const fn new() -> Self {
        Self {
            modes: Vec::new(),
            current_mode: CurrentMode::empty(),
        }
    }

    pub(super) unsafe fn init() {
        unsafe {
            if (0x45c as *const u8).read_volatile() & 0x40 == 0 {
                // not supported
                return;
            }

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
                fb: PhysicalAddress::from_usize(0x00f0_0000),
                fb_size: 640 * 480,
            };

            System::console_controller().set_graphics(driver as Box<dyn GraphicsOutput>);
        }
    }
}

impl GraphicsOutput for PegcBios {
    fn modes(&self) -> &[ModeInfo] {
        &self.modes
    }

    fn current_mode(&self) -> &CurrentMode {
        &self.current_mode
    }

    fn set_mode(&mut self, mode: ModeIndex) -> Result<(), ()> {
        unsafe {
            let _inner_mode = *self.modes.get(mode.0 as usize).ok_or(())?;

            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x300c);
            regs.ebx.set_d(0x3200);
            VM86::call_bios(INT18, &mut regs);
            regs.eax.set_d(0x4d00);
            regs.ecx.set_d(0x0100);
            VM86::call_bios(INT18, &mut regs);

            regs.eax.set_d(0x0d00);
            VM86::call_bios(INT18, &mut regs);
            regs.eax.set_d(0x4000);
            VM86::call_bios(INT18, &mut regs);

            (0x000e_0100 as *mut u8).write_volatile(0);
            (0x000e_0102 as *mut u8).write_volatile(1);

            for (i, &color) in COLOR_PALETTE.iter().enumerate() {
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
            let mut regs = X86StackContext::default();
            regs.eax.set_d(0x4100);
            VM86::call_bios(INT18, &mut regs);
            regs.eax.set_d(0x3008);
            regs.ebx.set_d(0x2200);
            VM86::call_bios(INT18, &mut regs);
            regs.eax.set_d(0x4d00);
            regs.ecx.set_d(0x0000);
            VM86::call_bios(INT18, &mut regs);

            regs.eax.set_d(0x0c00);
            VM86::call_bios(INT18, &mut regs);
        }
    }
}
