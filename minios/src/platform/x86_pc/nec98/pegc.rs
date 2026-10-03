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

            let mut regs = Vm86Context::default();
            regs.eax = 0x4100.into();
            INT18.call(&mut regs);
            regs.eax = 0x4200.into();
            regs.ecx = 0xc000.into();
            INT18.call(&mut regs);
            regs.eax = 0x4d00.into();
            regs.ecx = 0x0100.into();
            INT18.call(&mut regs);

            (0x000e_0004 as *mut u16).write_volatile(0x0000);
            match Self::pegc_set_mode(true, true) {
                Ok(_) => {}
                Err(_addr) => {
                    // panic!("Failed to set PEGC mode at address {:x}", addr);
                    return;
                }
            }

            // Decide the framebuffer address
            let mut decided_fb = 0;
            let base0 = 0x00f0_0000 as *const u8;
            let base1 = 0xfff0_0000 as *const u8;
            for _ in 0..2 {
                for i in 0..16 {
                    let banked = (0x000a_8000 + i as usize) as *mut u8;
                    let value = banked.read_volatile() ^ i;
                    banked.write_volatile(value);

                    if base0.add(i as usize).read_volatile() == value {
                        decided_fb = base0 as usize;
                    } else if base1.add(i as usize).read_volatile() == value {
                        decided_fb = base1 as usize;
                    } else {
                        decided_fb = 0;
                        break;
                    }
                }
            }
            if decided_fb == 0 {
                // panic!("Failed to determine desired framebuffer address");
                return;
            }

            regs.eax = 0x4d00.into();
            regs.ecx = 0x0000.into();
            INT18.call(&mut regs);

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
                fb: PhysicalAddress::from_usize(decided_fb),
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

    /// Adjusts the PEGC mode.
    unsafe fn pegc_set_mode(is_packed: bool, is_framebuffer: bool) -> Result<(), usize> {
        unsafe {
            let mode_planar = 0x000e_0100 as *mut u8;
            let value_planar = if is_packed { 0 } else { 1 };
            mode_planar.write_volatile(value_planar);
            if mode_planar.read_volatile() != value_planar {
                return Err(mode_planar as usize);
            }

            let mode_framebuffer = 0x000e_0102 as *mut u16;
            let value_framebuffer = if is_framebuffer { 0x0001 } else { 0x0000 };
            mode_framebuffer.write_volatile(value_framebuffer);
            if mode_framebuffer.read_volatile() != value_framebuffer {
                return Err(mode_framebuffer as usize);
            }

            Ok(())
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

            let _ = Self::pegc_set_mode(true, true);

            regs.eax = 0x0d00.into();
            INT18.call(&mut regs);
            regs.eax = 0x4000.into();
            INT18.call(&mut regs);

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
            regs.eax = 0x4d00.into();
            regs.ecx = 0x0000.into();
            INT18.call(&mut regs);

            regs.eax = 0x3008.into();
            regs.ebx = 0x2100.into();
            INT18.call(&mut regs);
        }
        Pc98Text::handover();
    }
}
